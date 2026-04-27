use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use shared_kernel::{cli_is_runnable, cli_is_supported, AppContext, AppError, CliInfo, Result, SUPPORTED_CLIS};

use crate::infrastructure::plane_client::PlaneClient;

// ── Helpers ──

fn mask_secret(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    if s.len() <= 8 {
        return "••••".to_string();
    }
    format!("{}••••{}", &s[..4], &s[s.len() - 4..])
}

async fn load_workspace(db: &sqlx::PgPool, id: &str) -> Result<(String, String, String)> {
    let row = sqlx::query(
        "SELECT base_url, workspace_slug, api_key FROM plane_workspaces WHERE id = $1 AND enabled = true",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::BadRequest("Plane workspace not found or disabled".into()))?;

    Ok((row.get("base_url"), row.get("workspace_slug"), row.get("api_key")))
}

fn plane_client_for(base_url: &str, slug: &str, api_key: &str) -> PlaneClient {
    PlaneClient::new(base_url, slug, api_key)
}

// ── Plane Workspaces CRUD ──

#[derive(Serialize)]
pub struct PlaneWorkspace {
    pub id: String,
    pub name: String,
    pub workspace_url: String,
    pub api_key_masked: String,
    pub webhook_secret_masked: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub workspace_url: String,
    pub api_key: String,
    #[serde(default)]
    pub webhook_secret: String,
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub workspace_url: Option<String>,
    pub api_key: Option<String>,
    pub webhook_secret: Option<String>,
}

fn parse_workspace_url(s: &str) -> std::result::Result<(String, String), String> {
    let trimmed = s.trim().trim_end_matches('/');
    let (scheme, rest) = match trimmed.split_once("://") {
        Some(p) => p,
        None => return Err("workspace_url must start with http:// or https://".into()),
    };
    let (authority, path) = match rest.split_once('/') {
        Some(p) => p,
        None => return Err("workspace_url must include a workspace slug path".into()),
    };
    let (prefix, slug) = match path.rsplit_once('/') {
        Some((p, s)) => (p, s),
        None => ("", path),
    };
    if slug.is_empty() {
        return Err("workspace slug is empty".into());
    }
    let base = if prefix.is_empty() {
        format!("{scheme}://{authority}")
    } else {
        format!("{scheme}://{authority}/{prefix}")
    };
    Ok((base, slug.to_string()))
}

