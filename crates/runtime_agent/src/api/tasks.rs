use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::agents::get_executor;
use shared_kernel::{AppContext, AppError, Result};

#[derive(Serialize)]
pub struct Task {
    pub id: String,
    pub agent_id: String,
    pub description: String,
    pub status: String,
    pub tmux_window: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub description: String,
}

pub async fn create_task(
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<Task>> {
    if req.description.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Task description cannot be empty".into(),
        ));
    }

    let (executor, agent_info) = get_executor(&state, &agent_id).await?;

    let container_name = agent_info.docker_container_name.unwrap_or_default();
    let tmux_session = agent_info.tmux_session;
    let workdir = agent_info.workdir;
    let use_docker = agent_info.use_docker;

    let id = Uuid::new_v4().to_string();
    let tmux_window = format!("task-{}", &id[..8]);

    let cli_cmd = match agent_info.cli_type.as_str() {
        "claude" | "claude_code" => format!("claude '{}'", req.description.replace('\'', "'\\''")),
        "codex" => format!("codex --yolo '{}'", req.description.replace('\'', "'\\''")),
        "gemini" | "gemini_cli" => format!("gemini '{}'", req.description.replace('\'', "'\\''")),
        "opencode" => format!("opencode '{}'", req.description.replace('\'', "'\\''")),
        _ => return Err(AppError::BadRequest("Unknown cli_type".into())),
    };
    let cli_cmd_escaped = cli_cmd.replace('\'', "'\\''");

    // Ensure session exists
    let ensure_session = if use_docker {
        format!(
            "docker exec {} tmux new-session -d -s {} -c /workspace 2>/dev/null || true",
            container_name, tmux_session
        )
    } else {
        format!(
            "tmux new-session -d -s {} -c \"{}\" 2>/dev/null || true",
            tmux_session, workdir
        )
    };
    let _ = executor.execute(&ensure_session).await;

    // Create a new window for this task
    let new_window_cmd = if use_docker {
        format!(
            "docker exec {} tmux new-window -t {} -n {} -c /workspace",
            container_name, tmux_session, tmux_window
        )
    } else {
        format!(
            "tmux new-window -t {} -n {} -c \"{}\"",
            tmux_session, tmux_window, workdir
        )
    };
    executor
        .execute(&new_window_cmd)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create tmux window: {}", e)))?;

    // Send command to that window
    let send_cmd = if use_docker {
        format!(
            "docker exec {} tmux send-keys -t '{}:{}' '{}' Enter",
            container_name, tmux_session, tmux_window, cli_cmd_escaped
        )
    } else {
        format!(
            "tmux send-keys -t '{}:{}' '{}' Enter",
            tmux_session, tmux_window, cli_cmd_escaped
        )
    };
    executor
        .execute(&send_cmd)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let now = Utc::now();

    sqlx::query!(
        "INSERT INTO tasks (id, agent_id, description, status, tmux_window, created_at, started_at) VALUES ($1, $2, $3, 'running', $4, $5, $6)",
        id, agent_id, req.description, tmux_window, now, now
    )
    .execute(&state.db)
    .await?;

    Ok(Json(Task {
        id,
        agent_id,
        description: req.description,
        status: "running".into(),
        tmux_window: Some(tmux_window),
        created_at: now.to_string(),
        started_at: Some(now.to_string()),
        completed_at: None,
    }))
}

pub async fn list_tasks(
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
) -> Result<Json<Vec<Task>>> {
    let _ = sqlx::query!("SELECT id FROM agents WHERE id = $1", agent_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Agent not found".into()))?;

    let rows = sqlx::query!(
        "SELECT id, agent_id, description, status, tmux_window, created_at, started_at, completed_at FROM tasks WHERE agent_id = $1 ORDER BY created_at DESC",
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
            tmux_window: r.tmux_window,
            created_at: r.created_at.to_string(),
            started_at: r.started_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
            completed_at: r.completed_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
        })
        .collect();

    Ok(Json(tasks))
}

pub async fn get_task(
    State(state): State<AppContext>,
    Path(task_id): Path<String>,
) -> Result<Json<Task>> {
    let row = sqlx::query!(
        "SELECT id, agent_id, description, status, tmux_window, created_at, started_at, completed_at FROM tasks WHERE id = $1",
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
        tmux_window: row.tmux_window,
        created_at: row.created_at.to_string(),
        started_at: row.started_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
        completed_at: row.completed_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
    }))
}
