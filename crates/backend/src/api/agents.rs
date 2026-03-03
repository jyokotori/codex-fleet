use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    crypto::Crypto,
    error::{AppError, Result},
    ssh::client::{SshClient, SshClientPool},
    AppState,
};

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

fn cli_config_dir(cli_type: &str) -> &str {
    match cli_type {
        "codex" => "/root/.codex",
        "claude" | "claude_code" => "/root/.claude",
        "gemini" | "gemini_cli" => "/root/.gemini",
        "opencode" => "/root/.opencode",
        _ => "/root/.config/cli",
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

async fn append_provision_log(db: &sqlx::PgPool, agent_id: &str, msg: &str) {
    let _ = sqlx::query!(
        "UPDATE agents SET provision_log = provision_log || $1 WHERE id = $2",
        msg,
        agent_id
    )
    .execute(db)
    .await;
}

pub async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<Agent>>> {
    let rows = sqlx::query!(
        r#"SELECT id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, container_id,
           tmux_session, workdir, use_docker, status, provision_log, created_at
           FROM agents ORDER BY created_at DESC"#
    )
    .fetch_all(&state.db)
    .await?;

    let agents = rows
        .into_iter()
        .map(|r| Agent {
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
            created_at: r.created_at.to_string(),
        })
        .collect();

    Ok(Json(agents))
}

pub async fn create_agent(
    State(state): State<AppState>,
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
    let git_branch = req.git_branch.unwrap_or_else(|| "main".into());
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

    tokio::spawn(async move {
        let crypto = Crypto::new(&master_key);
        let password = ssh_password_enc.as_deref().and_then(|p| crypto.decrypt(p).ok());
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
                let msg = format!("\n[Error] SSH connect failed: {}\n", e);
                append_provision_log(&db, &agent_id, &msg).await;
                let _ = sqlx::query!(
                    "UPDATE agents SET status = 'error' WHERE id = $1",
                    agent_id
                )
                .execute(&db)
                .await;
                return;
            }
        };

        match provision_agent(
            &executor,
            &db,
            &agent_id,
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
                let msg = format!("\n[Error] Provisioning failed: {}\n", e);
                append_provision_log(&db, &agent_id, &msg).await;
                let _ = sqlx::query!(
                    "UPDATE agents SET status = 'error' WHERE id = $1",
                    agent_id
                )
                .execute(&db)
                .await;
                tracing::error!("Agent {} provisioning failed: {}", agent_id, e);
            }
        }
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
        created_at: now.to_string(),
    }))
}

