use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::api::agents::{get_server_credentials, sync_agent_status};
use crate::ssh::terminal::open_exec_channel;
use shared_kernel::{AppContext, AppError, AuthContext, Result};

/// Check if an agent is currently busy (has active work items or running tasks).
pub async fn is_agent_busy(db: &sqlx::PgPool, agent_id: &str) -> Result<bool> {
    let busy = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM work_items
            WHERE assigned_agent_id = $1 AND status IN ('agent_in_progress','agent_completed')
        ) OR EXISTS(
            SELECT 1 FROM tasks WHERE agent_id = $1 AND status = 'agent_in_progress'
        ) AS "busy!""#,
        agent_id
    )
    .fetch_one(db)
    .await?;
    Ok(busy)
}

/// Core task dispatch logic shared by the HTTP handler and the scheduler.
pub async fn dispatch_task_for_agent(
    state: &AppContext,
    agent_id: &str,
    title: &str,
    description: &str,
    work_item_id: Option<String>,
    notification_ids: Vec<String>,
    user_id: Option<String>,
    username: String,
) -> Result<Task> {
    let _ = sync_agent_status(state, agent_id).await?;
    let (creds, agent_info) = get_server_credentials(state, agent_id).await?;

    if agent_info.status == "provisioning" || agent_info.status == "error" {
        return Err(AppError::Conflict(format!(
            "Agent is {} and cannot accept tasks",
            agent_info.status
        )));
    }

    if agent_info.use_docker && agent_info.status != "running" {
        return Err(AppError::Conflict(
            "Docker agent must be running before dispatching tasks".into(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    // Build CLI command and task_dir based on cli_type
    let (cli_cmd, task_dir) = match agent_info.cli_type.as_str() {
        "codex" => {
            let dir = task_dir_path(agent_info.use_docker, agent_id, &id, &now);
            let cmd = format!(
                "mkdir -p '{}' && codex exec --yolo -s danger-full-access --json -o '{}/result.md' {}",
                dir, dir, shell_quote(description)
            );
            (cmd, dir)
        }
        "claude" | "claude_code" => (format!("claude {}", shell_quote(description)), String::new()),
        "gemini" | "gemini_cli" => (format!("gemini {}", shell_quote(description)), String::new()),
        "opencode" => (format!("opencode {}", shell_quote(description)), String::new()),
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
    let notification_ids_json = serde_json::to_string(&notification_ids).unwrap_or_else(|_| "[]".into());
    sqlx::query!(
        "INSERT INTO tasks (id, agent_id, title, description, status, work_item_id, task_log, task_dir, notification_ids, user_id, username, created_at, started_at) VALUES ($1, $2, $3, $4, 'agent_in_progress', $5, '', $6, $7, $8, $9, $10, $11)",
        id, agent_id, title, description, work_item_id, task_dir, notification_ids_json, user_id, username, now, now
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
    let work_item_id_clone = work_item_id.clone();
    let task_dir_clone = task_dir.clone();
    let use_docker = agent_info.use_docker;
    let container_name = agent_info.docker_container_name.clone().unwrap_or_default();
    let notif_ids = notification_ids.clone();
    let notif_title = title.to_string();
    let notif_agent_id = agent_id.to_string();
    let notif_user_id = user_id.clone();
    let notif_username = username.clone();
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
                    "agent_completed"
                } else {
                    "agent_failed"
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
                ("agent_failed", Some(Utc::now()))
            }
        };

        // Try to read result.md from task_dir if it exists
        let result_md = if !task_dir_clone.is_empty() && status == "agent_completed" {
            let cat_cmd = if use_docker {
                format!(
                    "docker exec {} cat {}/result.md",
                    container_name, task_dir_clone
                )
            } else {
                format!("cat {}/result.md", task_dir_clone)
            };
            tracing::debug!("Reading result.md for task {}: {}", task_id, cat_cmd);
            match read_remote_file(
                &creds.ip, creds.port, &creds.username, &creds.auth_type,
                creds.password.as_deref(), creds.ssh_key_content.as_deref(),
                &cat_cmd,
            ).await {
                Ok(content) => {
                    tracing::info!("Read result.md for task {} ({} bytes)", task_id, content.len());
                    content
                }
                Err(e) => {
                    tracing::warn!("Failed to read result.md for task {}: {e}", task_id);
                    String::new()
                }
            }
        } else {
            String::new()
        };

        let _ = sqlx::query!(
            "UPDATE tasks SET status = $1, completed_at = $2, result_md = $3 WHERE id = $4",
            status,
            completed_at,
            result_md,
            task_id
        )
        .execute(&db)
        .await;

        // Sync work_item status with task status
        if let Some(ref wi_id) = work_item_id_clone {
            let wi_status = if status == "agent_completed" { "agent_completed" } else { "agent_failed" };
            let _ = sqlx::query!(
                "UPDATE work_items SET status = $1, updated_at = NOW() WHERE id = $2 AND status = 'agent_in_progress'",
                wi_status,
                wi_id
            )
            .execute(&db)
            .await;
        }

        // Send webhook notifications
        if !notif_ids.is_empty() {
            let mut payload = serde_json::json!({
                "event": status,
                "task": {
                    "id": task_id,
                    "agent_id": notif_agent_id,
                    "title": notif_title,
                    "status": status,
                    "result_md": if result_md.is_empty() { None } else { Some(&result_md) },
                    "user_id": notif_user_id,
                    "username": notif_username,
                    "created_at": now.to_string(),
                    "completed_at": completed_at.map(|t| t.to_string()),
                }
            });
            // Attach work_item info if linked
            if let Some(ref wi_id) = work_item_id_clone.clone() {
                if let Ok(Some(wi)) = sqlx::query!(
                    "SELECT id, project_id, title, status, priority, assigned_agent_id FROM work_items WHERE id = $1",
                    wi_id
                ).fetch_optional(&db).await {
                    payload["work_item"] = serde_json::json!({
                        "id": wi.id,
                        "project_id": wi.project_id,
                        "title": wi.title,
                        "status": wi.status,
                        "priority": wi.priority,
                        "assigned_agent_id": wi.assigned_agent_id,
                    });
                }
            }
            shared_kernel::send_task_notification(&db, &notif_ids, status, payload).await;
        }

        // Clean up broadcast channel
        let mut channels = task_channels.lock().await;
        channels.remove(&task_id);
    });

    Ok(Task {
        id,
        agent_id: agent_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        status: "agent_in_progress".into(),
        work_item_id,
        task_log: String::new(),
        task_dir,
        result_md: String::new(),
        thread_id: None,
        notification_ids: notification_ids_json,
        user_id,
        username,
        created_at: now.to_string(),
        started_at: Some(now.to_string()),
        completed_at: None,
    })
}

/// Lightweight task for list queries (no task_log).
#[derive(Serialize)]
pub struct TaskSummary {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub status: String,
    pub work_item_id: Option<String>,
    pub task_dir: String,
    pub thread_id: Option<String>,
    pub notification_ids: String,
    pub user_id: Option<String>,
    pub username: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Full task detail (includes task_log).
#[derive(Serialize)]
pub struct Task {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub work_item_id: Option<String>,
    pub task_log: String,
    pub task_dir: String,
    pub result_md: String,
    pub thread_id: Option<String>,
    pub notification_ids: String,
    pub user_id: Option<String>,
    pub username: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Paginated response wrapper.
#[derive(Serialize)]
pub struct PaginatedTasks {
    pub items: Vec<TaskSummary>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

/// Compute date-partitioned task directory path (codex only).
/// Docker: `/agent/task-codex-fleet/logs/YYYY/M/D/{task_id}/`
/// Non-Docker: `~/.codex-fleet/{agent_id}/agent/task-codex-fleet/logs/YYYY/M/D/{task_id}/`
fn task_dir_path(use_docker: bool, agent_id: &str, task_id: &str, now: &chrono::DateTime<Utc>) -> String {
    let date_part = format!("{}/{}/{}", now.format("%Y"), now.format("%-m"), now.format("%-d"));
    if use_docker {
        format!("/agent/task-codex-fleet/logs/{}/{}", date_part, task_id)
    } else {
        format!("~/.codex-fleet/{}/agent/task-codex-fleet/logs/{}/{}", agent_id, date_part, task_id)
    }
}

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub title: Option<String>,
    pub description: String,
    pub notification_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct ListTasksQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub async fn create_task(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Path(agent_id): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<Task>> {
    if req.description.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Task description cannot be empty".into(),
        ));
    }

    // Block dispatch if agent is busy
    if is_agent_busy(&state.db, &agent_id).await? {
        return Err(AppError::Conflict("Agent is busy".into()));
    }

    let title = req.title.unwrap_or_default();
    let notification_ids = req.notification_ids.unwrap_or_default();
    let task = dispatch_task_for_agent(
        &state, &agent_id, &title, &req.description, None,
        notification_ids, Some(auth.user_id), auth.display_name,
    ).await?;
    Ok(Json(task))
}

/// Flush accumulated db_buf to DB in a single UPDATE, then clear it.
async fn flush_log_buf(db: &sqlx::PgPool, task_id: &str, db_buf: &mut String) {
    if db_buf.is_empty() {
        return;
    }
    let _ = sqlx::query!(
        "UPDATE tasks SET task_log = task_log || $1 WHERE id = $2",
        *db_buf,
        task_id
    )
    .execute(db)
    .await;
    db_buf.clear();
}

const FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
const FLUSH_SIZE: usize = 4096;

/// Run a command via SSH and return its stdout as a string.
async fn read_remote_file(
    ip: &str, port: u16, username: &str, auth_type: &str,
    password: Option<&str>, ssh_key_content: Option<&str>,
    command: &str,
) -> anyhow::Result<String> {
    let (mut channel, _handle) =
        open_exec_channel(ip, port, username, auth_type, password, ssh_key_content, command).await?;
    let mut output = Vec::new();
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { data }) => output.extend_from_slice(&data),
            Some(russh::ChannelMsg::Eof | russh::ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    Ok(String::from_utf8_lossy(&output).to_string())
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
    let mut byte_buf = Vec::new();
    let mut db_buf = String::new();
    let mut first_line_parsed = false;
    let mut last_flush = tokio::time::Instant::now();

    loop {
        let msg = tokio::select! {
            msg = channel.wait() => msg,
            _ = tokio::time::sleep_until(last_flush + FLUSH_INTERVAL), if !db_buf.is_empty() => {
                // Timer fired — flush buffered data to DB
                flush_log_buf(db, task_id, &mut db_buf).await;
                last_flush = tokio::time::Instant::now();
                continue;
            }
        };

        match msg {
            Some(russh::ChannelMsg::Data { data }) => {
                byte_buf.extend_from_slice(&data);
                // Process complete lines
                while let Some(newline_pos) = byte_buf.iter().position(|&b| b == b'\n') {
                    let line_bytes = byte_buf.drain(..=newline_pos).collect::<Vec<_>>();
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

                    // Broadcast to live WS clients immediately
                    let _ = tx.send(line.clone());
                    // Buffer for batched DB write
                    db_buf.push_str(&line);
                }

                // Flush to DB if buffer is large enough
                if db_buf.len() >= FLUSH_SIZE {
                    flush_log_buf(db, task_id, &mut db_buf).await;
                    last_flush = tokio::time::Instant::now();
                }
            }
            Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = tx.send(text.clone());
                db_buf.push_str(&text);

                if db_buf.len() >= FLUSH_SIZE {
                    flush_log_buf(db, task_id, &mut db_buf).await;
                    last_flush = tokio::time::Instant::now();
                }
            }
            Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = Some(exit_status);
            }
            Some(russh::ChannelMsg::Eof | russh::ChannelMsg::Close) | None => break,
            _ => {}
        }
    }

    // Flush remaining byte buffer
    if !byte_buf.is_empty() {
        let remaining = String::from_utf8_lossy(&byte_buf).to_string();
        let _ = tx.send(remaining.clone());
        db_buf.push_str(&remaining);
    }

    // Final flush to DB
    flush_log_buf(db, task_id, &mut db_buf).await;

    Ok(exit_code)
}

pub async fn list_tasks(
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
    Query(params): Query<ListTasksQuery>,
) -> Result<Json<PaginatedTasks>> {
    let _ = sqlx::query!("SELECT id FROM agents WHERE id = $1", agent_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Agent not found".into()))?;

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let total = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM tasks WHERE agent_id = $1",
        agent_id
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);

    let rows = sqlx::query!(
        "SELECT id, agent_id, title, status, work_item_id, task_dir, thread_id, notification_ids, user_id, username, created_at, started_at, completed_at FROM tasks WHERE agent_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        agent_id, per_page, offset
    )
    .fetch_all(&state.db)
    .await?;

    let items = rows
        .into_iter()
        .map(|r| TaskSummary {
            id: r.id,
            agent_id: r.agent_id,
            title: r.title,
            status: r.status,
            work_item_id: r.work_item_id,
            task_dir: r.task_dir,
            thread_id: r.thread_id,
            notification_ids: r.notification_ids,
            user_id: r.user_id,
            username: r.username,
            created_at: r.created_at.to_string(),
            started_at: r.started_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
            completed_at: r.completed_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
        })
        .collect();

    Ok(Json(PaginatedTasks { items, total, page, per_page }))
}

pub async fn get_task(
    State(state): State<AppContext>,
    Path(task_id): Path<String>,
) -> Result<Json<Task>> {
    let row = sqlx::query!(
        "SELECT id, agent_id, title, description, status, work_item_id, task_log, task_dir, result_md, thread_id, notification_ids, user_id, username, created_at, started_at, completed_at FROM tasks WHERE id = $1",
        task_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Task {} not found", task_id)))?;

    Ok(Json(Task {
        id: row.id,
        agent_id: row.agent_id,
        title: row.title,
        description: row.description,
        status: row.status,
        work_item_id: row.work_item_id,
        task_log: row.task_log,
        task_dir: row.task_dir,
        result_md: row.result_md,
        thread_id: row.thread_id,
        notification_ids: row.notification_ids,
        user_id: row.user_id,
        username: row.username,
        created_at: row.created_at.to_string(),
        started_at: row.started_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
        completed_at: row.completed_at.map(|t: chrono::DateTime<chrono::Utc>| t.to_string()),
    }))
}
