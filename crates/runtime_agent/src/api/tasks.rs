use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::api::agents::get_server_credentials;
use crate::ssh::terminal::open_exec_channel;
use shared_kernel::{AppContext, AppError, Result};

#[derive(Serialize)]
pub struct Task {
    pub id: String,
    pub agent_id: String,
    pub description: String,
    pub status: String,
    pub task_log: String,
    pub thread_id: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub description: String,
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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

    let (creds, agent_info) = get_server_credentials(&state, &agent_id).await?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    // Build CLI command based on cli_type
    let cli_cmd = match agent_info.cli_type.as_str() {
        "codex" => format!(
            "codex exec --yolo -s danger-full-access --json -o result.md {}",
            shell_quote(&req.description)
        ),
        "claude" | "claude_code" => format!("claude {}", shell_quote(&req.description)),
        "gemini" | "gemini_cli" => format!("gemini {}", shell_quote(&req.description)),
        "opencode" => format!("opencode {}", shell_quote(&req.description)),
        _ => return Err(AppError::BadRequest("Unknown cli_type".into())),
    };

    // Wrap with docker exec if needed
    let full_cmd = if agent_info.use_docker {
        let container = agent_info
            .docker_container_name
            .as_deref()
            .unwrap_or("");
        format!(
            "docker exec {} sh -lc {}",
            container,
            shell_quote(&format!("cd /workspace && {}", cli_cmd))
        )
    } else {
        format!("cd {} && {}", agent_info.workdir, cli_cmd)
    };

    // Insert task record
    sqlx::query!(
        "INSERT INTO tasks (id, agent_id, description, status, task_log, created_at, started_at) VALUES ($1, $2, $3, 'running', '', $4, $5)",
        id, agent_id, req.description, now, now
    )
    .execute(&state.db)
    .await?;

    // Create broadcast channel for live streaming
    let (tx, _) = broadcast::channel::<String>(256);
    {
        let mut channels = state.task_channels.lock().await;
        channels.insert(id.clone(), tx.clone());
    }

    // Spawn background task for streaming exec
    let task_id = id.clone();
    let db = state.db.clone();
    let task_channels = state.task_channels.clone();
    tokio::spawn(async move {
        let result = run_task_exec(
            &creds.ip,
            creds.port,
            &creds.username,
            &creds.auth_type,
            creds.password.as_deref(),
            creds.ssh_key_content.as_deref(),
            &full_cmd,
            &task_id,
            &db,
            &tx,
        )
        .await;

        let (status, completed_at) = match result {
            Ok(exit_code) => {
                let s = if exit_code == Some(0) || exit_code.is_none() {
                    "completed"
                } else {
                    "failed"
                };
                (s, Some(Utc::now()))
            }
            Err(e) => {
                let err_line = format!("[error] {}\n", e);
                let _ = sqlx::query!(
                    "UPDATE tasks SET task_log = task_log || $1 WHERE id = $2",
                    err_line,
                    task_id
                )
                .execute(&db)
                .await;
                let _ = tx.send(err_line);
                ("failed", Some(Utc::now()))
            }
        };

        let _ = sqlx::query!(
            "UPDATE tasks SET status = $1, completed_at = $2 WHERE id = $3",
            status,
            completed_at,
            task_id
        )
        .execute(&db)
        .await;

        // Clean up broadcast channel
        let mut channels = task_channels.lock().await;
        channels.remove(&task_id);
    });

    Ok(Json(Task {
        id,
        agent_id,
        description: req.description,
        status: "running".into(),
        task_log: String::new(),
        thread_id: None,
        created_at: now.to_string(),
        started_at: Some(now.to_string()),
        completed_at: None,
    }))
}

async fn run_task_exec(
    ip: &str,
    port: u16,
    username: &str,
    auth_type: &str,
    password: Option<&str>,
    ssh_key_content: Option<&str>,
    command: &str,
    task_id: &str,
    db: &sqlx::PgPool,
    tx: &broadcast::Sender<String>,
) -> anyhow::Result<Option<u32>> {
    let (mut channel, _handle) =
        open_exec_channel(ip, port, username, auth_type, password, ssh_key_content, command)
            .await?;

    let mut exit_code = None;
    let mut buffer = Vec::new();
    let mut first_line_parsed = false;

    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { data }) => {
                buffer.extend_from_slice(&data);
                // Process complete lines
                while let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line_bytes = buffer.drain(..=newline_pos).collect::<Vec<_>>();
                    let line = String::from_utf8_lossy(&line_bytes).to_string();

                    // Try to parse thread_id from first JSONL line
                    if !first_line_parsed {
                        first_line_parsed = true;
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                            if json.get("type").and_then(|v| v.as_str())
                                == Some("thread.started")
                            {
                                if let Some(tid) =
                                    json.get("thread_id").and_then(|v| v.as_str())
                                {
                                    let _ = sqlx::query!(
                                        "UPDATE tasks SET thread_id = $1 WHERE id = $2",
                                        tid,
                                        task_id
                                    )
                                    .execute(db)
                                    .await;
                                }
                            }
                        }
                    }

                    // Append to DB log and broadcast
                    let _ = sqlx::query!(
                        "UPDATE tasks SET task_log = task_log || $1 WHERE id = $2",
                        line,
                        task_id
                    )
                    .execute(db)
                    .await;
                    let _ = tx.send(line);
                }
            }
            Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                // stderr — treat same as stdout for logging
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = sqlx::query!(
                    "UPDATE tasks SET task_log = task_log || $1 WHERE id = $2",
                    text,
                    task_id
                )
                .execute(db)
                .await;
                let _ = tx.send(text);
            }
            Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = Some(exit_status);
            }
            Some(russh::ChannelMsg::Eof | russh::ChannelMsg::Close) | None => break,
            _ => {}
        }
    }

    // Flush remaining buffer
    if !buffer.is_empty() {
        let remaining = String::from_utf8_lossy(&buffer).to_string();
        let _ = sqlx::query!(
            "UPDATE tasks SET task_log = task_log || $1 WHERE id = $2",
            remaining,
            task_id
        )
        .execute(db)
        .await;
        let _ = tx.send(remaining);
    }

    Ok(exit_code)
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
        "SELECT id, agent_id, description, status, task_log, thread_id, created_at, started_at, completed_at FROM tasks WHERE agent_id = $1 ORDER BY created_at DESC",
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
            task_log: r.task_log,
            thread_id: r.thread_id,
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
        "SELECT id, agent_id, description, status, task_log, thread_id, created_at, started_at, completed_at FROM tasks WHERE id = $1",
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
        task_log: row.task_log,
        thread_id: row.thread_id,
        created_at: row.created_at.to_string(),
        started_at: row.started_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
        completed_at: row.completed_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
    }))
}
