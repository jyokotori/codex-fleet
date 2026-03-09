use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
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
};
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

fn target_shell_command(use_docker: bool, container_name: &str, cmd: &str) -> String {
    if use_docker {
        format!("docker exec {} sh -lc {}", container_name, shell_quote(cmd))
    } else {
        cmd.to_string()
    }
}

fn tmux_session_for_cli(cli_type: &str) -> &str {
    match cli_type {
        "codex" => "codex",
        "claude" | "claude_code" => "claude",
        "gemini" | "gemini_cli" => "gemini",
        "opencode" => "opencode",
        _ => "main",
    }
}

#[derive(Serialize, Clone)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub server_id: String,
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
    pub tmux_session: String,
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
    pub git_repo: Option<String>,
    pub git_branch: Option<String>,
    pub force_reclone: Option<bool>,
    pub codex_config_id: Option<String>,
    pub agents_md_id: Option<String>,
    pub docker_config_id: Option<String>,
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
    if !agent_info.use_docker || agent_info.status == "provisioning" {
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

async fn sync_docker_statuses(state: &AppContext, agents: &mut [Agent]) {
    let mut targets_by_server: HashMap<String, Vec<(usize, DockerSyncTarget)>> = HashMap::new();

    for (index, agent) in agents.iter().enumerate() {
        if !agent.use_docker || agent.status == "provisioning" {
            continue;
        }

        if let Some(container_name) = agent
            .docker_container_name
            .as_ref()
            .filter(|name| !name.is_empty())
        {
            targets_by_server
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
    }

    let mut updates = Vec::new();

    for (server_id, targets) in targets_by_server {
        let executor = match connect_executor_for_server(state, &server_id).await {
            Ok(executor) => executor,
            Err(err) => {
                tracing::warn!("Failed to sync docker status for server {}: {}", server_id, err);
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

    for (agent_id, status) in updates {
        if let Err(err) = sqlx::query("UPDATE agents SET status = $1 WHERE id = $2")
            .bind(&status)
            .bind(&agent_id)
            .execute(&state.db)
            .await
        {
            tracing::warn!("Failed to persist synced status for agent {}: {}", agent_id, err);
        }
    }
}

pub async fn sync_agent_status(state: &AppContext, agent_id: &str) -> Result<String> {
    let (executor, agent_info) = get_executor(state, agent_id).await?;
    sync_docker_agent_status_with_executor(state, agent_id, &executor, &agent_info).await
}

fn docker_container_name(agent_info: &AgentRow) -> Result<&str> {
    agent_info
        .docker_container_name
        .as_deref()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| AppError::BadRequest("Docker container not configured".into()))
}

async fn ensure_tmux_session(executor: &Executor, agent_info: &AgentRow) -> Result<()> {
    let tmux_cmd = if agent_info.use_docker {
        let container_name = docker_container_name(agent_info)?;
        format!(
            "docker exec {} tmux new-session -d -s {} -c /workspace 2>/dev/null || true",
            container_name, agent_info.tmux_session
        )
    } else {
        format!(
            "tmux new-session -d -s {} -c {} 2>/dev/null || true",
            agent_info.tmux_session, agent_info.workdir
        )
    };

    executor
        .execute(&tmux_cmd)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(())
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

pub async fn list_agents(State(state): State<AppContext>) -> Result<Json<Vec<Agent>>> {
    let rows = sqlx::query!(
        r#"SELECT id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, container_id,
           tmux_session, workdir, use_docker, status, provision_log, provision_steps, created_at
           FROM agents ORDER BY created_at DESC"#
    )
    .fetch_all(&state.db)
    .await?;

    // Build a set of agent IDs that are busy (have active work items)
    let busy_rows = sqlx::query_scalar!(
        r#"SELECT DISTINCT assigned_agent_id AS "id!" FROM work_items
           WHERE status IN ('agent_in_progress','agent_completed') AND assigned_agent_id IS NOT NULL"#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let busy_set: std::collections::HashSet<String> = busy_rows.into_iter().collect();

    let mut agents = rows
        .into_iter()
        .map(|r| {
            let is_busy = busy_set.contains(&r.id);
            Agent {
                id: r.id,
                name: r.name,
                server_id: r.server_id,
                git_repo: r.git_repo,
                git_branch: r.git_branch,
                git_auth_type: r.git_auth_type,
                git_username: r.git_username,
                cli_type: r.cli_type,
                codex_config_id: r.codex_config_id,
                agents_md_id: r.agents_md_id,
                docker_config_id: r.docker_config_id,
                docker_image: r.docker_image,
                docker_container_name: r.docker_container_name,
                container_id: r.container_id,
                tmux_session: r.tmux_session,
                workdir: r.workdir,
                use_docker: r.use_docker,
                status: r.status,
                provision_log: r.provision_log,
                provision_steps: r.provision_steps,
                is_busy,
                created_at: r.created_at.to_string(),
            }
        })
        .collect::<Vec<_>>();

    sync_docker_statuses(&state, &mut agents).await;

    Ok(Json(agents))
}

pub async fn create_agent(
    State(state): State<AppContext>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<Agent>> {
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
    let tmux_session = tmux_session_for_cli(&req.cli_type).to_string();
    let workdir = if use_docker {
        "/workspace".to_string()
    } else {
        format!("$HOME/.codex-fleet/{}/workspace", id)
    };
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
        r#"INSERT INTO agents (id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           git_password_encrypted, cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, tmux_session, workdir, use_docker, status, provision_log, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, 'provisioning', '', $18)"#,
        id, req.name, req.server_id, git_repo, git_branch, git_auth_type, req.git_username,
        git_password_encrypted, req.cli_type, req.codex_config_id, req.agents_md_id,
        req.docker_config_id, docker_image, container_name_db, tmux_session, workdir,
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

    tokio::spawn(async move {
        let crypto = Crypto::new(&master_key);
        let password = ssh_password_enc
            .as_deref()
            .and_then(|p| crypto.decrypt(p).ok());
        let executor = match SshClientPool::connect(
            &ssh_ip,
            ssh_port as u16,
            &ssh_username,
            &ssh_auth_type,
            password.as_deref(),
            ssh_key_content.as_deref(),
        )
        .await
        {
            Ok(client) => Executor::Ssh(client),
            Err(e) => {
                emit(
                    &db,
                    &agent_id,
                    &tx,
                    ev_step_failed(0, &format!("SSH connect failed: {}", e)),
                )
                .await;
                emit(&db, &agent_id, &tx, ev_provision_done("error")).await;
                let _ = sqlx::query!("UPDATE agents SET status = 'error' WHERE id = $1", agent_id)
                    .execute(&db)
                    .await;
                provision_channels.lock().await.remove(&agent_id);
                return;
            }
        };

        match provision_agent(
            &executor,
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
            }
            Err(e) => {
                tracing::error!("Agent {} provisioning failed: {}", agent_id, e);
            }
        }
        provision_channels.lock().await.remove(&agent_id);
    });

    Ok(Json(Agent {
        id,
        name: req.name,
        server_id: req.server_id,
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
        tmux_session,
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
    executor: &Executor,
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
    let workspace_dir = if use_docker {
        "/workspace".to_string()
    } else {
        format!("$HOME/.codex-fleet/{}/workspace", agent_id)
    };

    // Step 1: Create dirs + write config files
    emit(
        db,
        agent_id,
        tx,
        ev_step_start(1, "Create dirs & write configs"),
    )
    .await;
    let dir_cmd = format!(
        "mkdir -p $HOME/.codex-fleet/{id}/agent $HOME/.codex-fleet/{id}/workspace",
        id = agent_id
    );
    match executor.execute(&dir_cmd).await {
        Ok(out) => {
            if !out.trim().is_empty() {
                emit(db, agent_id, tx, ev_step_output(1, out.trim())).await;
            }
            emit(
                db,
                agent_id,
                tx,
                ev_step_output(1, &format!("Created: ~/.codex-fleet/{}/", agent_id)),
            )
            .await;
        }
        Err(e) => {
            let err = format!("Step 1 failed: {}", e);
            emit(db, agent_id, tx, ev_step_failed(1, &err)).await;
            emit(db, agent_id, tx, ev_provision_done("error")).await;
            let _ = sqlx::query!("UPDATE agents SET status = 'error' WHERE id = $1", agent_id)
                .execute(db)
                .await;
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
                let b64 = BASE64.encode(row.config_toml.as_bytes());
                let cmd = format!(
                    "echo '{}' | base64 -d > $HOME/.codex-fleet/{}/agent/config.toml",
                    b64, agent_id
                );
                if let Err(e) = executor.execute(&cmd).await {
                    emit(
                        db,
                        agent_id,
                        tx,
                        ev_warn(1, &format!("config.toml write failed: {}", e)),
                    )
                    .await;
                } else {
                    emit(db, agent_id, tx, ev_step_output(1, "Wrote config.toml")).await;
                }
            }

            if !auth_json_content.is_empty() {
                let b64 = BASE64.encode(auth_json_content.as_bytes());
                let cmd = format!(
                    "echo '{}' | base64 -d > $HOME/.codex-fleet/{}/agent/auth.json",
                    b64, agent_id
                );
                if let Err(e) = executor.execute(&cmd).await {
                    emit(
                        db,
                        agent_id,
                        tx,
                        ev_warn(1, &format!("auth.json write failed: {}", e)),
                    )
                    .await;
                } else {
                    emit(db, agent_id, tx, ev_step_output(1, "Wrote auth.json")).await;
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
                let b64 = BASE64.encode(row.content.as_bytes());
                let cmd = format!(
                    "echo '{}' | base64 -d > $HOME/.codex-fleet/{}/agent/AGENTS.md",
                    b64, agent_id
                );
                if let Err(e) = executor.execute(&cmd).await {
                    emit(
                        db,
                        agent_id,
                        tx,
                        ev_warn(1, &format!("AGENTS.md write failed: {}", e)),
                    )
                    .await;
                } else {
                    emit(db, agent_id, tx, ev_step_output(1, "Wrote AGENTS.md")).await;
                }
            }
        }
    }
    emit(db, agent_id, tx, ev_step_done(1)).await;

    // Step 2: Docker start + run init_script (skip if !use_docker)
    if use_docker {
        emit(db, agent_id, tx, ev_step_start(2, "Docker setup")).await;

        let _ = executor
            .execute(&format!(
                "docker rm -f {} 2>/dev/null || true",
                container_name
            ))
            .await;
        let mut docker_run = format!(
            "docker run -d --name {container} --workdir /workspace \
             -v $HOME/.codex-fleet/{id}/agent:/agent \
             -v $HOME/.codex-fleet/{id}/workspace:/workspace",
            container = container_name,
            id = agent_id,
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
        emit(
            db,
            agent_id,
            tx,
            ev_step_output(2, &format!("$ {}", docker_run)),
        )
        .await;

        match executor.execute(&docker_run).await {
            Ok(container_id_raw) => {
                let cid = container_id_raw.trim().to_string();
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
            Err(e) => {
                let err = format!("Step 2 failed (docker run): {}", e);
                emit(db, agent_id, tx, ev_step_failed(2, &err)).await;
                emit(db, agent_id, tx, ev_provision_done("error")).await;
                let _ = sqlx::query!("UPDATE agents SET status = 'error' WHERE id = $1", agent_id)
                    .execute(db)
                    .await;
                return Err(anyhow::anyhow!(err));
            }
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
                    let cmd = target_shell_command(true, container_name, &dc.init_script);
                    match executor.execute(&cmd).await {
                        Ok(out) => {
                            if !out.trim().is_empty() {
                                emit(db, agent_id, tx, ev_step_output(2, out.trim())).await;
                            }
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
                    }
                }
            }
        }
        emit(db, agent_id, tx, ev_step_done(2)).await;
    } else {
        emit(db, agent_id, tx, ev_step_skipped(2, "no-docker mode")).await;
    }

    // Step 3: Install CLI + tmux + git (inside docker if use_docker)
    emit(
        db,
        agent_id,
        tx,
        ev_step_start(3, "Install CLI & environment"),
    )
    .await;

    if cli_type == "codex" {
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
        let ensure_npm_cmd = target_shell_command(use_docker, container_name, ensure_npm_script);
        match executor.execute(&ensure_npm_cmd).await {
            Ok(out) => {
                if !out.trim().is_empty() {
                    emit(db, agent_id, tx, ev_step_output(3, out.trim())).await;
                }
            }
            Err(e) => {
                let err = format!("Step 3 failed (npm check/install): {}", e);
                emit(db, agent_id, tx, ev_step_failed(3, &err)).await;
                emit(db, agent_id, tx, ev_provision_done("error")).await;
                let _ = sqlx::query!("UPDATE agents SET status = 'error' WHERE id = $1", agent_id)
                    .execute(db)
                    .await;
                return Err(anyhow::anyhow!(err));
            }
        }

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
        match executor.execute(&install_codex_cmd).await {
            Ok(out) => {
                if !out.trim().is_empty() {
                    emit(db, agent_id, tx, ev_step_output(3, out.trim())).await;
                }
            }
            Err(e) => {
                let err = format!("Step 3 failed (codex install): {}", e);
                emit(db, agent_id, tx, ev_step_failed(3, &err)).await;
                emit(db, agent_id, tx, ev_provision_done("error")).await;
                let _ = sqlx::query!("UPDATE agents SET status = 'error' WHERE id = $1", agent_id)
                    .execute(db)
                    .await;
                return Err(anyhow::anyhow!(err));
            }
        }

        if use_docker {
            let link_cmd = target_shell_command(
                true,
                container_name,
                "mkdir -p /root && ln -sfn /agent /root/.codex",
            );
            if let Err(e) = executor.execute(&link_cmd).await {
                emit(
                    db,
                    agent_id,
                    tx,
                    ev_warn(3, &format!("link /agent -> /root/.codex failed: {}", e)),
                )
                .await;
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

    // Install tmux (check first)
    let ensure_tmux_script = r#"if command -v tmux >/dev/null 2>&1; then
  echo "tmux already installed"
else
  run_pm() {
    if [ "$(id -u)" -eq 0 ]; then "$@";
    elif command -v sudo >/dev/null 2>&1; then sudo "$@";
    else "$@";
    fi
  }
  if command -v apt-get >/dev/null 2>&1; then
    run_pm apt-get update -qq && DEBIAN_FRONTEND=noninteractive run_pm apt-get install -y tmux -qq
  elif command -v dnf >/dev/null 2>&1; then
    run_pm dnf install -y tmux
  elif command -v yum >/dev/null 2>&1; then
    run_pm yum install -y tmux
  elif command -v apk >/dev/null 2>&1; then
    run_pm apk add --no-cache tmux
  else
    echo "tmux not found and no supported package manager" >&2 || true
  fi
fi"#;
    let ensure_tmux_cmd = target_shell_command(use_docker, container_name, ensure_tmux_script);
    match executor.execute(&ensure_tmux_cmd).await {
        Ok(out) => {
            if !out.trim().is_empty() {
                emit(db, agent_id, tx, ev_step_output(3, out.trim())).await;
            }
        }
        Err(e) => {
            emit(
                db,
                agent_id,
                tx,
                ev_warn(3, &format!("tmux install failed (non-fatal): {}", e)),
            )
            .await;
        }
    }

    // Install git (check first)
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
    match executor.execute(&ensure_git_cmd).await {
        Ok(out) => {
            if !out.trim().is_empty() {
                emit(db, agent_id, tx, ev_step_output(3, out.trim())).await;
            }
            emit(db, agent_id, tx, ev_step_done(3)).await;
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
                let err = format!("Step 3 failed (git install): {}", e);
                emit(db, agent_id, tx, ev_step_failed(3, &err)).await;
                emit(db, agent_id, tx, ev_provision_done("error")).await;
                let _ = sqlx::query!("UPDATE agents SET status = 'error' WHERE id = $1", agent_id)
                    .execute(db)
                    .await;
                return Err(anyhow::anyhow!(err));
            }
        }
    }

    // Step 4: Git clone / sync (skip if no git_repo)
    if !git_repo.is_empty() {
        emit(db, agent_id, tx, ev_step_start(4, "Git clone / sync")).await;
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
        match executor.execute(&git_sync_cmd).await {
            Ok(out) => {
                if !out.trim().is_empty() {
                    emit(db, agent_id, tx, ev_step_output(4, out.trim())).await;
                }
                emit(db, agent_id, tx, ev_step_done(4)).await;
            }
            Err(e) => {
                let err = format!("Step 4 failed: {}", e);
                emit(db, agent_id, tx, ev_step_failed(4, &err)).await;
                emit(db, agent_id, tx, ev_provision_done("error")).await;
                let _ = sqlx::query!("UPDATE agents SET status = 'error' WHERE id = $1", agent_id)
                    .execute(db)
                    .await;
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

    // Done
    emit(db, agent_id, tx, ev_provision_done("stopped")).await;
    let _ = sqlx::query!(
        "UPDATE agents SET status = 'stopped' WHERE id = $1",
        agent_id
    )
    .execute(db)
    .await;

    Ok(())
}

pub async fn update_agent(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> std::result::Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let existing = sqlx::query!(
        r#"SELECT id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, container_id,
           tmux_session, workdir, use_docker, status, provision_log, provision_steps, created_at
           FROM agents WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let codex_config_changed = req.codex_config_id.is_some();
    let agents_md_changed = req.agents_md_id.is_some();

    let name = req.name.unwrap_or(existing.name);
    let git_branch = req.git_branch.unwrap_or(existing.git_branch.clone());
    let new_git_repo = req.git_repo.clone().unwrap_or(existing.git_repo.clone());
    let codex_config_id = req.codex_config_id.or(existing.codex_config_id.clone());
    let agents_md_id = req.agents_md_id.or(existing.agents_md_id.clone());
    let docker_config_id = req.docker_config_id.or(existing.docker_config_id.clone());
    let use_docker = existing.use_docker;

    let git_repo_changed = req.git_repo.is_some() && new_git_repo != existing.git_repo;
    if git_repo_changed && req.force_reclone != Some(true) {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "requires_confirm": true,
                "message": "Changing git_repo will clear the workspace and re-clone. Pass force_reclone=true to confirm."
            })),
        ));
    }

    sqlx::query!(
        "UPDATE agents SET name=$1, git_repo=$2, git_branch=$3, codex_config_id=$4, agents_md_id=$5, docker_config_id=$6 WHERE id=$7",
        name, new_git_repo, git_branch, codex_config_id, agents_md_id, docker_config_id, id
    )
    .execute(&state.db)
    .await?;

    let config_changed = codex_config_changed || agents_md_changed;
    let reclone_needed = git_repo_changed && req.force_reclone == Some(true);

    if config_changed || reclone_needed {
        let db = state.db.clone();
        let master_key = state.config.master_key.clone();
        let agent_id = id.clone();
        let container_name = existing.docker_container_name.clone().unwrap_or_default();
        let new_git_repo2 = new_git_repo.clone();
        let git_branch2 = git_branch.clone();
        let server_id = existing.server_id.clone();

        let server = sqlx::query!(
            "SELECT ip, port, username, auth_type, password_encrypted, ssh_key_content FROM servers WHERE id = $1",
            server_id
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Server {} not found", server_id)))?;

        let ssh_ip = server.ip;
        let ssh_port = server.port;
        let ssh_username = server.username;
        let ssh_auth_type = server.auth_type;
        let ssh_password_enc = server.password_encrypted;
        let ssh_key_content = server.ssh_key_content;
        let new_codex_config_id = codex_config_id.clone();
        let new_agents_md_id = agents_md_id.clone();

        tokio::spawn(async move {
            let crypto = Crypto::new(&master_key);
            let password = ssh_password_enc
                .as_deref()
                .and_then(|p| crypto.decrypt(p).ok());
            let executor = match SshClientPool::connect(
                &ssh_ip,
                ssh_port as u16,
                &ssh_username,
                &ssh_auth_type,
                password.as_deref(),
                ssh_key_content.as_deref(),
            )
            .await
            {
                Ok(c) => Executor::Ssh(c),
                Err(e) => {
                    tracing::error!("update_agent async connect failed: {}", e);
                    return;
                }
            };

            if config_changed {
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
                                "echo '{}' | base64 -d > $HOME/.codex-fleet/{}/agent/config.toml",
                                b64, agent_id
                            );
                            let _ = executor.execute(&cmd).await;
                        }
                        if !auth_json_content.is_empty() {
                            let b64 = BASE64.encode(auth_json_content.as_bytes());
                            let cmd = format!(
                                "echo '{}' | base64 -d > $HOME/.codex-fleet/{}/agent/auth.json",
                                b64, agent_id
                            );
                            let _ = executor.execute(&cmd).await;
                        }
                    }
                }

                if let Some(md_id) = new_agents_md_id {
                    if let Ok(row) =
                        sqlx::query!("SELECT content FROM company_configs WHERE id = $1", md_id)
                            .fetch_one(&db)
                            .await
                    {
                        if !row.content.is_empty() {
                            let b64 = BASE64.encode(row.content.as_bytes());
                            let cmd = format!(
                                "echo '{}' | base64 -d > $HOME/.codex-fleet/{}/agent/AGENTS.md",
                                b64, agent_id
                            );
                            let _ = executor.execute(&cmd).await;
                        }
                    }
                }
            }

            if reclone_needed {
                // Helper to append a JSONL event to provision_log without broadcasting
                let db_log = |db: &sqlx::PgPool, aid: &str, ev: Value| {
                    let db = db.clone();
                    let aid = aid.to_string();
                    async move {
                        let line = serde_json::to_string(&ev).unwrap_or_default() + "\n";
                        let _ = sqlx::query!(
                            "UPDATE agents SET provision_log = provision_log || $1 WHERE id = $2",
                            line,
                            aid
                        )
                        .execute(&db)
                        .await;
                    }
                };

                db_log(&db, &agent_id, json!({"t":"step_output","step":7,"text":"[Re-clone] Clearing workspace...","ts":unix_now()})).await;
                if use_docker && !container_name.is_empty() {
                    let clear_cmd =
                        format!("docker exec {} sh -c 'rm -rf /workspace/*'", container_name);
                    let _ = executor.execute(&clear_cmd).await;
                    let clone_cmd = format!(
                        "docker exec {} sh -c 'git clone {} /workspace && cd /workspace && git checkout {}'",
                        container_name, new_git_repo2, git_branch2
                    );
                    match executor.execute(&clone_cmd).await {
                        Ok(out) => {
                            db_log(&db, &agent_id, json!({"t":"step_output","step":7,"text":format!("[Re-clone] Done: {}", out.trim()),"ts":unix_now()})).await;
                        }
                        Err(e) => {
                            db_log(&db, &agent_id, json!({"t":"warn","step":7,"text":format!("[Re-clone] Failed: {}", e),"ts":unix_now()})).await;
                        }
                    }
                } else {
                    let workdir = format!("$HOME/.codex-fleet/{}/workspace", agent_id);
                    let clear_cmd = format!("rm -rf {}/*", workdir);
                    let _ = executor.execute(&clear_cmd).await;
                    let clone_cmd = format!(
                        "git clone {} {} && cd {} && git checkout {}",
                        new_git_repo2, workdir, workdir, git_branch2
                    );
                    match executor.execute(&clone_cmd).await {
                        Ok(out) => {
                            db_log(&db, &agent_id, json!({"t":"step_output","step":7,"text":format!("[Re-clone] Done: {}", out.trim()),"ts":unix_now()})).await;
                        }
                        Err(e) => {
                            db_log(&db, &agent_id, json!({"t":"warn","step":7,"text":format!("[Re-clone] Failed: {}", e),"ts":unix_now()})).await;
                        }
                    }
                }
            }
        });
    }

    let updated = sqlx::query!(
        r#"SELECT id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, container_id,
           tmux_session, workdir, use_docker, status, provision_log, provision_steps, created_at
           FROM agents WHERE id = $1"#,
        id
    )
    .fetch_one(&state.db)
    .await?;

    let agent = Agent {
        id: updated.id,
        name: updated.name,
        server_id: updated.server_id,
        git_repo: updated.git_repo,
        git_branch: updated.git_branch,
        git_auth_type: updated.git_auth_type,
        git_username: updated.git_username,
        cli_type: updated.cli_type,
        codex_config_id: updated.codex_config_id,
        agents_md_id: updated.agents_md_id,
        docker_config_id: updated.docker_config_id,
        docker_image: updated.docker_image,
        docker_container_name: updated.docker_container_name,
        container_id: updated.container_id,
        tmux_session: updated.tmux_session,
        workdir: updated.workdir,
        use_docker: updated.use_docker,
        status: updated.status,
        provision_log: updated.provision_log,
        provision_steps: updated.provision_steps,
        is_busy: false,
        created_at: updated.created_at.to_string(),
    };

    Ok((StatusCode::OK, Json(serde_json::to_value(agent).unwrap())))
}

pub async fn delete_agent(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (executor, agent_info) = get_executor(&state, &id).await?;
    let use_docker = agent_info.use_docker;

    if use_docker {
        let container = agent_info.docker_container_name.unwrap_or_default();
        if !container.is_empty() {
            let _ = executor
                .execute(&format!(
                    "docker stop {} 2>/dev/null; docker rm {} 2>/dev/null",
                    container, container
                ))
                .await;
        }
    }

    let _ = executor
        .execute(&format!("rm -rf $HOME/.codex-fleet/{}/", id))
        .await;

    sqlx::query!("DELETE FROM agents WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({"message": "Agent deleted"})))
}

pub async fn start_agent(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
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

    ensure_tmux_session(&executor, &agent_info).await?;

    sqlx::query!("UPDATE agents SET status = 'running' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(Json(
        serde_json::json!({"message": "Agent started", "status": "running"}),
    ))
}

pub async fn stop_agent(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
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
    } else {
        let tmux_session = agent_info.tmux_session;
        let _ = executor
            .execute(&format!(
                "tmux kill-session -t {} 2>/dev/null; true",
                tmux_session
            ))
            .await;
    }

    sqlx::query!("UPDATE agents SET status = 'stopped' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(Json(
        serde_json::json!({"message": "Agent stopped", "status": "stopped"}),
    ))
}

pub async fn restart_agent(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
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

    ensure_tmux_session(&executor, &agent_info).await?;

    sqlx::query!("UPDATE agents SET status = 'running' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(Json(
        serde_json::json!({"message": "Agent restarted", "status": "running"}),
    ))
}

pub async fn resume_agent(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (executor, agent_info) = get_executor(&state, &id).await?;
    let tmux_session = agent_info.tmux_session;

    let resume_cmd = match agent_info.cli_type.as_str() {
        "claude" | "claude_code" => "claude --resume",
        "codex" => "codex --resume",
        _ => {
            return Err(AppError::BadRequest(
                "CLI type not supported for resume".into(),
            ))
        }
    };

    let cmd = if agent_info.use_docker {
        let container_name = agent_info.docker_container_name.unwrap_or_default();
        format!(
            "docker exec {} tmux send-keys -t {} '{}' Enter",
            container_name, tmux_session, resume_cmd
        )
    } else {
        format!("tmux send-keys -t {} '{}' Enter", tmux_session, resume_cmd)
    };

    executor
        .execute(&cmd)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({"message": "Resume command sent"})))
}

pub async fn clone_agent(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Agent>)> {
    let row = sqlx::query!(
        r#"SELECT name, server_id, git_repo, git_branch, git_auth_type, git_username,
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
    let tmux_session = tmux_session_for_cli(&row.cli_type).to_string();
    let workdir = if row.use_docker {
        "/workspace".to_string()
    } else {
        format!("~/.codex-fleet/{}/workspace", new_id)
    };
    let docker_container_name = if row.use_docker {
        Some(format!("codex-fleet-{}", &new_id[..8]))
    } else {
        None
    };
    let now = Utc::now();

    sqlx::query!(
        r#"INSERT INTO agents (id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           git_password_encrypted, cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, tmux_session, workdir, use_docker,
           status, provision_log, provision_steps, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, 'stopped', '', '{}', $18)"#,
        new_id, name, row.server_id, row.git_repo, row.git_branch, row.git_auth_type, row.git_username,
        row.git_password_encrypted, row.cli_type, row.codex_config_id, row.agents_md_id, row.docker_config_id,
        row.docker_image, docker_container_name, tmux_session, workdir, row.use_docker, now
    )
    .execute(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(Agent {
        id: new_id,
        name,
        server_id: row.server_id,
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
        tmux_session,
        workdir,
        use_docker: row.use_docker,
        status: "stopped".into(),
        provision_log: String::new(),
        provision_steps: serde_json::json!({}),
        is_busy: false,
        created_at: now.to_string(),
    })))
}

#[derive(Serialize)]
pub struct TerminalCommandResponse {
    pub local_cmd: String,
    pub ssh_cmd: Option<String>,
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

    let (local_cmd, ssh_shell_cmd) = if use_docker {
        let container = docker_container_name.unwrap_or_default();
        (
            format!(
                "docker exec -it {} bash -lc {}",
                container,
                shell_quote("cd /workspace && exec bash")
            ),
            format!(
                "docker exec -it {} bash -lc {}",
                container,
                shell_quote("cd /workspace && exec bash")
            ),
        )
    } else {
        (
            format!(
                "cd {} && exec \"${{SHELL:-bash}}\" -l",
                shell_quote(&workdir)
            ),
            format!(
                "cd {} && exec \"${{SHELL:-bash}}\" -l",
                shell_quote(&workdir)
            ),
        )
    };

    let ssh_cmd = if let Ok(server) = sqlx::query!(
        "SELECT ip, port, username FROM servers WHERE id = $1",
        server_id
    )
    .fetch_one(&state.db)
    .await
    {
        if use_docker {
            Some(format!(
                "ssh {}@{} -p {} -t \"{}\"",
                server.username, server.ip, server.port, ssh_shell_cmd
            ))
        } else {
            Some(format!("ssh {}@{} -p {}", server.username, server.ip, server.port))
        }
    } else {
        None
    };

    Ok(Json(TerminalCommandResponse { local_cmd, ssh_cmd }))
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
        "SELECT server_id, docker_container_name, workdir, use_docker, cli_type FROM agents WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let server_id: String = agent.get("server_id");
    let docker_container_name: Option<String> = agent.get("docker_container_name");
    let workdir: String = agent.get("workdir");
    let use_docker: bool = agent.get("use_docker");
    let cli_type: String = agent.get("cli_type");

    let resume_cmd = match cli_type.as_str() {
        "codex" => format!("codex resume {}", params.thread_id),
        "claude" | "claude_code" => format!("claude resume {}", params.thread_id),
        _ => format!("codex resume {}", params.thread_id),
    };

    let (local_cmd, ssh_shell_cmd) = if use_docker {
        let container = docker_container_name.unwrap_or_default();
        let inner = format!("cd /workspace && {}", resume_cmd);
        (
            format!("docker exec -it {} bash -lc {}", container, shell_quote(&inner)),
            format!("docker exec -it {} bash -lc {}", container, shell_quote(&inner)),
        )
    } else {
        (
            format!("cd {} && {}", shell_quote(&workdir), resume_cmd),
            format!("cd {} && {}", shell_quote(&workdir), resume_cmd),
        )
    };

    let ssh_cmd = if let Ok(server) = sqlx::query!(
        "SELECT ip, port, username FROM servers WHERE id = $1",
        server_id
    )
    .fetch_one(&state.db)
    .await
    {
        Some(format!(
            "ssh {}@{} -p {} -t \"{}\"",
            server.username, server.ip, server.port, ssh_shell_cmd
        ))
    } else {
        None
    };

    Ok(Json(TerminalCommandResponse { local_cmd, ssh_cmd }))
}

pub struct AgentRow {
    pub tmux_session: String,
    pub docker_container_name: Option<String>,
    pub cli_type: String,
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

/// Get server credentials and agent info for an agent (used by WS handlers that need raw SSH).
pub async fn get_server_credentials(
    state: &AppContext,
    agent_id: &str,
) -> Result<(ServerCredentials, AgentRow)> {
    let agent = sqlx::query(
        "SELECT server_id, tmux_session, docker_container_name, cli_type, workdir, use_docker, status FROM agents WHERE id = $1",
    )
    .bind(agent_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let server_id: String = agent.get("server_id");

    let agent_row = AgentRow {
        tmux_session: agent.get("tmux_session"),
        docker_container_name: agent.get("docker_container_name"),
        cli_type: agent.get("cli_type"),
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

/// Get an Executor (SSH) and agent row info for an agent.
pub async fn get_executor(state: &AppContext, agent_id: &str) -> Result<(Executor, AgentRow)> {
    let agent = sqlx::query(
        "SELECT server_id, tmux_session, docker_container_name, cli_type, workdir, use_docker, status FROM agents WHERE id = $1",
    )
    .bind(agent_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let server_id: String = agent.get("server_id");

    let agent_row = AgentRow {
        tmux_session: agent.get("tmux_session"),
        docker_container_name: agent.get("docker_container_name"),
        cli_type: agent.get("cli_type"),
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
