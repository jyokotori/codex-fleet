use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, Result};

// ── Plane Projects Proxy ──

#[derive(Serialize, Deserialize)]
pub struct PlaneProject {
    pub id: String,
    pub name: String,
    pub identifier: String,
}

pub async fn list_plane_projects(
    State(state): State<AppContext>,
) -> Result<Json<Vec<PlaneProject>>> {
    let base = &state.config.plane_base_url;
    let slug = &state.config.plane_workspace_slug;
    let key = &state.config.plane_api_key;

    if base.is_empty() || key.is_empty() {
        return Err(AppError::BadRequest("Plane integration not configured".into()));
    }

    let url = format!("{base}/api/v1/workspaces/{slug}/projects/");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("x-api-key", key)
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

// ── Plane Bindings CRUD ──

#[derive(Serialize)]
pub struct PlaneBinding {
    pub id: String,
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

pub async fn list_plane_bindings(
    State(state): State<AppContext>,
) -> Result<Json<Vec<PlaneBinding>>> {
    let rows = sqlx::query(
        r#"SELECT pb.id, pb.plane_project_id, pb.plane_project_name, pb.plane_project_identifier,
                  pb.agent_group_id, ag.name AS agent_group_name, pb.enabled, pb.created_at::text AS created_at
           FROM plane_bindings pb
           LEFT JOIN agent_groups ag ON ag.id = pb.agent_group_id
           ORDER BY pb.created_at DESC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let bindings = rows
        .iter()
        .map(|r| PlaneBinding {
            id: r.get("id"),
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

pub async fn create_plane_binding(
    State(state): State<AppContext>,
    Json(req): Json<CreatePlaneBindingRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
    let id = Uuid::new_v4().to_string();

    sqlx::query!(
        r#"INSERT INTO plane_bindings (id, plane_project_id, plane_project_name, plane_project_identifier, agent_group_id)
           VALUES ($1, $2, $3, $4, $5)"#,
        id,
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
        r#"SELECT id, plane_issue_id, plane_project_id, title, description, priority,
                  assignee_email, status, agent_id, task_id, created_at, updated_at
           FROM plane_tasks ORDER BY created_at DESC LIMIT 200"#
    )
    .fetch_all(&state.db)
    .await?;

    let tasks = rows
        .into_iter()
        .map(|r| PlaneTask {
            id: r.id,
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
