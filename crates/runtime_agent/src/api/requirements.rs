use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, Result};

// ── Response types ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct WorkItem {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub r#type: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub assigned_agent_id: Option<String>,
    pub assigned_user_id: Option<String>,
    pub execution_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ── Request types ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateWorkItemRequest {
    pub parent_id: Option<String>,
    pub r#type: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub assigned_user_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateWorkItemRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub status: Option<String>,
    pub assigned_agent_id: Option<String>,
    pub assigned_user_id: Option<String>,
    pub execution_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ListWorkItemsQuery {
    pub status: Option<String>,
    pub r#type: Option<String>,
}

// ── Project handlers ───────────────────────────────────────────────────────

pub async fn list_projects(State(state): State<AppContext>) -> Result<Json<Vec<Project>>> {
    let rows = sqlx::query!(
        "SELECT id, name, description, status, created_at, updated_at FROM projects ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| Project {
                id: r.id,
                name: r.name,
                description: r.description,
                status: r.status,
                created_at: r.created_at.to_string(),
                updated_at: r.updated_at.to_string(),
            })
            .collect(),
    ))
}

pub async fn create_project(
    State(state): State<AppContext>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Project>)> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Project name cannot be empty".into()));
    }
    let id = Uuid::new_v4().to_string();
    let description = req.description.unwrap_or_default();
    let now = Utc::now();

    sqlx::query!(
        "INSERT INTO projects (id, name, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)",
        id, req.name, description, now, now
    )
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(Project {
            id,
            name: req.name,
            description,
            status: "active".into(),
            created_at: now.to_string(),
            updated_at: now.to_string(),
        }),
    ))
}

pub async fn get_project(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<Project>> {
    let row = sqlx::query!(
        "SELECT id, name, description, status, created_at, updated_at FROM projects WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Project {} not found", id)))?;

    Ok(Json(Project {
        id: row.id,
        name: row.name,
        description: row.description,
        status: row.status,
        created_at: row.created_at.to_string(),
        updated_at: row.updated_at.to_string(),
    }))
}

pub async fn update_project(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<Json<Project>> {
    let row = sqlx::query!(
        "SELECT id, name, description, status, created_at, updated_at FROM projects WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Project {} not found", id)))?;

    let name = req.name.unwrap_or(row.name);
    let description = req.description.unwrap_or(row.description);
    let status = req.status.unwrap_or(row.status);
    let now = Utc::now();

    sqlx::query!(
        "UPDATE projects SET name=$1, description=$2, status=$3, updated_at=$4 WHERE id=$5",
        name, description, status, now, id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(Project {
        id,
        name,
        description,
        status,
        created_at: row.created_at.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn delete_project(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM projects WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Project {} not found", id)));
    }

    Ok(Json(serde_json::json!({ "message": "Project deleted" })))
}

// ── Work item handlers ─────────────────────────────────────────────────────

pub async fn list_work_items(
    State(state): State<AppContext>,
    Path(project_id): Path<String>,
    Query(params): Query<ListWorkItemsQuery>,
) -> Result<Json<Vec<WorkItem>>> {
    let _ = sqlx::query!("SELECT id FROM projects WHERE id = $1", project_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", project_id)))?;

    let rows = sqlx::query!(
        r#"SELECT id, project_id, parent_id, type as "type", title, description, status, priority,
           assigned_agent_id, assigned_user_id, execution_id, created_at, updated_at
           FROM work_items
           WHERE project_id = $1
             AND ($2::text IS NULL OR status = $2)
             AND ($3::text IS NULL OR type = $3)
           ORDER BY created_at ASC"#,
        project_id,
        params.status,
        params.r#type,
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| WorkItem {
                id: r.id,
                project_id: r.project_id,
                parent_id: r.parent_id,
                r#type: r.r#type,
                title: r.title,
                description: r.description,
                status: r.status,
                priority: r.priority,
                assigned_agent_id: r.assigned_agent_id,
                assigned_user_id: r.assigned_user_id,
                execution_id: r.execution_id,
                created_at: r.created_at.to_string(),
                updated_at: r.updated_at.to_string(),
            })
            .collect(),
    ))
}

pub async fn create_work_item(
    State(state): State<AppContext>,
    Path(project_id): Path<String>,
    Json(req): Json<CreateWorkItemRequest>,
) -> Result<(StatusCode, Json<WorkItem>)> {
    let _ = sqlx::query!("SELECT id FROM projects WHERE id = $1", project_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project {} not found", project_id)))?;

    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("Title cannot be empty".into()));
    }

    let id = Uuid::new_v4().to_string();
    let description = req.description.unwrap_or_default();
    let priority = req.priority.unwrap_or_else(|| "medium".into());
    let now = Utc::now();

    sqlx::query!(
        r#"INSERT INTO work_items (id, project_id, parent_id, type, title, description, priority, assigned_user_id, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        id, project_id, req.parent_id, req.r#type, req.title, description, priority, req.assigned_user_id, now, now
    )
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(WorkItem {
            id,
            project_id,
            parent_id: req.parent_id,
            r#type: req.r#type,
            title: req.title,
            description,
            status: "open".into(),
            priority,
            assigned_agent_id: None,
            assigned_user_id: req.assigned_user_id,
            execution_id: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        }),
    ))
}

