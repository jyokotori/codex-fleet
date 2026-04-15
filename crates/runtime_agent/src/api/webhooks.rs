use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use shared_kernel::AppContext;
use tracing::{info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Verify the Plane webhook signature.
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
        warn!("Plane webhook signature mismatch: expected={expected}, got={signature}");
        false
    }
}

/// Receive Plane webhook events.
/// Only processes work items moved to "Todo" state → inserts into plane_tasks queue.
pub async fn plane_webhook(
    State(state): State<AppContext>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if !verify_signature(&state.config.plane_webhook_secret, &body, &headers) {
        return StatusCode::UNAUTHORIZED;
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            warn!("Plane webhook invalid JSON: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    // Only handle issue update events where state changed
    let event = payload["event"].as_str().unwrap_or_default();
    let action = payload["action"].as_str().unwrap_or_default();
    let field = payload["activity"]["field"].as_str().unwrap_or_default();

    if event != "issue" || action != "updated" || field != "state_id" {
        return StatusCode::OK;
    }

    let state_name = payload["data"]["state"]["name"].as_str().unwrap_or_default();
    if state_name != "Todo" {
        return StatusCode::OK;
    }

    // Extract issue data
    let issue_id = match payload["data"]["id"].as_str() {
        Some(id) => id,
        None => {
            warn!("Plane webhook: missing data.id");
            return StatusCode::OK;
        }
    };
    let project_id = payload["data"]["project"].as_str().unwrap_or_default();
    let title = payload["data"]["name"].as_str().unwrap_or_default();
    let description = payload["data"]["description_stripped"].as_str().unwrap_or_default();
    let priority = payload["data"]["priority"].as_str().unwrap_or("none");

    // Get assignee email (first assignee)
    let assignees = payload["data"]["assignees"].as_array();
    let assignee_email = match assignees {
        Some(arr) if arr.len() > 1 => {
            warn!("Plane webhook: issue {issue_id} has {} assignees, using first", arr.len());
            arr[0]["email"].as_str().unwrap_or_default()
        }
        Some(arr) if arr.len() == 1 => {
            arr[0]["email"].as_str().unwrap_or_default()
        }
        _ => {
            warn!("Plane webhook: issue {issue_id} has no assignees, will match any agent in group");
            ""
        }
    };

    // Insert into plane_tasks queue (no dedup — same issue can re-enter after Review Failed)
    let id = Uuid::new_v4().to_string();
    match sqlx::query!(
        r#"INSERT INTO plane_tasks (id, plane_issue_id, plane_project_id, title, description, priority, assignee_email, status)
           VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')"#,
        id,
        issue_id,
        project_id,
        title,
        description,
        priority,
        assignee_email,
    )
    .execute(&state.db)
    .await
    {
        Ok(_) => {
            info!(
                "Plane webhook: queued issue '{}' ({}) as plane_task {} [assignee={}]",
                title, issue_id, id, assignee_email
            );
        }
        Err(e) => {
            warn!("Plane webhook: failed to insert plane_task for issue {issue_id}: {e}");
        }
    }

    StatusCode::OK
}
