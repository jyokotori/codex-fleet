use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api::agents::get_executor,
    error::{AppError, Result},
    AppState,
};

#[derive(Serialize)]
pub struct Task {
    pub id: String,
    pub agent_id: String,
    pub description: String,
    pub status: String,
    pub task_dir: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub description: String,
}

/// Compute date-partitioned task directory path (codex only).
fn task_dir_path(use_docker: bool, agent_id: &str, task_id: &str, now: &chrono::DateTime<Utc>) -> String {
    let date_part = format!("{}/{}/{}", now.format("%Y"), now.format("%-m"), now.format("%-d"));
    if use_docker {
        format!("/agent/task-codex-fleet/logs/{}/{}", date_part, task_id)
    } else {
        format!("~/.codex-fleet/{}/agent/task-codex-fleet/logs/{}/{}", agent_id, date_part, task_id)
    }
}

pub async fn create_task(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<Task>> {
    if req.description.trim().is_empty() {
        return Err(AppError::BadRequest("Task description cannot be empty".into()));
    }

    let (executor, agent_info) = get_executor(&state, &agent_id).await?;

    let container_name = agent_info.docker_container_name.unwrap_or_default();

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let escaped_desc = req.description.replace('\'', "'\\''");

    let task_dir = task_dir_path(agent_info.use_docker, &agent_id, &id, &now);
    let cli_cmd = format!(
        "set -o pipefail; mkdir -p '{}' && cd /workspace && codex --yolo -o '{}/result.md' '{}' 2>&1 | tee '{}/task.log'",
        task_dir, task_dir, escaped_desc, task_dir
    );

    // Execute task command via docker exec
    let exec_cmd = format!(
        "docker exec -d {} sh -lc '{}'",
        container_name, cli_cmd.replace('\'', "'\\''")
    );
    executor
        .execute(&exec_cmd)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    sqlx::query!(
        "INSERT INTO tasks (id, agent_id, description, status, task_dir, created_at, started_at) VALUES ($1, $2, $3, 'agent_in_progress', $4, $5, $5)",
        id, agent_id, req.description, task_dir, now
    )
    .execute(&state.db)
    .await?;

    Ok(Json(Task {
        id,
        agent_id,
        description: req.description,
        status: "agent_in_progress".into(),
        task_dir,
        created_at: now.to_string(),
        started_at: Some(now.to_string()),
        completed_at: None,
    }))
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Vec<Task>>> {
    let _ = sqlx::query!("SELECT id FROM agents WHERE id = $1", agent_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Agent not found".into()))?;

    let rows = sqlx::query!(
        "SELECT id, agent_id, description, status, task_dir, created_at, started_at, completed_at FROM tasks WHERE agent_id = $1 ORDER BY created_at DESC",
        agent_id
    )
    .fetch_all(&state.db)
    .await?;

    let tasks = rows
        .into_iter()
        .map(|r| Task {
            id: r.id,
            agent_id: r.agent_id,
            description: r.description,
            status: r.status,
            task_dir: r.task_dir,
            created_at: r.created_at.to_string(),
            started_at: r.started_at.map(|t| t.to_string()),
            completed_at: r.completed_at.map(|t| t.to_string()),
        })
        .collect();

    Ok(Json(tasks))
}

pub async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Task>> {
    let row = sqlx::query!(
        "SELECT id, agent_id, description, status, task_dir, created_at, started_at, completed_at FROM tasks WHERE id = $1",
        task_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Task {} not found", task_id)))?;

    Ok(Json(Task {
        id: row.id,
        agent_id: row.agent_id,
        description: row.description,
        status: row.status,
        task_dir: row.task_dir,
        created_at: row.created_at.to_string(),
        started_at: row.started_at.map(|t| t.to_string()),
        completed_at: row.completed_at.map(|t| t.to_string()),
    }))
}