#[allow(clippy::too_many_arguments)]
async fn provision_agent(
    executor: &Executor,
    db: &sqlx::PgPool,
    agent_id: &str,
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
    let log = |msg: &str| format!("{}\n", msg);

    // Step 1: Create directories
    append_provision_log(db, agent_id, &log("[Step 1] Create directories")).await;
    let dir_cmd = format!(
        "mkdir -p $HOME/.codex-fleet/{id}/agent $HOME/.codex-fleet/{id}/workspace",
        id = agent_id
    );
    match executor.execute(&dir_cmd).await {
        Ok(out) => {
            if !out.is_empty() {
                append_provision_log(db, agent_id, &log(&format!("  {}", out.trim()))).await;
            }
            append_provision_log(db, agent_id, &log(&format!("  Created: ~/.codex-fleet/{}/", agent_id))).await;
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Step 1 failed: {}", e));
        }
    }

    // Step 2: Write config files
    append_provision_log(db, agent_id, &log("[Step 2] Write config files")).await;

    if let Some(config_id) = codex_config_id {
        if let Ok(row) = sqlx::query!(
            "SELECT config_toml, auth_json FROM codex_configs WHERE id = $1",
            config_id
        )
        .fetch_one(db)
        .await
        {
            let crypto = crate::crypto::Crypto::new(master_key);
            let auth_json_content = if row.auth_json.starts_with("enc:") {
                crypto.decrypt(row.auth_json.trim_start_matches("enc:")).unwrap_or(row.auth_json.clone())
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
                    append_provision_log(db, agent_id, &log(&format!("  [warn] config.toml write failed: {}", e))).await;
                } else {
                    append_provision_log(db, agent_id, &log("  Wrote config.toml")).await;
                }
            }

            if !auth_json_content.is_empty() {
                let b64 = BASE64.encode(auth_json_content.as_bytes());
                let cmd = format!(
                    "echo '{}' | base64 -d > $HOME/.codex-fleet/{}/agent/auth.json",
                    b64, agent_id
                );
                if let Err(e) = executor.execute(&cmd).await {
                    append_provision_log(db, agent_id, &log(&format!("  [warn] auth.json write failed: {}", e))).await;
                } else {
                    append_provision_log(db, agent_id, &log("  Wrote auth.json")).await;
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
                    append_provision_log(db, agent_id, &log(&format!("  [warn] AGENTS.md write failed: {}", e))).await;
                } else {
                    append_provision_log(db, agent_id, &log("  Wrote AGENTS.md")).await;
                }
            }
        }
    }

    if use_docker {
        // Step 3: Start Docker container
        append_provision_log(db, agent_id, &log("[Step 3] Start Docker container")).await;

        let config_dir = cli_config_dir(cli_type);
        let mut docker_run = format!(
            "docker run -d --name {container} \
             -v $HOME/.codex-fleet/{id}/agent:{config_dir} \
             -v $HOME/.codex-fleet/{id}/workspace:/workspace",
            container = container_name,
            id = agent_id,
            config_dir = config_dir,
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
                            let cont = p.get("container_port").and_then(|v| v.as_str()).unwrap_or("");
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
                                docker_run.push_str(&format!(" -e {}={}", key, val));
                                env_count += 1;
                            }
                        }
                        if env_count > 0 {
                            append_provision_log(db, agent_id, &log(&format!("  Injecting {} env var(s)", env_count))).await;
                        }
                    }
                }
                if let Ok(vols) = serde_json::from_str::<serde_json::Value>(&dc.volume_mappings) {
                    if let Some(arr) = vols.as_array() {
                        for v in arr {
                            let host = v.get("host_path").and_then(|v| v.as_str()).unwrap_or("");
                            let cont = v.get("container_path").and_then(|v| v.as_str()).unwrap_or("");
                            let mode = v.get("mode").and_then(|v| v.as_str()).unwrap_or("rw");
                            if !host.is_empty() && !cont.is_empty() {
                                docker_run.push_str(&format!(" -v {}:{}:{}", host, cont, mode));
                            }
                        }
                    }
                }
            }
        }

        docker_run.push_str(&format!(" {} tail -f /dev/null", docker_image));

        match executor.execute(&docker_run).await {
            Ok(container_id_raw) => {
                let cid = container_id_raw.trim().to_string();
                append_provision_log(db, agent_id, &log(&format!("  Container ID: {}", &cid[..cid.len().min(12)]))).await;
                let _ = sqlx::query!(
                    "UPDATE agents SET container_id = $1 WHERE id = $2",
                    cid,
                    agent_id
                )
                .execute(db)
                .await;
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Step 3 failed: {}", e));
            }
        }

        // Step 4: Run init_script
        append_provision_log(db, agent_id, &log("[Step 4] Run init_script")).await;
        if let Some(dc_id) = docker_config_id {
            if let Ok(dc) = sqlx::query!("SELECT init_script FROM docker_configs WHERE id = $1", dc_id)
                .fetch_one(db)
                .await
            {
                if !dc.init_script.is_empty() {
                    let cmd = format!(
                        "docker exec {} sh -c '{}'",
                        container_name,
                        dc.init_script.replace('\'', "'\\''")
                    );
                    match executor.execute(&cmd).await {
                        Ok(out) => append_provision_log(db, agent_id, &log(&format!("  {}", out.trim()))).await,
                        Err(e) => append_provision_log(db, agent_id, &log(&format!("  [warn] init_script failed: {}", e))).await,
                    }
                } else {
                    append_provision_log(db, agent_id, &log("  (no init_script)")).await;
                }
            }
        } else {
            append_provision_log(db, agent_id, &log("  (no docker config)")).await;
        }

        // Step 5: Install CLI (codex only)
        append_provision_log(db, agent_id, &log("[Step 5] Install CLI")).await;
        if cli_type == "codex" {
            let check_npm = format!(
                "docker exec {} sh -c 'which npm || (apt-get update -qq && apt-get install -y nodejs npm -qq)'",
                container_name
            );
            match executor.execute(&check_npm).await {
                Ok(_) => {
                    let install_codex = format!(
                        "docker exec {} npm i -g @openai/codex",
                        container_name
                    );
                    match executor.execute(&install_codex).await {
                        Ok(out) => append_provision_log(db, agent_id, &log(&format!("  {}", out.trim()))).await,
                        Err(e) => {
                            return Err(anyhow::anyhow!("Step 5 failed (npm install): {}", e));
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Step 5 failed (npm check): {}", e));
                }
            }
        } else {
            append_provision_log(db, agent_id, &log(&format!("  (cli_type={}, skip)", cli_type))).await;
        }

        // Step 6: Install git
        append_provision_log(db, agent_id, &log("[Step 6] Install git")).await;
        let check_git = format!(
            "docker exec {} sh -c 'which git || apt-get install -y git -qq'",
            container_name
        );
        match executor.execute(&check_git).await {
            Ok(out) => append_provision_log(db, agent_id, &log(&format!("  {}", out.trim()))).await,
            Err(e) => append_provision_log(db, agent_id, &log(&format!("  [warn] git install failed: {}", e))).await,
        }

        // Step 7: Git clone (docker)
        append_provision_log(db, agent_id, &log("[Step 7] Git clone")).await;
        if !git_repo.is_empty() {
            let clone_cmd = format!(
                "docker exec {} sh -c 'git clone {} /workspace && cd /workspace && git checkout {}'",
                container_name,
                git_repo,
                git_branch
            );
            match executor.execute(&clone_cmd).await {
                Ok(out) => append_provision_log(db, agent_id, &log(&format!("  {}", out.trim()))).await,
                Err(e) => {
                    return Err(anyhow::anyhow!("Step 7 failed: {}", e));
                }
            }
        } else {
            append_provision_log(db, agent_id, &log("  (no git repo, skip)")).await;
        }
    } else {
        // No-docker mode: skip steps 3-6, only clone if git repo set
        append_provision_log(db, agent_id, &log("[Step 3] Start Docker container — skipped (no-docker mode)")).await;
        append_provision_log(db, agent_id, &log("[Step 4] Run init_script — skipped (no-docker mode)")).await;
        append_provision_log(db, agent_id, &log("[Step 5] Install CLI — skipped (no-docker mode)")).await;
        append_provision_log(db, agent_id, &log("[Step 6] Install git — skipped (no-docker mode)")).await;

        // Step 7: Git clone directly on host
        append_provision_log(db, agent_id, &log("[Step 7] Git clone")).await;
        if !git_repo.is_empty() {
            let workdir = format!("$HOME/.codex-fleet/{}/workspace", agent_id);
            let clone_cmd = format!(
                "git clone {} {} && cd {} && git checkout {}",
                git_repo, workdir, workdir, git_branch
            );
            match executor.execute(&clone_cmd).await {
                Ok(out) => append_provision_log(db, agent_id, &log(&format!("  {}", out.trim()))).await,
                Err(e) => {
                    return Err(anyhow::anyhow!("Step 7 failed: {}", e));
                }
            }
        } else {
            append_provision_log(db, agent_id, &log("  (no git repo, skip)")).await;
        }
    }

    // Done
    append_provision_log(db, agent_id, &log("[Done] Provisioning complete")).await;
    let _ = sqlx::query!(
        "UPDATE agents SET status = 'stopped' WHERE id = $1",
        agent_id
    )
    .execute(db)
    .await;

    Ok(())
}

