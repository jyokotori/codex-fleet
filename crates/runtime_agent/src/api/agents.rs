use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    infrastructure::crypto::Crypto,
    ssh::client::{SshClient, SshClientPool},
    ssh::terminal::{connect_russh, ClientHandler},
};
use russh::client::Handle;
use shared_kernel::{AppContext, AppError, Result};

/// Unified command executor: SSH connection only (local exec removed).
pub enum Executor {
    Ssh(SshClient),
}

impl Executor {
    pub async fn execute(&self, cmd: &str) -> anyhow::Result<String> {
        match self {
            Executor::Ssh(c) => c.execute(cmd).await,
        }
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn agent_workspace_dir(agent_id: &str) -> String {
    format!("$HOME/.codex-fleet/{}/workspace", agent_id)
}

fn agent_base_dir_from_workdir(workdir: &str) -> Option<String> {
    workdir
        .strip_suffix("/workspace")
        .filter(|base| !base.is_empty())
        .map(ToString::to_string)
}

async fn resolve_remote_home(handle: &Handle<ClientHandler>) -> anyhow::Result<String> {
    let mut ch = handle.channel_open_session().await?;
    ch.exec(true, r#"printf '%s' "$HOME""#).await?;
    let mut buf = Vec::new();
    loop {
        match ch.wait().await {
            Some(russh::ChannelMsg::Data { data }) => buf.extend_from_slice(&data),
            Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    let home = String::from_utf8_lossy(&buf).trim().to_string();
    if home.is_empty() {
        anyhow::bail!("failed to resolve remote HOME")
    }
    Ok(home)
}

/// Shell environment preamble for non-docker SSH exec (loads nvm etc.)
pub const HOST_ENV_SETUP: &str =
    r#"export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"; [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"; "#;

fn target_shell_command(use_docker: bool, container_name: &str, cmd: &str) -> String {
    if use_docker {
        format!("docker exec {} sh -lc {}", container_name, shell_quote(cmd))
    } else {
        format!("{}{}", HOST_ENV_SETUP, cmd)
    }
}

/// Return `export CODEX_HOME=<base>/agent && ` prefix for non-docker mode.
/// Docker mode uses a symlink so no prefix is needed.
/// `workdir` should be the agent's absolute workspace path (e.g. `/home/demo/.codex-fleet/{id}/workspace`).
pub fn codex_home_prefix(use_docker: bool, workdir: &str) -> String {
    if use_docker {
        String::new()
    } else {
        // workdir = /home/user/.codex-fleet/{id}/workspace → base = /home/user/.codex-fleet/{id}
        let base = workdir.trim_end_matches("/workspace");
        format!("export CODEX_HOME={}/agent && ", base)
    }
}

fn resume_terminal_input_command(use_docker: bool, workdir: &str, thread_id: &str) -> String {
    format!(
        "{}codex resume {}",
        codex_home_prefix(use_docker, workdir),
        thread_id
    )
}

#[cfg(test)]
mod tests {
    use super::{agent_base_dir_from_workdir, agent_workspace_dir, resume_terminal_input_command};

    #[test]
    fn derives_base_dir_from_host_workspace() {
        assert_eq!(
            agent_base_dir_from_workdir("/home/demo/.codex-fleet/test-agent/workspace"),
            Some("/home/demo/.codex-fleet/test-agent".to_string())
        );
    }

    #[test]
    fn rejects_invalid_workspace_shape() {
        assert_eq!(agent_base_dir_from_workdir("/workspace"), None);
        assert_eq!(
            agent_base_dir_from_workdir("/home/demo/.codex-fleet/test-agent"),
            None
        );
    }

    #[test]
    fn builds_workspace_path() {
        assert_eq!(
            agent_workspace_dir("test-agent"),
            "$HOME/.codex-fleet/test-agent/workspace"
        );
    }

    #[test]
    fn builds_resume_command_for_host_terminal() {
        assert_eq!(
            resume_terminal_input_command(
                false,
                "/home/demo/.codex-fleet/test-agent/workspace",
                "thread-123"
            ),
            "export CODEX_HOME=/home/demo/.codex-fleet/test-agent/agent && codex resume thread-123"
        );
    }

    #[test]
    fn builds_resume_command_for_docker_terminal() {
        assert_eq!(
            resume_terminal_input_command(true, "/workspace", "thread-123"),
            "codex resume thread-123"
        );
    }
}

#[derive(Serialize, Clone)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub server_id: String,
    pub user_id: Option<String>,
    pub user_display_name: Option<String>,
    pub git_repo: String,
    pub git_branch: String,
    pub git_auth_type: String,
    pub git_username: Option<String>,
    pub cli_type: String,
    pub codex_config_id: Option<String>,
    pub agents_md_id: Option<String>,
    pub docker_config_id: Option<String>,
    pub docker_image: String,
    pub docker_container_name: Option<String>,
    pub container_id: Option<String>,
    pub workdir: String,
    pub use_docker: bool,
    pub status: String,
    pub provision_log: String,
    pub provision_steps: serde_json::Value,
    pub is_busy: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub server_id: String,
    pub user_id: Option<String>,
    pub git_repo: Option<String>,
    pub git_branch: Option<String>,
    pub git_auth_type: Option<String>,
    pub git_username: Option<String>,
    pub git_password: Option<String>,
    pub cli_type: String,
    pub codex_config_id: Option<String>,
    pub agents_md_id: Option<String>,
    pub docker_config_id: Option<String>,
    pub docker_image: Option<String>,
    pub use_docker: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub user_id: Option<String>,
    pub codex_config_id: Option<String>,
    pub agents_md_id: Option<String>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn ev_step_start(step: u8, name: &str) -> Value {
    json!({"t":"step_start","step":step,"name":name,"ts":unix_now()})
}
fn ev_step_output(step: u8, text: &str) -> Value {
    json!({"t":"step_output","step":step,"text":text,"ts":unix_now()})
}
fn ev_step_done(step: u8) -> Value {
    json!({"t":"step_done","step":step,"ts":unix_now()})
}
fn ev_step_skipped(step: u8, reason: &str) -> Value {
    json!({"t":"step_skipped","step":step,"reason":reason,"ts":unix_now()})
}
fn ev_step_failed(step: u8, error: &str) -> Value {
    json!({"t":"step_failed","step":step,"error":error,"ts":unix_now()})
}
fn ev_warn(step: u8, text: &str) -> Value {
    json!({"t":"warn","step":step,"text":text,"ts":unix_now()})
}
fn ev_provision_done(status: &str) -> Value {
    json!({"t":"provision_done","status":status,"ts":unix_now()})
}
fn ev_substep(step: u8, text: &str) -> Value {
    json!({"t":"substep","step":step,"text":text,"ts":unix_now()})
}

const DOCKER_UNAVAILABLE_MARKER: &str = "__docker_unavailable__";

#[derive(Clone)]
struct DockerSyncTarget {
    agent_id: String,
    container_name: String,
}

#[derive(Clone)]
struct DockerStatusProbe {
    runtime_status: String,
    health_status: Option<String>,
}

fn agent_status_from_docker_probe(probe: &DockerStatusProbe) -> Option<&'static str> {
    if probe.health_status.as_deref() == Some("unhealthy") {
        return Some("error");
    }

    match probe.runtime_status.as_str() {
        "running" => Some("running"),
        "created" | "exited" | "paused" | "restarting" | "removing" | "dead" | "missing" => {
            Some("stopped")
        }
        _ => None,
    }
}

fn docker_status_probe_command(container_names: &[String]) -> String {
    let names = container_names
        .iter()
        .map(|name| shell_quote(name))
        .collect::<Vec<_>>()
        .join(" ");
    let format_template = shell_quote(
        "{{.State.Status}}|{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}",
    );

    format!(
        "if ! command -v docker >/dev/null 2>&1 || ! docker ps >/dev/null 2>&1; then \
            echo '{marker}'; \
         else \
            for name in {names}; do \
                if out=$(docker inspect --format {template} \"$name\" 2>/dev/null); then \
                    printf '%s|%s\\n' \"$name\" \"$out\"; \
                else \
                    printf '%s|missing|none\\n' \"$name\"; \
                fi; \
            done; \
         fi",
        marker = DOCKER_UNAVAILABLE_MARKER,
        names = names,
        template = format_template,
    )
}

fn parse_docker_status_probes(output: &str) -> Option<HashMap<String, DockerStatusProbe>> {
    let mut probes = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == DOCKER_UNAVAILABLE_MARKER {
            return None;
        }

        let mut parts = trimmed.splitn(3, '|');
        let container_name = parts
            .next()
            .unwrap_or_default()
            .trim()
            .trim_start_matches('/');
        let runtime_status = parts.next().unwrap_or("unknown").trim();
        let health_raw = parts.next().unwrap_or("none").trim();
        let health_status = match health_raw {
            "" | "none" | "null" => None,
            value => Some(value.to_string()),
        };

        probes.insert(
            container_name.to_string(),
            DockerStatusProbe {
                runtime_status: runtime_status.to_string(),
                health_status,
            },
        );
    }

    Some(probes)
}

async fn inspect_docker_statuses(
    executor: &Executor,
    container_names: &[String],
) -> anyhow::Result<Option<HashMap<String, DockerStatusProbe>>> {
    if container_names.is_empty() {
        return Ok(Some(HashMap::new()));
    }

    let output = executor
        .execute(&docker_status_probe_command(container_names))
        .await?;

    Ok(parse_docker_status_probes(&output))
}

async fn connect_executor_for_server(state: &AppContext, server_id: &str) -> Result<Executor> {
    let server = sqlx::query(
        "SELECT ip, port, username, auth_type, password_encrypted, ssh_key_content FROM servers WHERE id = $1",
    )
    .bind(server_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Server {} not found", server_id)))?;

    let crypto = Crypto::new(&state.config.master_key);
    let password_encrypted: Option<String> = server.get("password_encrypted");
    let password = password_encrypted
        .as_deref()
        .and_then(|value| crypto.decrypt(value).ok());
    let ip: String = server.get("ip");
    let port: i64 = server.get("port");
    let username: String = server.get("username");
    let auth_type: String = server.get("auth_type");
    let ssh_key_content: Option<String> = server.get("ssh_key_content");
    let client = SshClientPool::connect(
        &ip,
        port as u16,
        &username,
        &auth_type,
        password.as_deref(),
        ssh_key_content.as_deref(),
    )
    .await
    .map_err(|e| AppError::Ssh(e.to_string()))?;

    Ok(Executor::Ssh(client))
}

async fn sync_docker_agent_status_with_executor(
    state: &AppContext,
    agent_id: &str,
    executor: &Executor,
    agent_info: &AgentRow,
) -> Result<String> {
    if !agent_info.use_docker
        || agent_info.status == "provisioning"
        || agent_info.status == "provision_failed"
    {
        return Ok(agent_info.status.clone());
    }

    let Some(container_name) = agent_info
        .docker_container_name
        .as_ref()
        .filter(|name| !name.is_empty())
    else {
        return Ok(agent_info.status.clone());
    };

    let next_status = inspect_docker_statuses(executor, &[container_name.clone()])
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .and_then(|probes| {
            probes
                .get(container_name)
                .and_then(agent_status_from_docker_probe)
                .map(str::to_string)
        })
        .unwrap_or_else(|| agent_info.status.clone());

    if next_status != agent_info.status {
        sqlx::query("UPDATE agents SET status = $1 WHERE id = $2")
            .bind(&next_status)
            .bind(agent_id)
            .execute(&state.db)
            .await?;
    }

    Ok(next_status)
}

async fn sync_agent_statuses(state: &AppContext, agents: &mut [Agent]) {
    let mut docker_targets_by_server: HashMap<String, Vec<(usize, DockerSyncTarget)>> =
        HashMap::new();
    // Non-docker agents grouped by server_id for SSH check
    let mut host_agents_by_server: HashMap<String, Vec<(usize, String)>> = HashMap::new();

    for (index, agent) in agents.iter().enumerate() {
        if agent.status == "provisioning" || agent.status == "provision_failed" {
            continue;
        }

        if agent.use_docker {
            if let Some(container_name) = agent
                .docker_container_name
                .as_ref()
                .filter(|name| !name.is_empty())
            {
                docker_targets_by_server
                    .entry(agent.server_id.clone())
                    .or_default()
                    .push((
                        index,
                        DockerSyncTarget {
                            agent_id: agent.id.clone(),
                            container_name: container_name.clone(),
                        },
                    ));
            }
        } else {
            host_agents_by_server
                .entry(agent.server_id.clone())
                .or_default()
                .push((index, agent.id.clone()));
        }
    }

    let mut updates = Vec::new();

    // Sync docker agents
    for (server_id, targets) in docker_targets_by_server {
        let executor = match connect_executor_for_server(state, &server_id).await {
            Ok(executor) => executor,
            Err(err) => {
                tracing::warn!(
                    "Failed to sync docker status for server {}: {}",
                    server_id,
                    err
                );
                continue;
            }
        };

        let container_names = targets
            .iter()
            .map(|(_, target)| target.container_name.clone())
            .collect::<Vec<_>>();

        let Some(probes) = (match inspect_docker_statuses(&executor, &container_names).await {
            Ok(result) => result,
            Err(err) => {
                tracing::warn!(
                    "Failed to inspect docker status for server {}: {}",
                    server_id,
                    err
                );
                continue;
            }
        }) else {
            continue;
        };

        for (index, target) in targets {
            let Some(probe) = probes.get(&target.container_name) else {
                continue;
            };
            let Some(next_status) = agent_status_from_docker_probe(probe) else {
                continue;
            };

            if agents[index].status != next_status {
                agents[index].status = next_status.to_string();
                updates.push((target.agent_id.clone(), next_status.to_string()));
            }
        }
    }

    // Sync non-docker agents: SSH reachable = running, unreachable = stopped
    for (server_id, agent_indices) in host_agents_by_server {
        let ssh_ok = connect_executor_for_server(state, &server_id).await.is_ok();
        let next_status = if ssh_ok { "running" } else { "stopped" };

        for (index, agent_id) in agent_indices {
            if agents[index].status != next_status {
                agents[index].status = next_status.to_string();
                updates.push((agent_id, next_status.to_string()));
            }
        }
    }

    for (agent_id, status) in updates {
        if let Err(err) = sqlx::query("UPDATE agents SET status = $1 WHERE id = $2")
            .bind(&status)
            .bind(&agent_id)
            .execute(&state.db)
            .await
        {
            tracing::warn!(
                "Failed to persist synced status for agent {}: {}",
                agent_id,
                err
            );
        }
    }
}

/// Sync agent status using pre-fetched server credentials (no additional DB queries for agent/server data).
pub async fn sync_agent_status_with_creds(
    state: &AppContext,
    agent_id: &str,
    creds: &ServerCredentials,
    agent_info: &AgentRow,
) -> Result<String> {
    state.agent_status_cache.invalidate(agent_id).await;

    let status = match SshClientPool::connect(
        &creds.ip,
        creds.port,
        &creds.username,
        &creds.auth_type,
        creds.password.as_deref(),
        creds.ssh_key_content.as_deref(),
    )
    .await
    {
        Ok(client) => {
            let executor = Executor::Ssh(client);
            if agent_info.use_docker {
                sync_docker_agent_status_with_executor(state, agent_id, &executor, agent_info)
                    .await?
            } else {
                if agent_info.status != "running" {
                    let _ = sqlx::query("UPDATE agents SET status = 'running' WHERE id = $1")
                        .bind(agent_id)
                        .execute(&state.db)
                        .await;
                }
                "running".into()
            }
        }
        Err(_) => {
            let _ = sqlx::query("UPDATE agents SET status = 'stopped' WHERE id = $1")
                .bind(agent_id)
                .execute(&state.db)
                .await;
            "stopped".into()
        }
    };

    state
        .agent_status_cache
        .set(agent_id.to_string(), status.clone())
        .await;
    Ok(status)
}

#[allow(dead_code)]
pub async fn sync_agent_status(state: &AppContext, agent_id: &str) -> Result<String> {
    let (creds, agent_info) = get_server_credentials(state, agent_id).await?;
    sync_agent_status_with_creds(state, agent_id, &creds, &agent_info).await
}

fn docker_container_name(agent_info: &AgentRow) -> Result<&str> {
    agent_info
        .docker_container_name
        .as_deref()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AppError::BadRequest("Docker container not configured".into()))
}

async fn emit(db: &sqlx::PgPool, agent_id: &str, tx: &broadcast::Sender<String>, event: Value) {
    let line = serde_json::to_string(&event).unwrap_or_default() + "\n";
    let _ = sqlx::query!(
        "UPDATE agents SET provision_log = provision_log || $1 WHERE id = $2",
        line,
        agent_id
    )
    .execute(db)
    .await;

    // Persist step state transitions so provision_steps survives reconnects/refreshes
    let step_num = event.get("step").and_then(|v| v.as_u64());
    let new_status = match event.get("t").and_then(|v| v.as_str()) {
        Some("step_start") => Some("running"),
        Some("step_done") => Some("ok"),
        Some("step_failed") => Some("failed"),
        Some("step_skipped") => Some("skipped"),
        _ => None,
    };
    if let (Some(step), Some(status)) = (step_num, new_status) {
        let _ = sqlx::query!(
            "UPDATE agents SET provision_steps = provision_steps || jsonb_build_object($1::text, $2::text) WHERE id = $3",
            step.to_string(),
            status,
            agent_id
        )
        .execute(db)
        .await;
    }

    let _ = tx.send(line);
}

/// Fail provision at a given step, updating DB and broadcasting events.
async fn fail_provision(
    db: &sqlx::PgPool,
    agent_id: &str,
    tx: &broadcast::Sender<String>,
    step: u8,
    err: &str,
) {
    emit(db, agent_id, tx, ev_step_failed(step, err)).await;
    emit(db, agent_id, tx, ev_provision_done("provision_failed")).await;
    let _ = sqlx::query!(
        "UPDATE agents SET status = 'provision_failed' WHERE id = $1",
        agent_id
    )
    .execute(db)
    .await;
}

const PROVISION_FLUSH_SIZE: usize = 4096;
const PROVISION_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

async fn flush_provision_log(db: &sqlx::PgPool, agent_id: &str, buf: &mut String) {
    if buf.is_empty() {
        return;
    }
    let _ = sqlx::query!(
        "UPDATE agents SET provision_log = provision_log || $1 WHERE id = $2",
        *buf,
        agent_id
    )
    .execute(db)
    .await;
    buf.clear();
}

/// Run a single command via SSH exec channel, streaming output as provision JSONL events.
/// Returns the process exit code (0 = success).
async fn stream_cmd(
    handle: &Handle<ClientHandler>,
    cmd: &str,
    display_cmd: &str,
    db: &sqlx::PgPool,
    agent_id: &str,
    tx: &broadcast::Sender<String>,
    step: u8,
) -> anyhow::Result<u32> {
    // Echo the command being run
    emit(
        db,
        agent_id,
        tx,
        ev_step_output(step, &format!("$ {}", display_cmd)),
    )
    .await;

    let mut channel = handle.channel_open_session().await?;
    channel.exec(true, cmd).await?;

    let mut exit_code: u32 = 0;
    let mut byte_buf = Vec::new();
    let mut db_buf = String::new();
    let mut last_flush = tokio::time::Instant::now();

    loop {
        let msg = tokio::select! {
            msg = channel.wait() => msg,
            _ = tokio::time::sleep_until(last_flush + PROVISION_FLUSH_INTERVAL), if !db_buf.is_empty() => {
                flush_provision_log(db, agent_id, &mut db_buf).await;
                last_flush = tokio::time::Instant::now();
                continue;
            }
        };

        let chunk = match msg {
            Some(russh::ChannelMsg::Data { data }) => Some(data),
            Some(russh::ChannelMsg::ExtendedData { data, .. }) => Some(data),
            Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                exit_code = exit_status;
                None
            }
            // Don't break on Eof — ExitStatus may arrive after Eof but before Close.
            Some(russh::ChannelMsg::Eof) => None,
            Some(russh::ChannelMsg::Close) | None => break,
            _ => None,
        };

        if let Some(data) = chunk {
            byte_buf.extend_from_slice(&data);

            while let Some(pos) = byte_buf.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = byte_buf.drain(..=pos).collect();
                let line = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
                let event = ev_step_output(step, &line);
                let json_line = serde_json::to_string(&event).unwrap_or_default() + "\n";
                let _ = tx.send(json_line.clone());
                db_buf.push_str(&json_line);
            }

            if db_buf.len() >= PROVISION_FLUSH_SIZE {
                flush_provision_log(db, agent_id, &mut db_buf).await;
                last_flush = tokio::time::Instant::now();
            }
        }
    }

    // Flush remaining byte buffer
    if !byte_buf.is_empty() {
        let remaining = String::from_utf8_lossy(&byte_buf).trim_end().to_string();
        if !remaining.is_empty() {
            let event = ev_step_output(step, &remaining);
            let json_line = serde_json::to_string(&event).unwrap_or_default() + "\n";
            let _ = tx.send(json_line.clone());
            db_buf.push_str(&json_line);
        }
    }
    flush_provision_log(db, agent_id, &mut db_buf).await;

    Ok(exit_code)
}

/// Run a quick command and capture its output (no streaming to provision log).
async fn quick_exec(handle: &Handle<ClientHandler>, cmd: &str) -> anyhow::Result<String> {
    let mut channel = handle.channel_open_session().await?;
    channel.exec(true, cmd).await?;
    let mut output = Vec::new();
    loop {
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { data }) => output.extend_from_slice(&data),
            Some(russh::ChannelMsg::Eof) => {} // wait for Close
            Some(russh::ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    Ok(String::from_utf8_lossy(&output).trim().to_string())
}

fn require_admin(auth: &shared_kernel::AuthContext) -> Result<()> {
    if !auth.has_role("admin") {
        return Err(AppError::Forbidden("Admin role required".into()));
    }
    Ok(())
}

pub async fn list_agents(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
) -> Result<Json<Vec<Agent>>> {
    let is_admin = auth.has_role("admin");

    // Exclude heavy fields (provision_log, provision_steps) from list query
    let rows = sqlx::query(
        r#"SELECT a.id, a.name, a.server_id, a.user_id, u.display_name AS user_display_name,
           a.git_repo, a.git_branch, a.git_auth_type, a.git_username,
           a.cli_type, a.codex_config_id, a.agents_md_id, a.docker_config_id,
           a.docker_image, a.docker_container_name, a.container_id,
           a.workdir, a.use_docker, a.status, a.created_at
           FROM agents a
           LEFT JOIN users u ON a.user_id = u.id
           ORDER BY a.created_at DESC"#,
    )
    .fetch_all(&state.db)
    .await?;

    // Build a set of agent IDs that are busy (have a running task)
    let busy_rows = sqlx::query_scalar!(
        r#"SELECT DISTINCT agent_id AS "id!" FROM tasks
           WHERE status = 'agent_in_progress'"#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let busy_set: std::collections::HashSet<String> = busy_rows.into_iter().collect();

    let agents = rows
        .into_iter()
        .filter_map(|r| {
            let user_id: Option<String> = r.get("user_id");
            // Non-admin users can only see agents assigned to them
            if !is_admin {
                match &user_id {
                    Some(uid) if uid == &auth.user_id => {}
                    _ => return None,
                }
            }
            let id: String = r.get("id");
            let is_busy = busy_set.contains(&id);
            Some(Agent {
                id,
                name: r.get("name"),
                server_id: r.get("server_id"),
                user_id,
                user_display_name: r.get("user_display_name"),
                git_repo: r.get("git_repo"),
                git_branch: r.get("git_branch"),
                git_auth_type: r.get("git_auth_type"),
                git_username: r.get("git_username"),
                cli_type: r.get("cli_type"),
                codex_config_id: r.get("codex_config_id"),
                agents_md_id: r.get("agents_md_id"),
                docker_config_id: r.get("docker_config_id"),
                docker_image: r.get("docker_image"),
                docker_container_name: r.get("docker_container_name"),
                container_id: r.get("container_id"),
                workdir: r.get("workdir"),
                use_docker: r.get("use_docker"),
                status: r.get("status"),
                provision_log: String::new(),
                provision_steps: serde_json::Value::default(),
                is_busy,
                created_at: r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_string(),
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(agents))
}

#[derive(Deserialize)]
pub struct SyncStatusRequest {
    pub agent_ids: Vec<String>,
}

/// Batch sync agent statuses. Returns cached statuses for hits, probes live for misses.
pub async fn sync_status(
    State(state): State<AppContext>,
    Extension(_auth): Extension<shared_kernel::AuthContext>,
    Json(req): Json<SyncStatusRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.agent_ids.len() > 50 {
        return Err(AppError::BadRequest(
            "Maximum 50 agent IDs per request".into(),
        ));
    }
    if req.agent_ids.is_empty() {
        return Ok(Json(json!({ "statuses": {} })));
    }

    let cache = &state.agent_status_cache;
    let cached = cache.get_many(&req.agent_ids).await;

    let miss_ids: Vec<String> = req
        .agent_ids
        .iter()
        .filter(|id| !cached.contains_key(id.as_str()))
        .cloned()
        .collect();

    let mut result: HashMap<String, String> = cached;

    if !miss_ids.is_empty() {
        // Query DB for agent info needed for probing
        let placeholders: Vec<String> = miss_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("${}", i + 1))
            .collect();
        let query_str = format!(
            "SELECT id, server_id, docker_container_name, use_docker, status FROM agents WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut query = sqlx::query(&query_str);
        for id in &miss_ids {
            query = query.bind(id);
        }
        let rows = query.fetch_all(&state.db).await?;

        // Group by server_id for concurrent probing
        let mut docker_by_server: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut host_by_server: HashMap<String, Vec<String>> = HashMap::new();
        let mut agent_db_status: HashMap<String, String> = HashMap::new();

        for r in &rows {
            let id: String = r.get("id");
            let server_id: String = r.get("server_id");
            let use_docker: bool = r.get("use_docker");
            let status: String = r.get("status");
            let container_name: Option<String> = r.get("docker_container_name");

            if status == "provisioning" || status == "provision_failed" {
                result.insert(id.clone(), status.clone());
                agent_db_status.insert(id, status);
                continue;
            }

            agent_db_status.insert(id.clone(), status);

            if use_docker {
                if let Some(cn) = container_name.filter(|n| !n.is_empty()) {
                    docker_by_server
                        .entry(server_id)
                        .or_default()
                        .push((id, cn));
                }
            } else {
                host_by_server.entry(server_id).or_default().push(id);
            }
        }

        // Probe all servers concurrently
        let mut probe_futures = Vec::new();

        for (server_id, targets) in &docker_by_server {
            let state_clone = state.clone();
            let server_id = server_id.clone();
            let targets = targets.clone();
            probe_futures.push(tokio::spawn(async move {
                let mut results = Vec::new();
                match connect_executor_for_server(&state_clone, &server_id).await {
                    Ok(executor) => {
                        let container_names: Vec<String> =
                            targets.iter().map(|(_, cn)| cn.clone()).collect();
                        match inspect_docker_statuses(&executor, &container_names).await {
                            Ok(Some(probes)) => {
                                for (agent_id, cn) in &targets {
                                    if let Some(probe) = probes.get(cn) {
                                        if let Some(s) = agent_status_from_docker_probe(probe) {
                                            results.push((agent_id.clone(), s.to_string()));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(_) => {}
                }
                results
            }));
        }

        for (server_id, agent_ids) in &host_by_server {
            let state_clone = state.clone();
            let server_id = server_id.clone();
            let agent_ids = agent_ids.clone();
            probe_futures.push(tokio::spawn(async move {
                let ssh_ok = connect_executor_for_server(&state_clone, &server_id)
                    .await
                    .is_ok();
                let status = if ssh_ok { "running" } else { "stopped" };
                agent_ids
                    .into_iter()
                    .map(|id| (id, status.to_string()))
                    .collect::<Vec<_>>()
            }));
        }

        let probe_results = futures::future::join_all(probe_futures).await;
        let mut cache_entries = Vec::new();
        let mut db_updates = Vec::new();

        for join_result in probe_results {
            if let Ok(entries) = join_result {
                for (agent_id, status) in entries {
                    // Check if DB status differs
                    if let Some(db_status) = agent_db_status.get(&agent_id) {
                        if db_status != &status {
                            db_updates.push((agent_id.clone(), status.clone()));
                        }
                    }
                    cache_entries.push((agent_id.clone(), status.clone()));
                    result.insert(agent_id, status);
                }
            }
        }

        // Fill in agents that weren't probed (no container name, etc.) with DB status
        for id in &miss_ids {
            if !result.contains_key(id) {
                if let Some(db_status) = agent_db_status.get(id) {
                    result.insert(id.clone(), db_status.clone());
                    cache_entries.push((id.clone(), db_status.clone()));
                }
            }
        }

        // Update DB for changed statuses
        for (agent_id, status) in &db_updates {
            let _ = sqlx::query("UPDATE agents SET status = $1 WHERE id = $2")
                .bind(status)
                .bind(agent_id)
                .execute(&state.db)
                .await;
        }

        // Update cache
        cache.set_many(cache_entries).await;
    }

    Ok(Json(json!({ "statuses": result })))
}

/// Get a single agent by ID (full data including provision_log/provision_steps + status sync).
pub async fn get_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Path(agent_id): Path<String>,
) -> Result<Json<Agent>> {
    let is_admin = auth.has_role("admin");

    let r = sqlx::query(
        r#"SELECT a.id, a.name, a.server_id, a.user_id, u.display_name AS user_display_name,
           a.git_repo, a.git_branch, a.git_auth_type, a.git_username,
           a.cli_type, a.codex_config_id, a.agents_md_id, a.docker_config_id,
           a.docker_image, a.docker_container_name, a.container_id,
           a.workdir, a.use_docker, a.status, a.provision_log, a.provision_steps, a.created_at
           FROM agents a
           LEFT JOIN users u ON a.user_id = u.id
           WHERE a.id = $1"#,
    )
    .bind(&agent_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let user_id: Option<String> = r.get("user_id");
    if !is_admin {
        match &user_id {
            Some(uid) if uid == &auth.user_id => {}
            _ => return Err(AppError::Forbidden("Access denied".into())),
        }
    }

    let id: String = r.get("id");
    let is_busy = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(
            SELECT 1 FROM tasks
            WHERE agent_id = $1 AND status = 'agent_in_progress'
        )"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);

    let mut agent = Agent {
        id,
        name: r.get("name"),
        server_id: r.get("server_id"),
        user_id,
        user_display_name: r.get("user_display_name"),
        git_repo: r.get("git_repo"),
        git_branch: r.get("git_branch"),
        git_auth_type: r.get("git_auth_type"),
        git_username: r.get("git_username"),
        cli_type: r.get("cli_type"),
        codex_config_id: r.get("codex_config_id"),
        agents_md_id: r.get("agents_md_id"),
        docker_config_id: r.get("docker_config_id"),
        docker_image: r.get("docker_image"),
        docker_container_name: r.get("docker_container_name"),
        container_id: r.get("container_id"),
        workdir: r.get("workdir"),
        use_docker: r.get("use_docker"),
        status: r.get("status"),
        provision_log: r.get("provision_log"),
        provision_steps: r.get("provision_steps"),
        is_busy,
        created_at: r.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_string(),
    };

    // Sync status: check cache first, fallback to live probe
    if agent.status != "provisioning" && agent.status != "provision_failed" {
        if let Some(cached) = state.agent_status_cache.get(&agent.id).await {
            agent.status = cached;
        } else {
            let mut agents_arr = [agent];
            sync_agent_statuses(&state, &mut agents_arr).await;
            agent = agents_arr.into_iter().next().unwrap();
            state
                .agent_status_cache
                .set(agent.id.clone(), agent.status.clone())
                .await;
        }
    }

    Ok(Json(agent))
}

pub async fn create_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<Agent>> {
    require_admin(&auth)?;
    let git_repo = req.git_repo.unwrap_or_default();
    let git_auth_type = if git_repo.is_empty() {
        "none".to_string()
    } else {
        let gat = req.git_auth_type.unwrap_or_else(|| "passwordless".into());
        if !["passwordless", "https_password", "ssh_key", "none"].contains(&gat.as_str()) {
            return Err(AppError::BadRequest("Invalid git_auth_type".into()));
        }
        gat
    };

    let use_docker = req.use_docker.unwrap_or(true);

    if req.cli_type != "codex" {
        return Err(AppError::BadRequest(
            "Only codex is supported for now".into(),
        ));
    }

    // Check name uniqueness
    let name_exists = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM agents WHERE LOWER(name) = LOWER($1)",
        req.name
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);
    if name_exists > 0 {
        return Err(AppError::BadRequest("Agent name already exists".into()));
    }

    // Always require a real server
    if req.server_id.is_empty() {
        return Err(AppError::BadRequest("server_id is required".into()));
    }

    let server = sqlx::query!(
        "SELECT id, ip, port, username, auth_type, password_encrypted, ssh_key_content FROM servers WHERE id = $1",
        req.server_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Server not found".into()))?;

    let id = Uuid::new_v4().to_string();
    let git_branch = req
        .git_branch
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| "main".into());
    let docker_image = req.docker_image.unwrap_or_else(|| "ubuntu:24.04".into());
    let workdir = agent_workspace_dir(&id);
    let docker_container_name = if use_docker {
        Some(format!("codex-agent-{}", id))
    } else {
        None
    };
    let now = Utc::now();

    let crypto = Crypto::new(&state.config.master_key);
    let git_password_encrypted = req
        .git_password
        .as_deref()
        .map(|p| crypto.encrypt(p))
        .transpose()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let container_name_db = docker_container_name.as_deref().unwrap_or("");

    sqlx::query!(
        r#"INSERT INTO agents (id, name, server_id, user_id, git_repo, git_branch, git_auth_type, git_username,
           git_password_encrypted, cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, workdir, use_docker, status, provision_log, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, 'provisioning', '', $18)"#,
        id, req.name, req.server_id, req.user_id, git_repo, git_branch, git_auth_type, req.git_username,
        git_password_encrypted, req.cli_type, req.codex_config_id, req.agents_md_id,
        req.docker_config_id, docker_image, container_name_db, workdir,
        use_docker, now
    )
    .execute(&state.db)
    .await?;

    // Provision asynchronously
    let agent_id = id.clone();
    let git_repo_clone = git_repo.clone();
    let git_branch_clone = git_branch.clone();
    let docker_image_clone = docker_image.clone();
    let container_name_clone = container_name_db.to_string();
    let cli_type_clone = req.cli_type.clone();
    let codex_config_id_clone = req.codex_config_id.clone();
    let agents_md_id_clone = req.agents_md_id.clone();
    let docker_config_id_clone = req.docker_config_id.clone();
    let db = state.db.clone();
    let master_key = state.config.master_key.clone();

    let ssh_ip = server.ip.clone();
    let ssh_port = server.port;
    let ssh_username = server.username.clone();
    let ssh_auth_type = server.auth_type.clone();
    let ssh_password_enc = server.password_encrypted.clone();
    let ssh_key_content = server.ssh_key_content.clone();

    // Create broadcast channel and register it before spawning
    let (tx, _) = broadcast::channel::<String>(256);
    {
        let mut ch = state.provision_channels.lock().await;
        ch.insert(agent_id.clone(), tx.clone());
    }
    let provision_channels = state.provision_channels.clone();
    let status_cache = state.agent_status_cache.clone();

    tokio::spawn(async move {
        let crypto = Crypto::new(&master_key);
        let password = ssh_password_enc
            .as_deref()
            .and_then(|p| crypto.decrypt(p).ok());
        let handle = match connect_russh(
            &ssh_ip,
            ssh_port as u16,
            &ssh_username,
            &ssh_auth_type,
            password.as_deref(),
            ssh_key_content.as_deref(),
        )
        .await
        {
            Ok(h) => h,
            Err(e) => {
                emit(
                    &db,
                    &agent_id,
                    &tx,
                    ev_step_failed(0, &format!("SSH connect failed: {}", e)),
                )
                .await;
                emit(&db, &agent_id, &tx, ev_provision_done("provision_failed")).await;
                let _ = sqlx::query!(
                    "UPDATE agents SET status = 'provision_failed' WHERE id = $1",
                    agent_id
                )
                .execute(&db)
                .await;
                status_cache
                    .set(agent_id.clone(), "provision_failed".into())
                    .await;
                provision_channels.lock().await.remove(&agent_id);
                return;
            }
        };

        match provision_agent(
            &handle,
            &db,
            &agent_id,
            &tx,
            &cli_type_clone,
            codex_config_id_clone.as_deref(),
            agents_md_id_clone.as_deref(),
            docker_config_id_clone.as_deref(),
            &git_repo_clone,
            &git_branch_clone,
            &docker_image_clone,
            &container_name_clone,
            &master_key,
            use_docker,
        )
        .await
        {
            Ok(_) => {
                tracing::info!("Agent {} provisioned successfully", agent_id);
                // Update cache with final status
                let final_status = if use_docker { "stopped" } else { "running" };
                status_cache
                    .set(agent_id.clone(), final_status.into())
                    .await;
            }
            Err(e) => {
                tracing::error!("Agent {} provisioning failed: {}", agent_id, e);
                status_cache
                    .set(agent_id.clone(), "provision_failed".into())
                    .await;
            }
        }
        provision_channels.lock().await.remove(&agent_id);
    });

    // Resolve user display name for the response
    let user_display_name = if let Some(ref uid) = req.user_id {
        sqlx::query_scalar!("SELECT display_name FROM users WHERE id = $1", uid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    Ok(Json(Agent {
        id,
        name: req.name,
        server_id: req.server_id,
        user_id: req.user_id,
        user_display_name,
        git_repo,
        git_branch,
        git_auth_type,
        git_username: req.git_username,
        cli_type: req.cli_type,
        codex_config_id: req.codex_config_id,
        agents_md_id: req.agents_md_id,
        docker_config_id: req.docker_config_id,
        docker_image,
        docker_container_name,
        container_id: None,
        workdir,
        use_docker,
        status: "provisioning".into(),
        provision_log: String::new(),
        provision_steps: serde_json::json!({}),
        is_busy: false,
        created_at: now.to_string(),
    }))
}

#[allow(clippy::too_many_arguments)]
async fn provision_agent(
    handle: &Handle<ClientHandler>,
    db: &sqlx::PgPool,
    agent_id: &str,
    tx: &broadcast::Sender<String>,
    cli_type: &str,
    codex_config_id: Option<&str>,
    agents_md_id: Option<&str>,
    docker_config_id: Option<&str>,
    git_repo: &str,
    git_branch: &str,
    docker_image: &str,
    container_name: &str,
    master_key: &str,
    use_docker: bool,
) -> anyhow::Result<()> {
    let remote_home = resolve_remote_home(handle).await?;
    let base_dir = format!("{}/.codex-fleet/{}", remote_home, agent_id);
    let workspace_dir = format!("{}/workspace", base_dir);

    // Persist the host-side workspace path for both docker and non-docker agents.
    let _ = sqlx::query("UPDATE agents SET workdir = $1 WHERE id = $2")
        .bind(&workspace_dir)
        .bind(agent_id)
        .execute(db)
        .await;

    // Step 1: Create dirs + write config files
    emit(
        db,
        agent_id,
        tx,
        ev_step_start(1, "Create dirs & write configs"),
    )
    .await;
    emit(db, agent_id, tx, ev_substep(1, "Creating directories")).await;
    let dir_cmd = format!("mkdir -p {base}/agent {base}/workspace", base = base_dir);
    match stream_cmd(handle, &dir_cmd, &dir_cmd, db, agent_id, tx, 1).await {
        Ok(0) => {}
        Ok(code) => {
            let err = format!("Step 1 failed (exit code {})", code);
            fail_provision(db, agent_id, tx, 1, &err).await;
            return Err(anyhow::anyhow!(err));
        }
        Err(e) => {
            let err = format!("Step 1 failed: {}", e);
            fail_provision(db, agent_id, tx, 1, &err).await;
            return Err(anyhow::anyhow!(err));
        }
    }

    // Write config files (codex config, auth.json, AGENTS.md)
    if let Some(config_id) = codex_config_id {
        if let Ok(row) = sqlx::query!(
            "SELECT config_toml, auth_json FROM codex_configs WHERE id = $1",
            config_id
        )
        .fetch_one(db)
        .await
        {
            let crypto = Crypto::new(master_key);
            let auth_json_content = if row.auth_json.starts_with("enc:") {
                crypto
                    .decrypt(row.auth_json.trim_start_matches("enc:"))
                    .unwrap_or(row.auth_json.clone())
            } else {
                row.auth_json.clone()
            };

            if !row.config_toml.is_empty() {
                emit(db, agent_id, tx, ev_substep(1, "Writing config.toml")).await;
                let b64 = BASE64.encode(row.config_toml.as_bytes());
                let cmd = format!(
                    "echo '{}' | base64 -d > {}/agent/config.toml",
                    b64, base_dir
                );
                let display = format!("Writing ~/.codex-fleet/{}/agent/config.toml", agent_id);
                match stream_cmd(handle, &cmd, &display, db, agent_id, tx, 1).await {
                    Ok(0) => {}
                    Ok(code) => {
                        emit(
                            db,
                            agent_id,
                            tx,
                            ev_warn(1, &format!("config.toml write failed (exit {})", code)),
                        )
                        .await;
                    }
                    Err(e) => {
                        emit(
                            db,
                            agent_id,
                            tx,
                            ev_warn(1, &format!("config.toml write failed: {}", e)),
                        )
                        .await;
                    }
                }
            }

            if !auth_json_content.is_empty() {
                emit(db, agent_id, tx, ev_substep(1, "Writing auth.json")).await;
                let b64 = BASE64.encode(auth_json_content.as_bytes());
                let cmd = format!("echo '{}' | base64 -d > {}/agent/auth.json", b64, base_dir);
                let display = format!("Writing ~/.codex-fleet/{}/agent/auth.json", agent_id);
                match stream_cmd(handle, &cmd, &display, db, agent_id, tx, 1).await {
                    Ok(0) => {}
                    Ok(code) => {
                        emit(
                            db,
                            agent_id,
                            tx,
                            ev_warn(1, &format!("auth.json write failed (exit {})", code)),
                        )
                        .await;
                    }
                    Err(e) => {
                        emit(
                            db,
                            agent_id,
                            tx,
                            ev_warn(1, &format!("auth.json write failed: {}", e)),
                        )
                        .await;
                    }
                }
            }
        }
    }

    if let Some(md_id) = agents_md_id {
        if let Ok(row) = sqlx::query!("SELECT content FROM company_configs WHERE id = $1", md_id)
            .fetch_one(db)
            .await
        {
            if !row.content.is_empty() {
                emit(db, agent_id, tx, ev_substep(1, "Writing AGENTS.md")).await;
                let b64 = BASE64.encode(row.content.as_bytes());
                let cmd = format!("echo '{}' | base64 -d > {}/agent/AGENTS.md", b64, base_dir);
                let display = format!("Writing ~/.codex-fleet/{}/agent/AGENTS.md", agent_id);
                match stream_cmd(handle, &cmd, &display, db, agent_id, tx, 1).await {
                    Ok(0) => {}
                    Ok(code) => {
                        emit(
                            db,
                            agent_id,
                            tx,
                            ev_warn(1, &format!("AGENTS.md write failed (exit {})", code)),
                        )
                        .await;
                    }
                    Err(e) => {
                        emit(
                            db,
                            agent_id,
                            tx,
                            ev_warn(1, &format!("AGENTS.md write failed: {}", e)),
                        )
                        .await;
                    }
                }
            }
        }
    }
    emit(db, agent_id, tx, ev_step_done(1)).await;

    // Step 2: Docker start + run init_script (skip if !use_docker)
    if use_docker {
        emit(db, agent_id, tx, ev_step_start(2, "Docker setup")).await;

        emit(db, agent_id, tx, ev_substep(2, "Removing old container")).await;
        let rm_cmd = format!("docker rm -f {} 2>/dev/null || true", container_name);
        let _ = stream_cmd(handle, &rm_cmd, &rm_cmd, db, agent_id, tx, 2).await;

        emit(
            db,
            agent_id,
            tx,
            ev_substep(2, "Starting container (docker run)"),
        )
        .await;
        let mut docker_run = format!(
            "docker run -d --name {container} --workdir /workspace \
             -e LANG=C.UTF-8 -e LC_ALL=C.UTF-8 -e TERM=xterm-256color \
             -v {base}/agent:/agent \
             -v {base}/workspace:/workspace",
            container = container_name,
            base = base_dir,
        );

        if let Some(dc_id) = docker_config_id {
            if let Ok(dc) = sqlx::query!(
                "SELECT port_mappings, env_vars, volume_mappings FROM docker_configs WHERE id = $1",
                dc_id
            )
            .fetch_one(db)
            .await
            {
                if let Ok(ports) = serde_json::from_str::<serde_json::Value>(&dc.port_mappings) {
                    if let Some(arr) = ports.as_array() {
                        for p in arr {
                            let host = p.get("host_port").and_then(|v| v.as_str()).unwrap_or("");
                            let cont = p
                                .get("container_port")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let proto = p.get("protocol").and_then(|v| v.as_str()).unwrap_or("tcp");
                            if !host.is_empty() && !cont.is_empty() {
                                docker_run.push_str(&format!(" -p {}:{}/{}", host, cont, proto));
                            }
                        }
                    }
                }
                if let Ok(envs) = serde_json::from_str::<serde_json::Value>(&dc.env_vars) {
                    if let Some(arr) = envs.as_array() {
                        let mut env_count = 0usize;
                        for e in arr {
                            let key = e.get("key").and_then(|v| v.as_str()).unwrap_or("");
                            let val = e.get("value").and_then(|v| v.as_str()).unwrap_or("");
                            if !key.is_empty() {
                                docker_run.push_str(&format!(
                                    " -e {}",
                                    shell_quote(&format!("{}={}", key, val))
                                ));
                                env_count += 1;
                            }
                        }
                        if env_count > 0 {
                            emit(
                                db,
                                agent_id,
                                tx,
                                ev_step_output(2, &format!("Injecting {} env var(s)", env_count)),
                            )
                            .await;
                        }
                    }
                }
                if let Ok(vols) = serde_json::from_str::<serde_json::Value>(&dc.volume_mappings) {
                    if let Some(arr) = vols.as_array() {
                        for v in arr {
                            let host = v.get("host_path").and_then(|v| v.as_str()).unwrap_or("");
                            let cont = v
                                .get("container_path")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let mode = v.get("mode").and_then(|v| v.as_str()).unwrap_or("rw");
                            if !host.is_empty() && !cont.is_empty() {
                                docker_run.push_str(&format!(
                                    " -v {}",
                                    shell_quote(&format!("{}:{}:{}", host, cont, mode))
                                ));
                            }
                        }
                    }
                }
            }
        }

        docker_run.push_str(&format!(" {} tail -f /dev/null", shell_quote(docker_image)));

        match stream_cmd(handle, &docker_run, &docker_run, db, agent_id, tx, 2).await {
            Ok(0) => {}
            Ok(code) => {
                let err = format!("Step 2 failed (docker run, exit code {})", code);
                fail_provision(db, agent_id, tx, 2, &err).await;
                return Err(anyhow::anyhow!(err));
            }
            Err(e) => {
                let err = format!("Step 2 failed (docker run): {}", e);
                fail_provision(db, agent_id, tx, 2, &err).await;
                return Err(anyhow::anyhow!(err));
            }
        }

        // Verify container is actually running (docker run -d can exit 0 but container may die immediately)
        let container_state = quick_exec(handle, &format!(
            "docker inspect --format '{{{{.State.Status}}}}|{{{{.Id}}}}' {} 2>/dev/null || echo 'missing|'",
            container_name
        )).await.unwrap_or_default();
        let mut parts = container_state.splitn(2, '|');
        let state = parts.next().unwrap_or("missing");
        let cid = parts.next().unwrap_or("").trim();
        if state != "running" {
            // Try to get error logs from the container
            let logs = quick_exec(
                handle,
                &format!("docker logs --tail 10 {} 2>&1", container_name),
            )
            .await
            .unwrap_or_default();
            let err = if logs.is_empty() {
                format!("Container is not running (state: {})", state)
            } else {
                format!("Container is not running (state: {})\n{}", state, logs)
            };
            fail_provision(db, agent_id, tx, 2, &err).await;
            return Err(anyhow::anyhow!(err));
        }
        if !cid.is_empty() {
            emit(
                db,
                agent_id,
                tx,
                ev_step_output(2, &format!("Container ID: {}", &cid[..cid.len().min(12)])),
            )
            .await;
            let _ = sqlx::query!(
                "UPDATE agents SET container_id = $1 WHERE id = $2",
                cid,
                agent_id
            )
            .execute(db)
            .await;
        }

        // Run init_script if configured
        if let Some(dc_id) = docker_config_id {
            if let Ok(dc) = sqlx::query!(
                "SELECT init_script FROM docker_configs WHERE id = $1",
                dc_id
            )
            .fetch_one(db)
            .await
            {
                if !dc.init_script.is_empty() {
                    emit(db, agent_id, tx, ev_substep(2, "Running init script")).await;
                    let cmd = target_shell_command(true, container_name, &dc.init_script);
                    match stream_cmd(handle, &cmd, "Running init script", db, agent_id, tx, 2).await
                    {
                        Ok(code) if code != 0 => {
                            emit(
                                db,
                                agent_id,
                                tx,
                                ev_warn(2, &format!("init_script failed (exit {})", code)),
                            )
                            .await;
                        }
                        Err(e) => {
                            emit(
                                db,
                                agent_id,
                                tx,
                                ev_warn(2, &format!("init_script failed: {}", e)),
                            )
                            .await;
                        }
                        _ => {}
                    }
                }
            }
        }
        emit(db, agent_id, tx, ev_step_done(2)).await;
    } else {
        emit(db, agent_id, tx, ev_step_skipped(2, "no-docker mode")).await;
    }

    // Step 3: Install CLI + git (inside docker if use_docker)
    emit(
        db,
        agent_id,
        tx,
        ev_step_start(3, "Install CLI & environment"),
    )
    .await;

    if cli_type == "codex" {
        let mut codex_found = false;
        let check_substep = if use_docker {
            "Checking codex in container"
        } else {
            "Checking codex on host"
        };
        emit(db, agent_id, tx, ev_substep(3, check_substep)).await;
        let check_script = "command -v codex >/dev/null 2>&1 && codex --version 2>/dev/null";
        let check_cmd = if use_docker {
            target_shell_command(true, container_name, check_script)
        } else {
            format!("{}{}", HOST_ENV_SETUP, check_script)
        };
        match stream_cmd(
            handle,
            &check_cmd,
            "command -v codex && codex --version",
            db,
            agent_id,
            tx,
            3,
        )
        .await
        {
            Ok(0) => {
                emit(
                    db,
                    agent_id,
                    tx,
                    ev_substep(3, "codex already installed, skipping install"),
                )
                .await;
                codex_found = true;
            }
            _ => {}
        }

        if !codex_found {
            emit(db, agent_id, tx, ev_substep(3, "Checking npm")).await;
            let ensure_npm_script = r#"if command -v npm >/dev/null 2>&1; then
  echo "npm already installed"
  exit 0
fi
run_pm() {
  if [ "$(id -u)" -eq 0 ]; then "$@";
  elif command -v sudo >/dev/null 2>&1; then sudo "$@";
  else "$@";
  fi
}
if command -v apt-get >/dev/null 2>&1; then
  run_pm apt-get update -qq && DEBIAN_FRONTEND=noninteractive run_pm apt-get install -y nodejs npm -qq
elif command -v dnf >/dev/null 2>&1; then
  run_pm dnf install -y nodejs npm
elif command -v yum >/dev/null 2>&1; then
  run_pm yum install -y nodejs npm
elif command -v apk >/dev/null 2>&1; then
  run_pm apk add --no-cache nodejs npm
else
  echo "npm not found and no supported package manager"
  exit 1
fi"#;
            let ensure_npm_cmd =
                target_shell_command(use_docker, container_name, ensure_npm_script);
            match stream_cmd(
                handle,
                &ensure_npm_cmd,
                "Checking/installing npm",
                db,
                agent_id,
                tx,
                3,
            )
            .await
            {
                Ok(0) => {}
                Ok(code) => {
                    let err = format!("Step 3 failed: npm check/install (exit code {})", code);
                    fail_provision(db, agent_id, tx, 3, &err).await;
                    return Err(anyhow::anyhow!(err));
                }
                Err(e) => {
                    let err = format!("Step 3 failed: npm check/install: {}", e);
                    fail_provision(db, agent_id, tx, 3, &err).await;
                    return Err(anyhow::anyhow!(err));
                }
            }

            emit(
                db,
                agent_id,
                tx,
                ev_substep(3, "Installing @openai/codex (may take a few minutes)"),
            )
            .await;
            let install_codex_script = r#"if npm i -g @openai/codex@latest; then
  exit 0
fi
if command -v sudo >/dev/null 2>&1; then
  sudo npm i -g @openai/codex@latest
else
  exit 1
fi"#;
            let install_codex_cmd =
                target_shell_command(use_docker, container_name, install_codex_script);
            match stream_cmd(
                handle,
                &install_codex_cmd,
                "npm i -g @openai/codex@latest",
                db,
                agent_id,
                tx,
                3,
            )
            .await
            {
                Ok(0) => {}
                Ok(code) => {
                    let err = format!("Step 3 failed: codex install (exit code {})", code);
                    fail_provision(db, agent_id, tx, 3, &err).await;
                    return Err(anyhow::anyhow!(err));
                }
                Err(e) => {
                    let err = format!("Step 3 failed: codex install: {}", e);
                    fail_provision(db, agent_id, tx, 3, &err).await;
                    return Err(anyhow::anyhow!(err));
                }
            }
        }

        if use_docker {
            emit(db, agent_id, tx, ev_substep(3, "Linking config directory")).await;
            let link_cmd = target_shell_command(
                true,
                container_name,
                "mkdir -p /root && if [ -e /root/.codex ] && [ ! -L /root/.codex ]; then mv /root/.codex /root/.codex_backup_$(date +%Y%m%d%H%M%S); fi && ln -sfn /agent /root/.codex",
            );
            match stream_cmd(
                handle,
                &link_cmd,
                "ln -sfn /agent /root/.codex",
                db,
                agent_id,
                tx,
                3,
            )
            .await
            {
                Ok(code) if code != 0 => {
                    emit(
                        db,
                        agent_id,
                        tx,
                        ev_warn(
                            3,
                            &format!("link /agent -> /root/.codex failed (exit {})", code),
                        ),
                    )
                    .await;
                }
                Err(e) => {
                    emit(
                        db,
                        agent_id,
                        tx,
                        ev_warn(3, &format!("link /agent -> /root/.codex failed: {}", e)),
                    )
                    .await;
                }
                _ => {}
            }
        }
    } else {
        emit(
            db,
            agent_id,
            tx,
            ev_step_output(
                3,
                &format!("(cli_type={}, skipping codex install)", cli_type),
            ),
        )
        .await;
    }

    // Install git (check first)
    emit(db, agent_id, tx, ev_substep(3, "Checking git")).await;
    let ensure_git_script = r#"if command -v git >/dev/null 2>&1; then
  echo "git already installed"
  exit 0
fi
run_pm() {
  if [ "$(id -u)" -eq 0 ]; then "$@";
  elif command -v sudo >/dev/null 2>&1; then sudo "$@";
  else "$@";
  fi
}
if command -v apt-get >/dev/null 2>&1; then
  run_pm apt-get update -qq && DEBIAN_FRONTEND=noninteractive run_pm apt-get install -y git -qq
elif command -v dnf >/dev/null 2>&1; then
  run_pm dnf install -y git
elif command -v yum >/dev/null 2>&1; then
  run_pm yum install -y git
elif command -v apk >/dev/null 2>&1; then
  run_pm apk add --no-cache git
else
  echo "git not found and no supported package manager"
  exit 1
fi"#;
    let ensure_git_cmd = target_shell_command(use_docker, container_name, ensure_git_script);
    match stream_cmd(
        handle,
        &ensure_git_cmd,
        "Checking/installing git",
        db,
        agent_id,
        tx,
        3,
    )
    .await
    {
        Ok(0) => {
            emit(db, agent_id, tx, ev_step_done(3)).await;
        }
        Ok(code) => {
            if git_repo.is_empty() {
                emit(
                    db,
                    agent_id,
                    tx,
                    ev_warn(3, &format!("git install failed (exit {})", code)),
                )
                .await;
                emit(db, agent_id, tx, ev_step_done(3)).await;
            } else {
                let err = format!("Step 3 failed: git install (exit code {})", code);
                fail_provision(db, agent_id, tx, 3, &err).await;
                return Err(anyhow::anyhow!(err));
            }
        }
        Err(e) => {
            if git_repo.is_empty() {
                emit(
                    db,
                    agent_id,
                    tx,
                    ev_warn(3, &format!("git install failed: {}", e)),
                )
                .await;
                emit(db, agent_id, tx, ev_step_done(3)).await;
            } else {
                let err = format!("Step 3 failed: git install: {}", e);
                fail_provision(db, agent_id, tx, 3, &err).await;
                return Err(anyhow::anyhow!(err));
            }
        }
    }

    // Step 4: Git clone / sync (skip if no git_repo)
    if !git_repo.is_empty() {
        emit(db, agent_id, tx, ev_step_start(4, "Git clone / sync")).await;
        emit(
            db,
            agent_id,
            tx,
            ev_substep(4, "Cloning / syncing repository"),
        )
        .await;
        let branch = if git_branch.trim().is_empty() {
            "main"
        } else {
            git_branch
        };
        let git_sync_script = format!(
            r#"workspace="{workspace}"
if [ -d "$workspace/.git" ]; then
  cd "$workspace" && git fetch --all && git checkout {branch} && (git pull --ff-only origin {branch} || git pull --ff-only || true)
else
  if [ -n "$(ls -A "$workspace" 2>/dev/null)" ]; then
    rm -rf "$workspace"/* "$workspace"/.[!.]* "$workspace"/..?* 2>/dev/null || true
  fi
  git clone {repo} "$workspace" && cd "$workspace" && git checkout {branch}
fi"#,
            workspace = workspace_dir,
            repo = shell_quote(git_repo),
            branch = shell_quote(branch),
        );
        let git_sync_cmd = target_shell_command(use_docker, container_name, &git_sync_script);
        let git_display = format!("git clone/sync {}", git_repo);
        match stream_cmd(handle, &git_sync_cmd, &git_display, db, agent_id, tx, 4).await {
            Ok(0) => {
                emit(db, agent_id, tx, ev_step_done(4)).await;
            }
            Ok(code) => {
                let err = format!("Step 4 failed (exit code {})", code);
                fail_provision(db, agent_id, tx, 4, &err).await;
                return Err(anyhow::anyhow!(err));
            }
            Err(e) => {
                let err = format!("Step 4 failed: {}", e);
                fail_provision(db, agent_id, tx, 4, &err).await;
                return Err(anyhow::anyhow!(err));
            }
        }
    } else {
        emit(
            db,
            agent_id,
            tx,
            ev_step_skipped(4, "no git repo configured"),
        )
        .await;
    }

    // Done — non-docker agents are "running" once SSH was verified during provisioning
    let final_status = if use_docker { "stopped" } else { "running" };
    emit(db, agent_id, tx, ev_provision_done(final_status)).await;
    let _ = sqlx::query("UPDATE agents SET status = $1 WHERE id = $2")
        .bind(final_status)
        .bind(agent_id)
        .execute(db)
        .await;

    Ok(())
}

pub async fn update_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> std::result::Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_admin(&auth)?;
    let existing = sqlx::query!(
        r#"SELECT id, name, server_id, user_id,
           cli_type, codex_config_id, agents_md_id,
           docker_container_name, workdir, use_docker, created_at
           FROM agents WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let name = req.name.unwrap_or(existing.name.clone());

    // Check name uniqueness (exclude self)
    if name != existing.name {
        let name_exists = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM agents WHERE LOWER(name) = LOWER($1) AND id != $2",
            name,
            id
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);
        if name_exists > 0 {
            return Err(AppError::BadRequest("Agent name already exists".into()));
        }
    }

    // Detect which configs actually changed (Some means the field was sent)
    let codex_config_changed = req.codex_config_id.is_some()
        && req.codex_config_id != existing.codex_config_id;
    let agents_md_changed = req.agents_md_id.is_some()
        && req.agents_md_id != existing.agents_md_id;

    // New values (empty string → None = "no config")
    let codex_config_id = if req.codex_config_id.is_some() {
        req.codex_config_id.clone().filter(|s| !s.is_empty())
    } else {
        existing.codex_config_id.clone()
    };
    let agents_md_id = if req.agents_md_id.is_some() {
        req.agents_md_id.clone().filter(|s| !s.is_empty())
    } else {
        existing.agents_md_id.clone()
    };

    // user_id: if explicitly provided (even as null/empty) update it; otherwise keep existing
    let user_id = if req.user_id.is_some() {
        req.user_id.clone().filter(|s| !s.is_empty())
    } else {
        existing.user_id.clone()
    };

    sqlx::query!(
        "UPDATE agents SET name=$1, codex_config_id=$2, agents_md_id=$3, user_id=$4 WHERE id=$5",
        name, codex_config_id, agents_md_id, user_id, id
    )
    .execute(&state.db)
    .await?;

    // Previous config IDs (for knowing whether there was an old file to backup)
    let old_codex_config_id = existing.codex_config_id.clone();
    let old_agents_md_id = existing.agents_md_id.clone();

    if codex_config_changed || agents_md_changed {
        let db = state.db.clone();
        let master_key = state.config.master_key.clone();
        let server_id = existing.server_id.clone();
        let agent_workdir = existing.workdir.clone();
        let new_codex_config_id = codex_config_id.clone();
        let new_agents_md_id = agents_md_id.clone();

        let server = sqlx::query!(
            "SELECT ip, port, username, auth_type, password_encrypted, ssh_key_content FROM servers WHERE id = $1",
            server_id
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", server_id)))?;

        tokio::spawn(async move {
            let crypto = Crypto::new(&master_key);
            let password = server.password_encrypted
                .as_deref()
                .and_then(|p| crypto.decrypt(p).ok());
            let executor = match SshClientPool::connect(
                &server.ip,
                server.port as u16,
                &server.username,
                &server.auth_type,
                password.as_deref(),
                server.ssh_key_content.as_deref(),
            )
            .await
            {
                Ok(c) => Executor::Ssh(c),
                Err(e) => {
                    tracing::error!("update_agent async connect failed: {}", e);
                    return;
                }
            };
            let Some(base_dir) = agent_base_dir_from_workdir(&agent_workdir) else {
                tracing::error!("update_agent async invalid workdir: {}", agent_workdir);
                return;
            };
            let agent_dir = format!("{}/agent", base_dir);
            let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");

            // --- Codex config (config.toml + auth.json) ---
            if codex_config_changed {
                // Backup old files if a previous config existed
                if old_codex_config_id.is_some() {
                    let _ = executor.execute(&format!(
                        "[ -f {dir}/config.toml ] && mv {dir}/config.toml {dir}/config.toml_backup_{ts}; \
                         [ -f {dir}/auth.json ] && mv {dir}/auth.json {dir}/auth.json_backup_{ts}",
                        dir = agent_dir, ts = ts
                    )).await;
                }

                // Write new files if a new config is selected
                if let Some(cid) = new_codex_config_id {
                    if let Ok(row) = sqlx::query!(
                        "SELECT config_toml, auth_json FROM codex_configs WHERE id = $1",
                        cid
                    )
                    .fetch_one(&db)
                    .await
                    {
                        let crypto = Crypto::new(&master_key);
                        let auth_json_content = if row.auth_json.starts_with("enc:") {
                            crypto
                                .decrypt(row.auth_json.trim_start_matches("enc:"))
                                .unwrap_or(row.auth_json.clone())
                        } else {
                            row.auth_json.clone()
                        };

                        if !row.config_toml.is_empty() {
                            let b64 = BASE64.encode(row.config_toml.as_bytes());
                            let cmd = format!(
                                "echo '{}' | base64 -d > {}/config.toml",
                                b64, agent_dir
                            );
                            let _ = executor.execute(&cmd).await;
                        }
                        if !auth_json_content.is_empty() {
                            let b64 = BASE64.encode(auth_json_content.as_bytes());
                            let cmd = format!(
                                "echo '{}' | base64 -d > {}/auth.json",
                                b64, agent_dir
                            );
                            let _ = executor.execute(&cmd).await;
                        }
                    }
                }
                // If new_codex_config_id is None, we only backed up — no new file written
            }

            // --- AGENTS.md ---
            if agents_md_changed {
                // Backup old file if a previous config existed
                if old_agents_md_id.is_some() {
                    let _ = executor.execute(&format!(
                        "[ -f {dir}/AGENTS.md ] && mv {dir}/AGENTS.md {dir}/AGENTS.md_backup_{ts}",
                        dir = agent_dir, ts = ts
                    )).await;
                }

                // Write new file if a new config is selected
                if let Some(md_id) = new_agents_md_id {
                    if let Ok(row) =
                        sqlx::query!("SELECT content FROM company_configs WHERE id = $1", md_id)
                            .fetch_one(&db)
                            .await
                    {
                        if !row.content.is_empty() {
                            let b64 = BASE64.encode(row.content.as_bytes());
                            let cmd = format!(
                                "echo '{}' | base64 -d > {}/AGENTS.md",
                                b64, agent_dir
                            );
                            let _ = executor.execute(&cmd).await;
                        }
                    }
                }
                // If new_agents_md_id is None, we only backed up — no new file written
            }
        });
    }

    let updated = sqlx::query(
        r#"SELECT a.id, a.name, a.server_id, a.user_id, u.display_name AS user_display_name,
           a.git_repo, a.git_branch, a.git_auth_type, a.git_username,
           a.cli_type, a.codex_config_id, a.agents_md_id, a.docker_config_id,
           a.docker_image, a.docker_container_name, a.container_id,
           a.workdir, a.use_docker, a.status, a.provision_log, a.provision_steps, a.created_at
           FROM agents a LEFT JOIN users u ON a.user_id = u.id
           WHERE a.id = $1"#,
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await?;

    let agent = Agent {
        id: updated.get("id"),
        name: updated.get("name"),
        server_id: updated.get("server_id"),
        user_id: updated.get("user_id"),
        user_display_name: updated.get("user_display_name"),
        git_repo: updated.get("git_repo"),
        git_branch: updated.get("git_branch"),
        git_auth_type: updated.get("git_auth_type"),
        git_username: updated.get("git_username"),
        cli_type: updated.get("cli_type"),
        codex_config_id: updated.get("codex_config_id"),
        agents_md_id: updated.get("agents_md_id"),
        docker_config_id: updated.get("docker_config_id"),
        docker_image: updated.get("docker_image"),
        docker_container_name: updated.get("docker_container_name"),
        container_id: updated.get("container_id"),
        workdir: updated.get("workdir"),
        use_docker: updated.get("use_docker"),
        status: updated.get("status"),
        provision_log: updated.get("provision_log"),
        provision_steps: updated.get("provision_steps"),
        is_busy: false,
        created_at: updated.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_string(),
    };

    Ok((StatusCode::OK, Json(serde_json::to_value(agent).unwrap())))
}

#[derive(Deserialize)]
pub struct DeleteAgentQuery {
    #[serde(default)]
    pub cleanup: Option<bool>,
}

pub async fn delete_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Path(id): Path<String>,
    Query(params): Query<DeleteAgentQuery>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&auth)?;
    let cleanup = params.cleanup.unwrap_or(false);
    tracing::info!(agent_id = %id, cleanup = cleanup, "Deleting agent");

    if cleanup {
        match get_executor(&state, &id).await {
            Ok((executor, agent_info)) => {
                if agent_info.use_docker {
                    let container = agent_info.docker_container_name.unwrap_or_default();
                    if !container.is_empty() {
                        tracing::info!(agent_id = %id, container = %container, "Stopping and removing docker container");
                        let _ = executor
                            .execute(&format!(
                                "docker stop {} 2>/dev/null; docker rm {} 2>/dev/null",
                                container, container
                            ))
                            .await;
                    }
                }

                // For Docker agents, workdir is the container-internal path (/workspace),
                // so derive the host-side base dir from the agent id instead.
                let base = if agent_info.use_docker {
                    format!("$HOME/.codex-fleet/{}", id)
                } else {
                    agent_base_dir_from_workdir(&agent_info.workdir).unwrap_or_else(|| {
                        format!("$HOME/.codex-fleet/{}", id)
                    })
                };
                tracing::info!(agent_id = %id, base_dir = %base, "Removing agent files");
                let _ = executor.execute(&format!("rm -rf {}/", base)).await;
            }
            Err(e) => {
                tracing::error!(agent_id = %id, error = %e, "Server unreachable during agent cleanup");
                return Err(AppError::Conflict(
                    "Server unreachable, cannot clean up remote data. You can delete the database record only by unchecking the cleanup option.".into(),
                ));
            }
        }
    }

    // Delete related tasks first (FK has no ON DELETE CASCADE)
    sqlx::query!("DELETE FROM tasks WHERE agent_id = $1", id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(agent_id = %id, error = %e, "Failed to delete agent tasks from database");
            AppError::Database(e)
        })?;

    sqlx::query!("DELETE FROM agents WHERE id = $1", id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(agent_id = %id, error = %e, "Failed to delete agent from database");
            AppError::Database(e)
        })?;

    tracing::info!(agent_id = %id, "Agent deleted successfully");
    Ok(Json(serde_json::json!({"message": "Agent deleted"})))
}

pub async fn start_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&auth)?;
    let (executor, agent_info) = get_executor(&state, &id).await?;

    if agent_info.use_docker {
        let container_name = docker_container_name(&agent_info)?;
        let _ = executor
            .execute(&format!("docker start {}", container_name))
            .await;

        let synced_status =
            sync_docker_agent_status_with_executor(&state, &id, &executor, &agent_info).await?;
        if synced_status != "running" {
            return Err(AppError::Conflict(format!(
                "Docker agent did not start successfully (current status: {})",
                synced_status
            )));
        }
    }
    // Non-docker: SSH connected successfully in get_executor → running

    sqlx::query!("UPDATE agents SET status = 'running' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    state
        .agent_status_cache
        .set(id.clone(), "running".into())
        .await;

    Ok(Json(
        serde_json::json!({"message": "Agent started", "status": "running"}),
    ))
}

pub async fn stop_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&auth)?;
    let (executor, agent_info) = get_executor(&state, &id).await?;

    if agent_info.use_docker {
        let container_name = docker_container_name(&agent_info)?;
        executor
            .execute(&format!("docker stop {}", container_name))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let synced_status =
            sync_docker_agent_status_with_executor(&state, &id, &executor, &agent_info).await?;
        if synced_status == "running" {
            return Err(AppError::Conflict(
                "Docker agent is still running after stop".into(),
            ));
        }
    }

    sqlx::query!("UPDATE agents SET status = 'stopped' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    state
        .agent_status_cache
        .set(id.clone(), "stopped".into())
        .await;

    Ok(Json(
        serde_json::json!({"message": "Agent stopped", "status": "stopped"}),
    ))
}

pub async fn restart_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&auth)?;
    let (executor, agent_info) = get_executor(&state, &id).await?;

    if !agent_info.use_docker {
        return Err(AppError::BadRequest(
            "Restart is only supported for Docker agents".into(),
        ));
    }

    let container_name = docker_container_name(&agent_info)?;
    executor
        .execute(&format!("docker restart {}", container_name))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let synced_status =
        sync_docker_agent_status_with_executor(&state, &id, &executor, &agent_info).await?;
    if synced_status != "running" {
        return Err(AppError::Conflict(format!(
            "Docker agent did not restart successfully (current status: {})",
            synced_status
        )));
    }

    sqlx::query!("UPDATE agents SET status = 'running' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    state
        .agent_status_cache
        .set(id.clone(), "running".into())
        .await;

    Ok(Json(
        serde_json::json!({"message": "Agent restarted", "status": "running"}),
    ))
}

pub async fn clone_agent(
    State(state): State<AppContext>,
    Extension(auth): Extension<shared_kernel::AuthContext>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Agent>)> {
    require_admin(&auth)?;
    let row = sqlx::query!(
        r#"SELECT name, server_id, user_id, git_repo, git_branch, git_auth_type, git_username,
           git_password_encrypted, cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, use_docker
           FROM agents WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let new_id = Uuid::new_v4().to_string();
    let name = format!("{} (copy)", row.name);
    let workdir = agent_workspace_dir(&new_id);
    let docker_container_name = if row.use_docker {
        Some(format!("codex-fleet-{}", &new_id[..8]))
    } else {
        None
    };
    let now = Utc::now();

    sqlx::query!(
        r#"INSERT INTO agents (id, name, server_id, user_id, git_repo, git_branch, git_auth_type, git_username,
           git_password_encrypted, cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, workdir, use_docker,
           status, provision_log, provision_steps, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, 'stopped', '', '{}', $18)"#,
        new_id, name, row.server_id, row.user_id, row.git_repo, row.git_branch, row.git_auth_type, row.git_username,
        row.git_password_encrypted, row.cli_type, row.codex_config_id, row.agents_md_id, row.docker_config_id,
        row.docker_image, docker_container_name, workdir, row.use_docker, now
    )
    .execute(&state.db)
    .await?;

    // Resolve user display name
    let user_display_name = if let Some(ref uid) = row.user_id {
        sqlx::query_scalar!("SELECT display_name FROM users WHERE id = $1", uid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
    } else {
        None
    };

    Ok((
        StatusCode::CREATED,
        Json(Agent {
            id: new_id,
            name,
            server_id: row.server_id,
            user_id: row.user_id,
            user_display_name,
            git_repo: row.git_repo,
            git_branch: row.git_branch,
            git_auth_type: row.git_auth_type,
            git_username: row.git_username,
            cli_type: row.cli_type,
            codex_config_id: row.codex_config_id,
            agents_md_id: row.agents_md_id,
            docker_config_id: row.docker_config_id,
            docker_image: row.docker_image,
            docker_container_name,
            container_id: None,
            workdir,
            use_docker: row.use_docker,
            status: "stopped".into(),
            provision_log: String::new(),
            provision_steps: serde_json::json!({}),
            is_busy: false,
            created_at: now.to_string(),
        }),
    ))
}

#[derive(Serialize)]
pub struct TerminalCommandResponse {
    pub local_cmd: String,
    pub ssh_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_input_cmd: Option<String>,
}

pub async fn terminal_command(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<TerminalCommandResponse>> {
    let agent = sqlx::query(
        "SELECT server_id, docker_container_name, workdir, use_docker FROM agents WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let server_id: String = agent.get("server_id");
    let docker_container_name: Option<String> = agent.get("docker_container_name");
    let workdir: String = agent.get("workdir");
    let use_docker: bool = agent.get("use_docker");

    let (local_cmd, ssh_cmd) = if use_docker {
        let container = docker_container_name.unwrap_or_default();
        let inner = shell_quote("cd /workspace && exec bash");
        let local = format!("docker exec -it {} bash -lc {}", container, inner);
        let ssh = if let Ok(server) = sqlx::query!(
            "SELECT ip, port, username FROM servers WHERE id = $1",
            server_id
        )
        .fetch_one(&state.db)
        .await
        {
            Some(format!(
                "ssh {}@{} -p {} -t \"{}\"",
                server.username, server.ip, server.port, local
            ))
        } else {
            None
        };
        (local, ssh)
    } else {
        let local = format!(
            "cd {} && exec \"${{SHELL:-bash}}\" -l",
            shell_quote(&workdir)
        );
        let ssh = if let Ok(server) = sqlx::query!(
            "SELECT ip, port, username FROM servers WHERE id = $1",
            server_id
        )
        .fetch_one(&state.db)
        .await
        {
            Some(format!(
                "ssh {}@{} -p {} -t 'cd {} && exec bash -l'",
                server.username,
                server.ip,
                server.port,
                shell_quote(&workdir)
            ))
        } else {
            None
        };
        (local, ssh)
    };

    Ok(Json(TerminalCommandResponse {
        local_cmd,
        ssh_cmd,
        terminal_input_cmd: None,
    }))
}

#[derive(Deserialize)]
pub struct ResumeCommandQuery {
    pub thread_id: String,
}

pub async fn resume_command(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Query(params): Query<ResumeCommandQuery>,
) -> Result<Json<TerminalCommandResponse>> {
    let agent = sqlx::query(
        "SELECT server_id, docker_container_name, workdir, use_docker FROM agents WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let server_id: String = agent.get("server_id");
    let docker_container_name: Option<String> = agent.get("docker_container_name");
    let workdir: String = agent.get("workdir");
    let use_docker: bool = agent.get("use_docker");

    let terminal_input_cmd = resume_terminal_input_command(use_docker, &workdir, &params.thread_id);

    let (local_cmd, ssh_cmd) = if use_docker {
        let container = docker_container_name.unwrap_or_default();
        let inner = format!("cd /workspace && {}", terminal_input_cmd);
        let local = format!(
            "docker exec -it {} bash -lc {}",
            container,
            shell_quote(&inner)
        );
        let ssh = if let Ok(server) = sqlx::query!(
            "SELECT ip, port, username FROM servers WHERE id = $1",
            server_id
        )
        .fetch_one(&state.db)
        .await
        {
            Some(format!(
                "ssh {}@{} -p {} -t \"{}\"",
                server.username, server.ip, server.port, local
            ))
        } else {
            None
        };
        (local, ssh)
    } else {
        let inner = format!(
            "cd {} && {}{}",
            shell_quote(&workdir),
            HOST_ENV_SETUP,
            terminal_input_cmd
        );
        let local = inner.clone();
        let ssh = if let Ok(server) = sqlx::query!(
            "SELECT ip, port, username FROM servers WHERE id = $1",
            server_id
        )
        .fetch_one(&state.db)
        .await
        {
            Some(format!(
                "ssh {}@{} -p {} -t {}",
                server.username,
                server.ip,
                server.port,
                shell_quote(&inner)
            ))
        } else {
            None
        };
        (local, ssh)
    };

    Ok(Json(TerminalCommandResponse {
        local_cmd,
        ssh_cmd,
        terminal_input_cmd: Some(terminal_input_cmd),
    }))
}

#[derive(Serialize)]
pub struct CheckResumeProcessResponse {
    pub running: bool,
    pub count: i32,
}

pub async fn check_resume_process(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Query(params): Query<ResumeCommandQuery>,
) -> Result<Json<CheckResumeProcessResponse>> {
    let (executor, agent_info) = get_executor(&state, &id).await?;

    let tid = &params.thread_id;
    let pattern = format!(
        "[c]odex resume {tid}|[c]laude resume {tid}|[g]emini resume {tid}|[o]pencode resume {tid}"
    );

    let cmd = if agent_info.use_docker {
        let container = docker_container_name(&agent_info)?;
        format!(
            "docker exec {} pgrep -c -f '{}' 2>/dev/null || echo 0",
            container, pattern
        )
    } else {
        format!("pgrep -c -f '{}' 2>/dev/null || echo 0", pattern)
    };

    let output = executor
        .execute(&cmd)
        .await
        .unwrap_or_else(|_| "0".to_string());
    let count: i32 = output.trim().parse().unwrap_or(0);

    Ok(Json(CheckResumeProcessResponse {
        running: count > 0,
        count,
    }))
}

pub struct AgentRow {
    pub docker_container_name: Option<String>,
    pub workdir: String,
    pub use_docker: bool,
    pub status: String,
}

pub struct ServerCredentials {
    pub ip: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub ssh_key_content: Option<String>,
}

/// Get agent info + server credentials in a single JOIN query (used by scheduler/dispatch).
pub async fn get_agent_with_credentials(
    state: &AppContext,
    agent_id: &str,
) -> Result<(ServerCredentials, AgentRow)> {
    let row = sqlx::query(
        "SELECT a.docker_container_name, a.workdir, a.use_docker, a.status,
                s.ip, s.port, s.username, s.auth_type, s.password_encrypted, s.ssh_key_content
         FROM agents a
         JOIN servers s ON s.id = a.server_id
         WHERE a.id = $1",
    )
    .bind(agent_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let crypto = Crypto::new(&state.config.master_key);
    let password: Option<String> = row
        .get::<Option<String>, _>("password_encrypted")
        .as_deref()
        .and_then(|p| crypto.decrypt(p).ok());

    Ok((
        ServerCredentials {
            ip: row.get("ip"),
            port: row.get::<i64, _>("port") as u16,
            username: row.get("username"),
            auth_type: row.get("auth_type"),
            password,
            ssh_key_content: row.get("ssh_key_content"),
        },
        AgentRow {
            docker_container_name: row.get("docker_container_name"),
            workdir: row.get("workdir"),
            use_docker: row.get("use_docker"),
            status: row.get("status"),
        },
    ))
}

/// Get server credentials and agent info for an agent (used by WS handlers that need raw SSH).
pub async fn get_server_credentials(
    state: &AppContext,
    agent_id: &str,
) -> Result<(ServerCredentials, AgentRow)> {
    let agent = sqlx::query(
        "SELECT server_id, docker_container_name, workdir, use_docker, status FROM agents WHERE id = $1",
    )
    .bind(agent_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let server_id: String = agent.get("server_id");

    let agent_row = AgentRow {
        docker_container_name: agent.get("docker_container_name"),
        workdir: agent.get("workdir"),
        use_docker: agent.get("use_docker"),
        status: agent.get("status"),
    };

    let server = sqlx::query!(
        "SELECT ip, port, username, auth_type, password_encrypted, ssh_key_content FROM servers WHERE id = $1",
        server_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Server {} not found", server_id)))?;

    let crypto = Crypto::new(&state.config.master_key);
    let password = server
        .password_encrypted
        .as_deref()
        .and_then(|p| crypto.decrypt(p).ok());

    Ok((
        ServerCredentials {
            ip: server.ip,
            port: server.port as u16,
            username: server.username,
            auth_type: server.auth_type,
            password,
            ssh_key_content: server.ssh_key_content,
        },
        agent_row,
    ))
}

/// Get an Executor (SSH or Local) and agent row info for an agent.
pub async fn get_executor(state: &AppContext, agent_id: &str) -> Result<(Executor, AgentRow)> {
    let agent = sqlx::query(
        "SELECT server_id, docker_container_name, workdir, use_docker, status FROM agents WHERE id = $1",
    )
    .bind(agent_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let server_id: String = agent.get("server_id");

    let agent_row = AgentRow {
        docker_container_name: agent.get("docker_container_name"),
        workdir: agent.get("workdir"),
        use_docker: agent.get("use_docker"),
        status: agent.get("status"),
    };

    let server = sqlx::query!(
        "SELECT ip, port, username, auth_type, password_encrypted, ssh_key_content FROM servers WHERE id = $1",
        server_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Server {} not found", server_id)))?;

    let crypto = Crypto::new(&state.config.master_key);
    let password = server
        .password_encrypted
        .as_deref()
        .and_then(|p| crypto.decrypt(p).ok());

    let client = SshClientPool::connect(
        &server.ip,
        server.port as u16,
        &server.username,
        &server.auth_type,
        password.as_deref(),
        server.ssh_key_content.as_deref(),
    )
    .await
    .map_err(|e| AppError::Ssh(e.to_string()))?;

    Ok((Executor::Ssh(client), agent_row))
}