pub async fn get_work_item(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<WorkItem>> {
    let row = sqlx::query!(
        r#"SELECT id, project_id, parent_id, type as "type", title, description, status, priority,
           assigned_agent_id, assigned_user_id, execution_id, created_at, updated_at
           FROM work_items WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Work item {} not found", id)))?;

    Ok(Json(WorkItem {
        id: row.id,
        project_id: row.project_id,
        parent_id: row.parent_id,
        r#type: row.r#type,
        title: row.title,
        description: row.description,
        status: row.status,
        priority: row.priority,
        assigned_agent_id: row.assigned_agent_id,
        assigned_user_id: row.assigned_user_id,
        execution_id: row.execution_id,
        created_at: row.created_at.to_string(),
        updated_at: row.updated_at.to_string(),
    }))
}

pub async fn update_work_item(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkItemRequest>,
) -> Result<Json<WorkItem>> {
    let row = sqlx::query!(
        r#"SELECT id, project_id, parent_id, type as "type", title, description, status, priority,
           assigned_agent_id, assigned_user_id, execution_id, created_at, updated_at
           FROM work_items WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Work item {} not found", id)))?;

    let title = req.title.unwrap_or(row.title);
    let description = req.description.unwrap_or(row.description);
    let priority = req.priority.unwrap_or(row.priority);
    let status = req.status.unwrap_or(row.status);
    // For optional FK fields: Some(value) = set, Some("") = clear, None = keep existing
    let assigned_agent_id = match req.assigned_agent_id {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v),
        None => row.assigned_agent_id,
    };
    let assigned_user_id = match req.assigned_user_id {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v),
        None => row.assigned_user_id,
    };
    let execution_id = match req.execution_id {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v),
        None => row.execution_id,
    };
    let now = Utc::now();

    sqlx::query!(
        r#"UPDATE work_items SET title=$1, description=$2, priority=$3, status=$4,
           assigned_agent_id=$5, assigned_user_id=$6, execution_id=$7, updated_at=$8 WHERE id=$9"#,
        title, description, priority, status, assigned_agent_id, assigned_user_id, execution_id, now, id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(WorkItem {
        id,
        project_id: row.project_id,
        parent_id: row.parent_id,
        r#type: row.r#type,
        title,
        description,
        status,
        priority,
        assigned_agent_id,
        assigned_user_id,
        execution_id,
        created_at: row.created_at.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn delete_work_item(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM work_items WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Work item {} not found", id)));
    }

    Ok(Json(serde_json::json!({ "message": "Work item deleted" })))
}