pub async fn update_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> std::result::Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let existing = sqlx::query!(
        r#"SELECT id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, container_id,
           tmux_session, workdir, use_docker, status, provision_log, created_at
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
            let password = ssh_password_enc.as_deref().and_then(|p| crypto.decrypt(p).ok());
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
                            crypto.decrypt(row.auth_json.trim_start_matches("enc:")).unwrap_or(row.auth_json.clone())
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
                    if let Ok(row) = sqlx::query!(
                        "SELECT content FROM company_configs WHERE id = $1",
                        md_id
                    )
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
                append_provision_log(&db, &agent_id, "\n[Re-clone] Clearing workspace...\n").await;
                if use_docker && !container_name.is_empty() {
                    let clear_cmd = format!("docker exec {} sh -c 'rm -rf /workspace/*'", container_name);
                    let _ = executor.execute(&clear_cmd).await;
                    let clone_cmd = format!(
                        "docker exec {} sh -c 'git clone {} /workspace && cd /workspace && git checkout {}'",
                        container_name, new_git_repo2, git_branch2
                    );
                    match executor.execute(&clone_cmd).await {
                        Ok(out) => append_provision_log(&db, &agent_id, &format!("[Re-clone] Done: {}\n", out.trim())).await,
                        Err(e) => append_provision_log(&db, &agent_id, &format!("[Re-clone] Failed: {}\n", e)).await,
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
                        Ok(out) => append_provision_log(&db, &agent_id, &format!("[Re-clone] Done: {}\n", out.trim())).await,
                        Err(e) => append_provision_log(&db, &agent_id, &format!("[Re-clone] Failed: {}\n", e)).await,
                    }
                }
            }
        });
    }

    let updated = sqlx::query!(
        r#"SELECT id, name, server_id, git_repo, git_branch, git_auth_type, git_username,
           cli_type, codex_config_id, agents_md_id, docker_config_id,
           docker_image, docker_container_name, container_id,
           tmux_session, workdir, use_docker, status, provision_log, created_at
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
        created_at: updated.created_at.to_string(),
    };

    Ok((StatusCode::OK, Json(serde_json::to_value(agent).unwrap())))
}

