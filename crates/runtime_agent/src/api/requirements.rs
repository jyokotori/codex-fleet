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
    pub assigned_username: String,
    pub execution_id: Option<String>,
    pub notification_ids: String,
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
    pub assigned_agent_id: Option<String>,
    pub assigned_user_id: Option<String>,
    pub notification_ids: Option<Vec<String>>,
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
    pub notification_ids: Option<Vec<String>>,
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
           assigned_agent_id, assigned_user_id, assigned_username, execution_id, notification_ids, created_at, updated_at
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
                assigned_username: r.assigned_username,
                execution_id: r.execution_id,
                notification_ids: r.notification_ids,
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

    let status = "waiting";
    let notification_ids_json = req.notification_ids.as_ref()
        .map(|ids| serde_json::to_string(ids).unwrap_or_else(|_| "[]".into()))
        .unwrap_or_else(|| "[]".into());

    sqlx::query!(
        r#"INSERT INTO work_items (id, project_id, parent_id, type, title, description, status, priority, assigned_agent_id, assigned_user_id, notification_ids, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
        id, project_id, req.parent_id, req.r#type, req.title, description, status, priority, req.assigned_agent_id, req.assigned_user_id, notification_ids_json, now, now
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
            status: status.into(),
            priority,
            assigned_agent_id: req.assigned_agent_id,
            assigned_user_id: req.assigned_user_id,
            assigned_username: String::new(),
            execution_id: None,
            notification_ids: notification_ids_json,
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
           assigned_agent_id, assigned_user_id, assigned_username, execution_id, notification_ids, created_at, updated_at
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
        assigned_username: row.assigned_username,
        execution_id: row.execution_id,
        notification_ids: row.notification_ids,
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
           assigned_agent_id, assigned_user_id, assigned_username, execution_id, notification_ids, created_at, updated_at
           FROM work_items WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Work item {} not found", id)))?;

    let title = req.title.unwrap_or(row.title);
    let description = req.description.unwrap_or(row.description);
    let priority = req.priority.unwrap_or(row.priority);
    let old_status = row.status.clone();
    let new_status = req.status.unwrap_or(row.status);

    // Validate status transition
    if new_status != old_status {
        let valid = match (old_status.as_str(), new_status.as_str()) {
            // Manual transitions allowed from API
            ("waiting", "closed") => true,
            ("waiting", "cancelled") => true,
            ("agent_in_progress", "closed") => true,
            ("agent_in_progress", "cancelled") => true,
            ("agent_completed", "human_approved") => true,
            ("agent_completed", "human_rejected") => true,
            ("agent_failed", "waiting") => true,
            ("agent_failed", "closed") => true,
            ("human_rejected", "waiting") => true,
            ("human_approved", "closed") => true,
            // Scheduler-only transitions: reject from API
            ("waiting", "agent_in_progress")
            | ("agent_in_progress", "agent_completed")
            | ("agent_in_progress", "agent_failed") => false,
            _ => false,
        };
        if !valid {
            return Err(AppError::BadRequest(format!(
                "Invalid status transition: {} -> {}", old_status, new_status
            )));
        }
    }

    let status = new_status;

    // For optional FK fields: Some(value) = set, Some("") = clear, None = keep existing
    let old_agent_id = row.assigned_agent_id.clone();
    let assigned_agent_id = match req.assigned_agent_id {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v),
        None => row.assigned_agent_id,
    };

    // Block agent change during agent_in_progress
    if old_status == "agent_in_progress" {
        let old_agent = old_agent_id.as_deref().unwrap_or("");
        let new_agent = assigned_agent_id.as_deref().unwrap_or("");
        if old_agent != new_agent {
            return Err(AppError::BadRequest(
                "Cannot change assigned agent while work item is in progress".into(),
            ));
        }
    }

    let assigned_user_id = match req.assigned_user_id {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v),
        None => row.assigned_user_id,
    };

    // When re-queuing from human_rejected → waiting, clear execution_id
    let execution_id = if old_status == "human_rejected" && status == "waiting" {
        None
    } else {
        match req.execution_id {
            Some(v) if v.is_empty() => None,
            Some(v) => Some(v),
            None => row.execution_id,
        }
    };
    let notification_ids = match req.notification_ids {
        Some(ids) => serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into()),
        None => row.notification_ids,
    };
    let now = Utc::now();

    sqlx::query!(
        r#"UPDATE work_items SET title=$1, description=$2, priority=$3, status=$4,
           assigned_agent_id=$5, assigned_user_id=$6, execution_id=$7, notification_ids=$8, updated_at=$9 WHERE id=$10"#,
        title, description, priority, status, assigned_agent_id, assigned_user_id, execution_id, notification_ids, now, id
    )
    .execute(&state.db)
    .await?;

    // Sync task status when work_item is approved/rejected
    if matches!(status.as_str(), "human_approved" | "human_rejected") {
        if let Some(ref exec_id) = execution_id {
            sqlx::query!(
                "UPDATE tasks SET status = $1 WHERE id = $2",
                status,
                exec_id
            )
            .execute(&state.db)
            .await?;

            // Send webhook notifications for the linked task
            if let Ok(Some(task_row)) = sqlx::query!(
                "SELECT notification_ids FROM tasks WHERE id = $1",
                exec_id
            ).fetch_optional(&state.db).await {
                let task_notif_ids: Vec<String> = serde_json::from_str(&task_row.notification_ids).unwrap_or_default();
                if !task_notif_ids.is_empty() {
                    let status_clone = status.clone();
                    let payload = serde_json::json!({
                        "event": &status,
                        "task": {
                            "id": exec_id,
                            "status": &status,
                        },
                        "work_item": {
                            "id": &id,
                            "project_id": &row.project_id,
                            "title": &title,
                            "status": &status,
                            "priority": &priority,
                            "assigned_agent_id": &assigned_agent_id,
                        }
                    });
                    let db = state.db.clone();
                    tokio::spawn(async move {
                        shared_kernel::send_task_notification(&db, &task_notif_ids, &status_clone, payload).await;
                    });
                }
            }
        }
    }

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
        assigned_username: row.assigned_username,
        execution_id,
        notification_ids,
        created_at: row.created_at.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn get_work_item_by_execution(
    State(state): State<AppContext>,
    Path(execution_id): Path<String>,
) -> Result<Json<WorkItem>> {
    let row = sqlx::query!(
        r#"SELECT id, project_id, parent_id, type as "type", title, description, status, priority,
           assigned_agent_id, assigned_user_id, assigned_username, execution_id, notification_ids, created_at, updated_at
           FROM work_items WHERE execution_id = $1"#,
        execution_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Work item with execution {} not found", execution_id)))?;

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
        assigned_username: row.assigned_username,
        execution_id: row.execution_id,
        notification_ids: row.notification_ids,
        created_at: row.created_at.to_string(),
        updated_at: row.updated_at.to_string(),
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
