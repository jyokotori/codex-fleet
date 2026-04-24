use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, Result};

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
    /// Full workspace URL, e.g. `http://192.168.14.63/magician`.
    /// Last path segment is the workspace slug; everything before is the base URL.
    pub workspace_url: String,
    pub api_key: String,
    #[serde(default)]
    pub webhook_secret: String,
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: Option<String>,
    pub workspace_url: Option<String>,
    /// Only updated when non-empty. Send empty string to keep existing secret.
    pub api_key: Option<String>,
    pub webhook_secret: Option<String>,
}

/// Split `http://host[:port]/path/slug` into (`http://host[:port]/path`, `slug`).
/// Trailing slashes are tolerated. Fails if no path segment is found.
fn parse_workspace_url(s: &str) -> std::result::Result<(String, String), String> {
    let trimmed = s.trim().trim_end_matches('/');
    // Split off scheme so the first '/' inside authority isn't misread
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
        // Allow explicit update (including to empty) when field present
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

// ── Plane Projects Proxy (scoped to workspace) ──

#[derive(Serialize, Deserialize)]
pub struct PlaneProject {
    pub id: String,
    pub name: String,
    pub identifier: String,
}

pub async fn list_workspace_projects(
    State(state): State<AppContext>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<PlaneProject>>> {
    let (base, slug, key) = load_workspace(&state.db, &workspace_id).await?;

    let url = format!("{base}/api/v1/workspaces/{slug}/projects/");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("x-api-key", &key)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Plane API error: {e}")))?;

    let body: serde_json::Value = resp
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

// ── Plane Bindings CRUD (scoped to workspace) ──

#[derive(Serialize)]
pub struct PlaneBinding {
    pub id: String,
    pub workspace_id: String,
    pub plane_project_id: String,
    pub plane_project_name: String,
    pub plane_project_identifier: String,
    pub agent_group_id: String,
    pub agent_group_name: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreatePlaneBindingRequest {
    pub plane_project_id: String,
    pub plane_project_name: String,
    #[serde(default)]
    pub plane_project_identifier: String,
    pub agent_group_id: String,
}

#[derive(Deserialize)]
pub struct UpdatePlaneBindingRequest {
    pub agent_group_id: Option<String>,
}

pub async fn list_workspace_bindings(
    State(state): State<AppContext>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<PlaneBinding>>> {
    let rows = sqlx::query(
        r#"SELECT pb.id, pb.workspace_id, pb.plane_project_id, pb.plane_project_name, pb.plane_project_identifier,
                  pb.agent_group_id, ag.name AS agent_group_name, pb.enabled, pb.created_at::text AS created_at
           FROM plane_bindings pb
           LEFT JOIN agent_groups ag ON ag.id = pb.agent_group_id
           WHERE pb.workspace_id = $1
           ORDER BY pb.created_at DESC"#,
    )
    .bind(&workspace_id)
    .fetch_all(&state.db)
    .await?;

    let bindings = rows
        .iter()
        .map(|r| PlaneBinding {
            id: r.get("id"),
            workspace_id: r.get("workspace_id"),
            plane_project_id: r.get("plane_project_id"),
            plane_project_name: r.get("plane_project_name"),
            plane_project_identifier: r.get("plane_project_identifier"),
            agent_group_id: r.get("agent_group_id"),
            agent_group_name: r.try_get("agent_group_name").unwrap_or_default(),
            enabled: r.get("enabled"),
            created_at: r.get("created_at"),
        })
        .collect();

    Ok(Json(bindings))
}

pub async fn create_workspace_binding(
    State(state): State<AppContext>,
    Path(workspace_id): Path<String>,
    Json(req): Json<CreatePlaneBindingRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let id = Uuid::new_v4().to_string();

    sqlx::query!(
        r#"INSERT INTO plane_bindings (id, workspace_id, plane_project_id, plane_project_name, plane_project_identifier, agent_group_id)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
        id,
        workspace_id,
        req.plane_project_id,
        req.plane_project_name,
        req.plane_project_identifier,
        req.agent_group_id
    )
    .execute(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": id}))))
}

pub async fn update_plane_binding(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePlaneBindingRequest>,
) -> Result<StatusCode> {
    if let Some(group_id) = req.agent_group_id {
        sqlx::query!(
            "UPDATE plane_bindings SET agent_group_id = $1 WHERE id = $2",
            group_id,
            id
        )
        .execute(&state.db)
        .await?;
    }
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