pub async fn delete_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (executor, agent_info) = get_executor(&state, &id).await?;
    let use_docker = agent_info.use_docker;

    if use_docker {
        let container = agent_info.docker_container_name.unwrap_or_default();
        if !container.is_empty() {
            let _ = executor
                .execute(&format!("docker stop {} 2>/dev/null; docker rm {} 2>/dev/null", container, container))
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
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (executor, agent_info) = get_executor(&state, &id).await?;
    let tmux_session = agent_info.tmux_session;

    if agent_info.use_docker {
        let container_name = agent_info.docker_container_name.unwrap_or_default();
        let _ = executor.execute(&format!("docker start {}", container_name)).await;
        let tmux_cmd = format!(
            "docker exec {} tmux new-session -d -s {} 2>/dev/null || true",
            container_name, tmux_session
        );
        executor
            .execute(&tmux_cmd)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    } else {
        let workdir = agent_info.workdir;
        let tmux_cmd = format!(
            "tmux new-session -d -s {} -c {} 2>/dev/null || true",
            tmux_session, workdir
        );
        executor
            .execute(&tmux_cmd)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    sqlx::query!("UPDATE agents SET status = 'running' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({"message": "Agent started", "status": "running"})))
}

pub async fn stop_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (executor, agent_info) = get_executor(&state, &id).await?;

    if agent_info.use_docker {
        let container_name = agent_info.docker_container_name.unwrap_or_default();
        executor
            .execute(&format!("docker stop {}", container_name))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    } else {
        let tmux_session = agent_info.tmux_session;
        let _ = executor
            .execute(&format!("tmux kill-session -t {} 2>/dev/null; true", tmux_session))
            .await;
    }

    sqlx::query!("UPDATE agents SET status = 'stopped' WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({"message": "Agent stopped", "status": "stopped"})))
}

pub async fn resume_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (executor, agent_info) = get_executor(&state, &id).await?;
    let tmux_session = agent_info.tmux_session;

    let resume_cmd = match agent_info.cli_type.as_str() {
        "claude" | "claude_code" => "claude --resume",
        "codex" => "codex --resume",
        _ => return Err(AppError::BadRequest("CLI type not supported for resume".into())),
    };

    let cmd = if agent_info.use_docker {
        let container_name = agent_info.docker_container_name.unwrap_or_default();
        format!(
            "docker exec {} tmux send-keys -t {} '{}' Enter",
            container_name, tmux_session, resume_cmd
        )
    } else {
        format!(
            "tmux send-keys -t {} '{}' Enter",
            tmux_session, resume_cmd
        )
    };

    executor
        .execute(&cmd)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({"message": "Resume command sent"})))
}

#[derive(Serialize)]
pub struct TerminalCommandResponse {
    pub local_cmd: String,
    pub ssh_cmd: Option<String>,
}

pub async fn terminal_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TerminalCommandResponse>> {
    let agent = sqlx::query!(
        "SELECT server_id, docker_container_name, tmux_session, use_docker FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", id)))?;

    let session = agent.tmux_session;
    let use_docker = agent.use_docker;

    let (local_cmd, ssh_attach_cmd) = if use_docker {
        let container = agent.docker_container_name.unwrap_or_default();
        (
            format!("docker exec -it {} tmux attach -t {}", container, session),
            format!("docker exec -it {} tmux attach -t {}", container, session),
        )
    } else {
        (
            format!("tmux attach -t {}", session),
            format!("tmux attach -t {}", session),
        )
    };

    let ssh_cmd = if let Ok(server) = sqlx::query!(
        "SELECT ip, port, username FROM servers WHERE id = $1",
        agent.server_id
    )
    .fetch_one(&state.db)
    .await
    {
        Some(format!(
            "ssh {}@{} -p {} -t \"{}\"",
            server.username, server.ip, server.port, ssh_attach_cmd
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
}

/// Get an Executor (SSH) and agent row info for an agent.
pub async fn get_executor(
    state: &AppState,
    agent_id: &str,
) -> Result<(Executor, AgentRow)> {
    let agent = sqlx::query!(
        "SELECT id, server_id, tmux_session, docker_container_name, cli_type, workdir, use_docker FROM agents WHERE id = $1",
        agent_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Agent {} not found", agent_id)))?;

    let agent_row = AgentRow {
        tmux_session: agent.tmux_session,
        docker_container_name: agent.docker_container_name,
        cli_type: agent.cli_type,
        workdir: agent.workdir,
        use_docker: agent.use_docker,
    };

    let server = sqlx::query!(
        "SELECT ip, port, username, auth_type, password_encrypted, ssh_key_content FROM servers WHERE id = $1",
        agent.server_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Server {} not found", agent.server_id)))?;

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
