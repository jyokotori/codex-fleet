use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use shared_kernel::AppContext;
use sqlx::Row;
use std::collections::HashSet;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::infrastructure::plane_client::PlaneClient;

type HmacSha256 = Hmac<Sha256>;

fn verify_signature(secret: &str, body: &[u8], headers: &HeaderMap) -> bool {
    if secret.is_empty() {
        return true;
    }
    let signature = match headers.get("x-plane-signature") {
        Some(v) => v.to_str().unwrap_or_default().to_string(),
        None => {
            warn!("Plane webhook missing x-plane-signature header");
            return false;
        }
    };
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    if expected == signature {
        true
    } else {
        warn!("Plane webhook signature mismatch");
        false
    }
}

/// Receive Plane webhook events for a specific workspace.
///
/// Filtering pipeline (any miss → 200 OK + ignore):
/// 1. Workspace exists and is enabled
/// 2. Signature verified
/// 3. Event is an issue state update
/// 4. A binding exists for (workspace_id, project_id) and is enabled
/// 5. New state_id == binding.accept_state_id
/// 6. Issue has at least one assignee with a resolvable email
/// 7. Issue's labels intersect with binding's bound labels
///
/// Dedup is handled by the partial unique index `plane_tasks_active_uq`
/// over `(workspace_id, plane_issue_id) WHERE status IN ('pending','dispatched')`.
pub async fn plane_webhook(
    State(state): State<AppContext>,
    Path(workspace_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    // Look up workspace
    let row = match sqlx::query(
        "SELECT base_url, workspace_slug, api_key, webhook_secret, enabled FROM plane_workspaces WHERE id = $1",
    )
    .bind(&workspace_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Plane webhook: db error looking up workspace {workspace_id}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    let (base_url, slug, api_key, webhook_secret, enabled): (String, String, String, String, bool) =
        match row {
            Some(r) => (
                r.get("base_url"),
                r.get("workspace_slug"),
                r.get("api_key"),
                r.get("webhook_secret"),
                r.get("enabled"),
            ),
            None => {
                warn!("Plane webhook: unknown workspace_id={workspace_id}");
                return StatusCode::NOT_FOUND;
            }
        };

    if !enabled {
        return StatusCode::OK;
    }

    if !verify_signature(&webhook_secret, &body, &headers) {
        return StatusCode::UNAUTHORIZED;
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            warn!("Plane webhook invalid JSON: {e}");
            return StatusCode::OK;
        }
    };

    debug!(
        "Plane webhook [{workspace_id}] payload: {}",
        serde_json::to_string(&payload).unwrap_or_default()
    );

    let event = payload["event"].as_str().unwrap_or_default();
    let action = payload["action"].as_str().unwrap_or_default();

    // Plane sends `event=issue` with `action` ∈ {created, updated}.
    // For `updated` events `activity.field` is the changed field; for `created`
    // it's null. We accept both — the state_id check below is the real gate.
    if event != "issue" || (action != "created" && action != "updated") {
        debug!(
            "Plane webhook [{workspace_id}]: ignoring event/action = '{event}'/'{action}'"
        );
        return StatusCode::OK;
    }

    // Extract issue id, project id, state id
    let issue_id = match payload["data"]["id"].as_str() {
        Some(id) => id.to_string(),
        None => {
            debug!("Plane webhook [{workspace_id}]: missing data.id");
            return StatusCode::OK;
        }
    };
    let project_id = payload["data"]["project"].as_str().unwrap_or_default().to_string();
    if project_id.is_empty() {
        debug!("Plane webhook [{workspace_id}]: missing data.project");
        return StatusCode::OK;
    }

    // The new state id may live under data.state (string id) or under data.state.id
    let new_state_id = payload["data"]["state"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| payload["data"]["state"]["id"].as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    // Look up the binding for this project. Must exist + enabled.
    let binding = match sqlx::query!(
        r#"SELECT id, accept_state_id, agent_group_id
           FROM plane_bindings
           WHERE workspace_id = $1 AND plane_project_id = $2 AND enabled = TRUE"#,
        workspace_id,
        project_id,
    )
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(b)) => b,
        Ok(None) => {
            debug!(
                "Plane webhook [{workspace_id}]: no enabled binding for project={project_id}"
            );
            return StatusCode::OK;
        }
        Err(e) => {
            warn!("Plane webhook: db error looking up binding: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    if new_state_id != binding.accept_state_id {
        debug!(
            "Plane webhook [{workspace_id}]: state mismatch issue={issue_id} got='{new_state_id}' want='{}'",
            binding.accept_state_id
        );
        return StatusCode::OK;
    }

    // Bound labels
    let bound_labels = match sqlx::query!(
        r#"SELECT label_id FROM plane_binding_labels WHERE binding_id = $1"#,
        binding.id,
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|r| r.label_id)
            .collect::<HashSet<_>>(),
        Err(e) => {
            warn!("Plane webhook: db error looking up binding labels: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    if bound_labels.is_empty() {
        debug!("Plane webhook [{workspace_id}]: binding has no labels configured");
        return StatusCode::OK;
    }

    // Extract issue label ids from the webhook payload. Plane sends labels as
    // an array of objects {id, name, color}; older docs / events may send raw
    // id strings — accept both.
    let issue_label_ids: HashSet<String> = payload["data"]["labels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(String::from)
                        .or_else(|| v["id"].as_str().map(String::from))
                })
                .collect()
        })
        .unwrap_or_default();

    if bound_labels.is_disjoint(&issue_label_ids) {
        debug!(
            "Plane webhook [{workspace_id}]: label intersection empty issue={issue_id} issue_labels={:?} bound_labels={:?}",
            issue_label_ids, bound_labels
        );
        return StatusCode::OK;
    }

    // Plane sends assignees as an array of objects {id, display_name, ...} or
    // sometimes raw user_id strings — accept both.
    let assignee_user_ids: Vec<String> = payload["data"]["assignees"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(String::from)
                        .or_else(|| v["id"].as_str().map(String::from))
                })
                .collect()
        })
        .unwrap_or_default();

    if assignee_user_ids.is_empty() {
        debug!(
            "Plane webhook [{workspace_id}]: no assignees on issue={issue_id} (raw assignees field = {})",
            payload["data"]["assignees"]
        );
        return StatusCode::OK;
    }

    // Resolve first assignee user_id → email via Plane workspace members.
    let plane_client = PlaneClient::new(&base_url, &slug, &api_key);
    let mut assignee_email = String::new();
    for uid in &assignee_user_ids {
        match plane_client.member_email(uid).await {
            Ok(e) if !e.is_empty() => {
                assignee_email = e;
                break;
            }
            Ok(_) => continue,
            Err(e) => {
                warn!("Plane webhook: member lookup failed for {uid}: {e}");
            }
        }
    }
    if assignee_email.is_empty() {
        return StatusCode::OK;
    }

    let title = payload["data"]["name"].as_str().unwrap_or_default();
    let description = payload["data"]["description_stripped"]
        .as_str()
        .unwrap_or_default();
    let priority = payload["data"]["priority"].as_str().unwrap_or("none");

    // Insert with partial-unique-index dedup. rows_affected==0 means already
    // queued or in-flight.
    let id = Uuid::new_v4().to_string();
    let res = sqlx::query!(
        r#"INSERT INTO plane_tasks (id, workspace_id, plane_issue_id, plane_project_id, title, description, priority, assignee_email, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending')
           ON CONFLICT DO NOTHING"#,
        id,
        workspace_id,
        issue_id,
        project_id,
        title,
        description,
        priority,
        assignee_email,
    )
    .execute(&state.db)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => {
            info!(
                "Plane webhook [{workspace_id}]: queued issue '{title}' ({issue_id}) [assignee={assignee_email}]",
            );
        }
        Ok(_) => {
            info!(
                "Plane webhook [{workspace_id}]: issue {issue_id} already queued or in-flight, skipped"
            );
        }
        Err(e) => {
            warn!("Plane webhook: insert failed for issue {issue_id}: {e}");
        }
    }

    StatusCode::OK
}