pub async fn list_workspaces(
    State(state): State<AppContext>,
) -> Result<Json<Vec<PlaneWorkspace>>> {
    let rows = sqlx::query(
        r#"SELECT id, name, base_url, workspace_slug, api_key, webhook_secret, enabled,
                  created_at::text AS created_at, updated_at::text AS updated_at
           FROM plane_workspaces ORDER BY created_at ASC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let workspaces = rows
        .iter()
        .map(|r| {
            let api_key: String = r.get("api_key");
            let webhook_secret: String = r.get("webhook_secret");
            let base_url: String = r.get("base_url");
            let slug: String = r.get("workspace_slug");
            PlaneWorkspace {
                id: r.get("id"),
                name: r.get("name"),
                workspace_url: format!("{base_url}/{slug}"),
                api_key_masked: mask_secret(&api_key),
                webhook_secret_masked: mask_secret(&webhook_secret),
                enabled: r.get("enabled"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }
        })
        .collect();

    Ok(Json(workspaces))
}

pub async fn create_workspace(
    State(state): State<AppContext>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    if req.name.trim().is_empty() || req.workspace_url.trim().is_empty() || req.api_key.trim().is_empty() {
        return Err(AppError::BadRequest("name, workspace_url, api_key are required".into()));
    }

    let (base_url, workspace_slug) = parse_workspace_url(&req.workspace_url)
        .map_err(AppError::BadRequest)?;

    let id = Uuid::new_v4().to_string();

    sqlx::query!(
        r#"INSERT INTO plane_workspaces (id, name, base_url, workspace_slug, api_key, webhook_secret)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
        id,
        req.name,
        base_url,
        workspace_slug,
        req.api_key,
        req.webhook_secret,
    )
    .execute(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": id}))))
}

pub async fn update_workspace(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<StatusCode> {
    let mut builder = sqlx::QueryBuilder::new("UPDATE plane_workspaces SET updated_at = NOW()");

    if let Some(v) = req.name.as_ref().filter(|v| !v.is_empty()) {
        builder.push(", name = ").push_bind(v);
    }
    if let Some(v) = req.workspace_url.as_ref().filter(|v| !v.is_empty()) {
        let (base_url, slug) = parse_workspace_url(v).map_err(AppError::BadRequest)?;
        builder.push(", base_url = ").push_bind(base_url);
        builder.push(", workspace_slug = ").push_bind(slug);
    }
    if let Some(v) = req.api_key.as_ref().filter(|v| !v.is_empty()) {
        builder.push(", api_key = ").push_bind(v);
    }
    if let Some(v) = req.webhook_secret.as_ref() {
        builder.push(", webhook_secret = ").push_bind(v);
    }
    builder.push(" WHERE id = ").push_bind(id);
    builder.build().execute(&state.db).await?;

    Ok(StatusCode::OK)
}

pub async fn delete_workspace(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    sqlx::query!("DELETE FROM plane_workspaces WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn toggle_workspace(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    sqlx::query!(
        "UPDATE plane_workspaces SET enabled = NOT enabled, updated_at = NOW() WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;
    Ok(StatusCode::OK)
}

// ── Plane Projects / States / Labels Proxy ──

#[derive(Serialize)]
pub struct PlaneProject {
    pub id: String,
    pub name: String,
    pub identifier: String,
}

#[derive(Serialize)]
pub struct PlaneState {
    pub id: String,
    pub name: String,
    pub group: String,
}

#[derive(Serialize)]
pub struct PlaneLabel {
    pub id: String,
    pub name: String,
    pub color: String,
}

pub async fn list_workspace_projects(
    State(state): State<AppContext>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<PlaneProject>>> {
    let (base, slug, key) = load_workspace(&state.db, &workspace_id).await?;
    let url = format!("{base}/api/v1/workspaces/{slug}/projects/");
    let client = reqwest::Client::new();
    let body: serde_json::Value = client
        .get(&url)
        .header("x-api-key", &key)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Plane API error: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Plane API parse error: {e}")))?;

    let empty = vec![];
    let results = body["results"].as_array().unwrap_or(&empty);
    let projects: Vec<PlaneProject> = results
        .iter()
        .filter_map(|p| {
            Some(PlaneProject {
                id: p["id"].as_str()?.to_string(),
                name: p["name"].as_str()?.to_string(),
                identifier: p["identifier"].as_str()?.to_string(),
            })
        })
        .collect();
    Ok(Json(projects))
}

pub async fn list_project_states(
    State(state): State<AppContext>,
    Path((workspace_id, project_id)): Path<(String, String)>,
) -> Result<Json<Vec<PlaneState>>> {
    let (base, slug, key) = load_workspace(&state.db, &workspace_id).await?;
    let url = format!("{base}/api/v1/workspaces/{slug}/projects/{project_id}/states/");
    let client = reqwest::Client::new();
    let body: serde_json::Value = client
        .get(&url)
        .header("x-api-key", &key)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Plane API error: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Plane API parse error: {e}")))?;

    let empty = vec![];
    let results = body["results"].as_array().unwrap_or(&empty);
    let states: Vec<PlaneState> = results
        .iter()
        .filter_map(|s| {
            Some(PlaneState {
                id: s["id"].as_str()?.to_string(),
                name: s["name"].as_str()?.to_string(),
                group: s["group"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();
    Ok(Json(states))
}

pub async fn list_project_labels(
    State(state): State<AppContext>,
    Path((workspace_id, project_id)): Path<(String, String)>,
) -> Result<Json<Vec<PlaneLabel>>> {
    let (base, slug, key) = load_workspace(&state.db, &workspace_id).await?;
    let url = format!("{base}/api/v1/workspaces/{slug}/projects/{project_id}/labels/");
    let client = reqwest::Client::new();
    let body: serde_json::Value = client
        .get(&url)
        .header("x-api-key", &key)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Plane API error: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Plane API parse error: {e}")))?;

    let empty = vec![];
    let results = body["results"].as_array().unwrap_or(&empty);
    let labels: Vec<PlaneLabel> = results
        .iter()
        .filter_map(|l| {
            Some(PlaneLabel {
                id: l["id"].as_str()?.to_string(),
                name: l["name"].as_str()?.to_string(),
                color: l["color"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect();
    Ok(Json(labels))
}

// ── Supported CLIs registry ──

pub async fn list_clis() -> Json<&'static [CliInfo]> {
    Json(SUPPORTED_CLIS)
}

// ── Plane Bindings CRUD (scoped to workspace) ──

#[derive(Serialize)]
pub struct PlaneBindingLabel {
    pub label_id: String,
    pub label_name: String,
    pub cli_type: String,
    pub priority: i32,
}

#[derive(Serialize)]
pub struct PlaneBinding {
    pub id: String,
    pub workspace_id: String,
    pub plane_project_id: String,
    pub plane_project_name: String,
    pub plane_project_identifier: String,
    pub agent_group_id: String,
    pub agent_group_name: String,
    pub accept_state_id: String,
    pub accept_state_name: String,
    pub in_progress_state_id: String,
    pub in_progress_state_name: String,
    pub completion_state_id: String,
    pub completion_state_name: String,
    pub labels: Vec<PlaneBindingLabel>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct PlaneBindingLabelInput {
    pub label_id: String,
    pub label_name: String,
    pub cli_type: String,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Deserialize)]
pub struct CreatePlaneBindingRequest {
    pub plane_project_id: String,
    pub plane_project_name: String,
    #[serde(default)]
    pub plane_project_identifier: String,
    pub agent_group_id: String,
    pub accept_state_id: String,
    pub accept_state_name: String,
    pub in_progress_state_id: String,
    pub in_progress_state_name: String,
    pub completion_state_id: String,
    pub completion_state_name: String,
    pub labels: Vec<PlaneBindingLabelInput>,
}

#[derive(Deserialize)]
pub struct UpdatePlaneBindingRequest {
    pub agent_group_id: Option<String>,
    pub accept_state_id: Option<String>,
    pub accept_state_name: Option<String>,
    pub in_progress_state_id: Option<String>,
    pub in_progress_state_name: Option<String>,
    pub completion_state_id: Option<String>,
    pub completion_state_name: Option<String>,
    /// When provided, completely replaces existing labels.
    pub labels: Option<Vec<PlaneBindingLabelInput>>,
}

async fn validate_binding_payload(
    db: &sqlx::PgPool,
    workspace_id: &str,
    project_id: &str,
    accept_state_id: &str,
    in_progress_state_id: &str,
    completion_state_id: &str,
    labels: &[PlaneBindingLabelInput],
) -> Result<()> {
    if labels.is_empty() {
        return Err(AppError::BadRequest("at least one label is required".into()));
    }
    let mut runnable = false;
    let mut seen_label_ids = std::collections::HashSet::new();
    let mut seen_priorities = std::collections::HashSet::new();
    for lb in labels {
        if !cli_is_supported(&lb.cli_type) {
            return Err(AppError::BadRequest(format!(
                "unsupported cli_type: {}",
                lb.cli_type
            )));
        }
        if cli_is_runnable(&lb.cli_type) {
            runnable = true;
        }
        if !seen_label_ids.insert(lb.label_id.clone()) {
            return Err(AppError::BadRequest(format!(
                "duplicate label_id: {}",
                lb.label_id
            )));
        }
        if !seen_priorities.insert(lb.priority) {
            return Err(AppError::BadRequest(format!(
                "duplicate label priority: {}",
                lb.priority
            )));
        }
    }
    if !runnable {
        return Err(AppError::BadRequest(
            "binding must include at least one non-WIP CLI label".into(),
        ));
    }
    // Validate states + labels exist in Plane
    let (base, slug, key) = load_workspace(db, workspace_id).await?;
    let client = plane_client_for(&base, &slug, &key);
    let states = client
        .get_states(project_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to fetch states: {e}")))?;
    let mut state_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for id in states.values() {
        state_ids.insert(id.as_str());
    }
    for sid in [accept_state_id, in_progress_state_id, completion_state_id] {
        if !state_ids.contains(sid) {
            return Err(AppError::BadRequest(format!(
                "state_id not found in project: {sid}"
            )));
        }
    }
    let labels_map = client
        .get_labels(project_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to fetch labels: {e}")))?;
    for lb in labels {
        if !labels_map.contains_key(&lb.label_id) {
            return Err(AppError::BadRequest(format!(
                "label_id not found in project: {}",
                lb.label_id
            )));
        }
    }
    Ok(())
}

async fn fetch_binding_labels(
    db: &sqlx::PgPool,
    binding_id: &str,
) -> Result<Vec<PlaneBindingLabel>> {
    let rows = sqlx::query!(
        r#"SELECT label_id, label_name, cli_type, priority
           FROM plane_binding_labels
           WHERE binding_id = $1
           ORDER BY priority ASC, label_name ASC"#,
        binding_id
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PlaneBindingLabel {
            label_id: r.label_id,
            label_name: r.label_name,
            cli_type: r.cli_type,
            priority: r.priority,
        })
        .collect())
}

pub async fn list_workspace_bindings(
    State(state): State<AppContext>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<PlaneBinding>>> {
    let rows = sqlx::query(
        r#"SELECT pb.id, pb.workspace_id, pb.plane_project_id, pb.plane_project_name, pb.plane_project_identifier,
                  pb.agent_group_id, ag.name AS agent_group_name,
                  pb.accept_state_id, pb.accept_state_name,
                  pb.in_progress_state_id, pb.in_progress_state_name,
                  pb.completion_state_id, pb.completion_state_name,
                  pb.enabled, pb.created_at::text AS created_at
           FROM plane_bindings pb
           LEFT JOIN agent_groups ag ON ag.id = pb.agent_group_id
           WHERE pb.workspace_id = $1
           ORDER BY pb.created_at DESC"#,
    )
    .bind(&workspace_id)
    .fetch_all(&state.db)
    .await?;

    let mut bindings = Vec::with_capacity(rows.len());
    for r in &rows {
        let id: String = r.get("id");
        let labels = fetch_binding_labels(&state.db, &id).await?;
        bindings.push(PlaneBinding {
            id: id.clone(),
            workspace_id: r.get("workspace_id"),
            plane_project_id: r.get("plane_project_id"),
            plane_project_name: r.get("plane_project_name"),
            plane_project_identifier: r.get("plane_project_identifier"),
            agent_group_id: r.get("agent_group_id"),
            agent_group_name: r.try_get("agent_group_name").unwrap_or_default(),
            accept_state_id: r.get("accept_state_id"),
            accept_state_name: r.get("accept_state_name"),
            in_progress_state_id: r.get("in_progress_state_id"),
            in_progress_state_name: r.get("in_progress_state_name"),
            completion_state_id: r.get("completion_state_id"),
            completion_state_name: r.get("completion_state_name"),
            labels,
            enabled: r.get("enabled"),
            created_at: r.get("created_at"),
        });
    }

    Ok(Json(bindings))
}

pub async fn create_workspace_binding(
    State(state): State<AppContext>,
    Path(workspace_id): Path<String>,
    Json(req): Json<CreatePlaneBindingRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    validate_binding_payload(
        &state.db,
        &workspace_id,
        &req.plane_project_id,
        &req.accept_state_id,
        &req.in_progress_state_id,
        &req.completion_state_id,
        &req.labels,
    )
    .await?;

    let id = Uuid::new_v4().to_string();
    let mut tx = state.db.begin().await?;

    sqlx::query!(
        r#"INSERT INTO plane_bindings (
              id, workspace_id, plane_project_id, plane_project_name, plane_project_identifier,
              agent_group_id,
              accept_state_id, accept_state_name,
              in_progress_state_id, in_progress_state_name,
              completion_state_id, completion_state_name)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
        id,
        workspace_id,
        req.plane_project_id,
        req.plane_project_name,
        req.plane_project_identifier,
        req.agent_group_id,
        req.accept_state_id,
        req.accept_state_name,
        req.in_progress_state_id,
        req.in_progress_state_name,
        req.completion_state_id,
        req.completion_state_name,
    )
    .execute(&mut *tx)
    .await?;

    for lb in &req.labels {
        let lid = Uuid::new_v4().to_string();
        sqlx::query!(
            r#"INSERT INTO plane_binding_labels (id, binding_id, label_id, label_name, cli_type, priority)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
            lid,
            id,
            lb.label_id,
            lb.label_name,
            lb.cli_type,
            lb.priority,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": id}))))
}

pub async fn update_plane_binding(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePlaneBindingRequest>,
) -> Result<StatusCode> {
    // Load existing binding to know workspace_id + project_id for validation if labels/states change
    let existing = sqlx::query!(
        r#"SELECT workspace_id, plane_project_id,
                  accept_state_id, in_progress_state_id, completion_state_id
           FROM plane_bindings WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::BadRequest("binding not found".into()))?;

    // If states or labels included, run full validation against Plane
    let needs_validation = req.accept_state_id.is_some()
        || req.in_progress_state_id.is_some()
        || req.completion_state_id.is_some()
        || req.labels.is_some();
    if needs_validation {
        let accept_id = req
            .accept_state_id
            .clone()
            .unwrap_or_else(|| existing.accept_state_id.clone());
        let inprog_id = req
            .in_progress_state_id
            .clone()
            .unwrap_or_else(|| existing.in_progress_state_id.clone());
        let comp_id = req
            .completion_state_id
            .clone()
            .unwrap_or_else(|| existing.completion_state_id.clone());
        let labels = req
            .labels
            .as_deref()
            .ok_or_else(|| AppError::BadRequest("labels must be provided when updating states".into()))
            .ok();
        // If labels were not in the request, fetch existing for runnable-cli validation only.
        if let Some(labels) = labels {
            validate_binding_payload(
                &state.db,
                &existing.workspace_id,
                &existing.plane_project_id,
                &accept_id,
                &inprog_id,
                &comp_id,
                labels,
            )
            .await?;
        } else {
            // No labels in payload — only validate state ids
            let (base, slug, key) = load_workspace(&state.db, &existing.workspace_id).await?;
            let client = plane_client_for(&base, &slug, &key);
            let states = client
                .get_states(&existing.plane_project_id)
                .await
                .map_err(|e| AppError::BadRequest(format!("failed to fetch states: {e}")))?;
            let ids: std::collections::HashSet<&str> = states.values().map(|s| s.as_str()).collect();
            for sid in [&accept_id, &inprog_id, &comp_id] {
                if !ids.contains(sid.as_str()) {
                    return Err(AppError::BadRequest(format!(
                        "state_id not found in project: {sid}"
                    )));
                }
            }
        }
    }

    let mut tx = state.db.begin().await?;

    let mut builder = sqlx::QueryBuilder::new("UPDATE plane_bindings SET ");
    let mut first = true;
    let push_pair = |b: &mut sqlx::QueryBuilder<sqlx::Postgres>, col: &str, val: &str, first: &mut bool| {
        if !*first {
            b.push(", ");
        }
        b.push(col);
        b.push(" = ");
        b.push_bind(val.to_string());
        *first = false;
    };

    if let Some(v) = &req.agent_group_id {
        push_pair(&mut builder, "agent_group_id", v, &mut first);
    }
    if let Some(v) = &req.accept_state_id {
        push_pair(&mut builder, "accept_state_id", v, &mut first);
    }
    if let Some(v) = &req.accept_state_name {
        push_pair(&mut builder, "accept_state_name", v, &mut first);
    }
    if let Some(v) = &req.in_progress_state_id {
        push_pair(&mut builder, "in_progress_state_id", v, &mut first);
    }
    if let Some(v) = &req.in_progress_state_name {
        push_pair(&mut builder, "in_progress_state_name", v, &mut first);
    }
    if let Some(v) = &req.completion_state_id {
        push_pair(&mut builder, "completion_state_id", v, &mut first);
    }
    if let Some(v) = &req.completion_state_name {
        push_pair(&mut builder, "completion_state_name", v, &mut first);
    }

    if !first {
        builder.push(" WHERE id = ").push_bind(id.clone());
        builder.build().execute(&mut *tx).await?;
    }

    if let Some(labels) = &req.labels {
        sqlx::query!("DELETE FROM plane_binding_labels WHERE binding_id = $1", id)
            .execute(&mut *tx)
            .await?;
        for lb in labels {
            let lid = Uuid::new_v4().to_string();
            sqlx::query!(
                r#"INSERT INTO plane_binding_labels (id, binding_id, label_id, label_name, cli_type, priority)
                   VALUES ($1, $2, $3, $4, $5, $6)"#,
                lid,
                id,
                lb.label_id,
                lb.label_name,
                lb.cli_type,
                lb.priority,
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(StatusCode::OK)
}

pub async fn delete_plane_binding(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    sqlx::query!("DELETE FROM plane_bindings WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn toggle_plane_binding(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    sqlx::query!(
        "UPDATE plane_bindings SET enabled = NOT enabled WHERE id = $1",
        id
    )
    .execute(&state.db)
    .await?;
    Ok(StatusCode::OK)
}

// ── Plane Task Queue Viewer ──

#[derive(Serialize)]
pub struct PlaneTask {
    pub id: String,
    pub workspace_id: String,
    pub plane_issue_id: String,
    pub plane_project_id: String,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub assignee_email: String,
    pub status: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_plane_tasks(
    State(state): State<AppContext>,
) -> Result<Json<Vec<PlaneTask>>> {
    let rows = sqlx::query!(
        r#"SELECT id, workspace_id, plane_issue_id, plane_project_id, title, description, priority,
                  assignee_email, status, agent_id, task_id, created_at, updated_at
           FROM plane_tasks ORDER BY created_at DESC LIMIT 200"#
    )
    .fetch_all(&state.db)
    .await?;

    let tasks = rows
        .into_iter()
        .map(|r| PlaneTask {
            id: r.id,
            workspace_id: r.workspace_id,
            plane_issue_id: r.plane_issue_id,
            plane_project_id: r.plane_project_id,
            title: r.title,
            description: r.description,
            priority: r.priority,
            assignee_email: r.assignee_email,
            status: r.status,
            agent_id: r.agent_id,
            task_id: r.task_id,
            created_at: r.created_at.to_string(),
            updated_at: r.updated_at.to_string(),
        })
        .collect();

    Ok(Json(tasks))
}
