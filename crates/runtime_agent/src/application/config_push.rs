//! Push CLI configs (`codex` config files, `claude` env) and AGENTS.md from
//! the database to the remote agent runtime (docker container or non-docker
//! host).
//!
//! This is the orchestration layer for "make a config change take effect on
//! a live agent without re-provisioning". It is invoked from:
//! - `update_agent` (when `cli_inits` are changed)
//! - the propagate endpoint exposed for editing a Codex config and pushing
//!   the new content to every agent that references it
//!
//! For Codex: writes `config.toml` / `auth.json` into the agent runtime's
//! config dir. For Claude: writes `claude.env` next to the codex files and
//! splices a `source` line into the remote `~/.bashrc` (non-docker only).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Serialize;
use shared_kernel::AppContext;

use crate::api::agents::{bashrc_splice_command, render_claude_env_body};
use crate::infrastructure::agent_runtime::{
    agent_base_dir_from_workdir, get_executor, target_shell_command,
};
use crate::infrastructure::crypto::Crypto;
use config_center::application::{claude_configs, codex_configs, company_configs};

#[derive(Serialize, Default)]
pub struct PushReport {
    pub pushed: Vec<String>,
    pub failed: Vec<PushFailure>,
}

#[derive(Serialize)]
pub struct PushFailure {
    pub agent_id: String,
    pub error: String,
}

/// Push every supported per-agent config (Codex files, AGENTS.md, Claude env)
/// referenced by the agent's current `cli_inits` to the remote target.
/// No-op for entries that have no associated content.
pub async fn push_for_agent(state: &AppContext, agent_id: &str) -> anyhow::Result<()> {
    let codex_init = sqlx::query!(
        r#"SELECT codex_config_id, agents_md_id
           FROM agent_cli_inits
           WHERE agent_id = $1 AND cli_type = 'codex'"#,
        agent_id
    )
    .fetch_optional(&state.db)
    .await?;
    let claude_init = sqlx::query!(
        r#"SELECT claude_config_id
           FROM agent_cli_inits
           WHERE agent_id = $1 AND cli_type = 'claude_code'"#,
        agent_id
    )
    .fetch_optional(&state.db)
    .await?;

    let codex_config_id = codex_init.as_ref().and_then(|r| r.codex_config_id.as_deref());
    let agents_md_id = codex_init.as_ref().and_then(|r| r.agents_md_id.as_deref());
    let claude_config_id = claude_init.as_ref().and_then(|r| r.claude_config_id.as_deref());

    if codex_config_id.is_none() && agents_md_id.is_none() && claude_config_id.is_none() {
        return Ok(());
    }

    let (executor, agent_row) = get_executor(state, agent_id)
        .await
        .map_err(|e| anyhow::anyhow!("connect agent: {}", e))?;
    let use_docker = agent_row.use_docker;
    let container = agent_row.docker_container_name.unwrap_or_default();
    if use_docker && container.is_empty() {
        anyhow::bail!("docker agent has no container_name");
    }

    // Host-side base dir (e.g. /home/user/.codex-fleet/{id}). Needed for both
    // the codex non-docker path and for splicing ~/.bashrc on the host.
    let host_base = agent_base_dir_from_workdir(&agent_row.workdir)
        .unwrap_or_else(|| format!("$HOME/.codex-fleet/{}", agent_id));

    // Codex config dir: /root/.codex inside the container (symlinked to /agent
    // during provisioning) for docker agents, otherwise the host-side agent dir.
    let codex_target_dir = if use_docker {
        "/root/.codex".to_string()
    } else {
        format!("{}/agent", host_base)
    };

    let mkdir = format!("mkdir -p {}", codex_target_dir);
    executor
        .execute(&target_shell_command(use_docker, &container, &mkdir))
        .await
        .map_err(|e| anyhow::anyhow!("mkdir {}: {}", codex_target_dir, e))?;

    let crypto = Crypto::new(&state.config.master_key);

    if let Some(cid) = codex_config_id {
        if let Some(content) = codex_configs::get_codex_config_content(&state.db, cid).await? {
            if !content.config_toml.is_empty() {
                let b64 = BASE64.encode(content.config_toml.as_bytes());
                let cmd = format!("echo {} | base64 -d > {}/config.toml", b64, codex_target_dir);
                executor
                    .execute(&target_shell_command(use_docker, &container, &cmd))
                    .await
                    .map_err(|e| anyhow::anyhow!("write config.toml: {}", e))?;
            }
            let auth = if content.auth_json.starts_with("enc:") {
                crypto
                    .decrypt(content.auth_json.trim_start_matches("enc:"))
                    .unwrap_or_else(|_| content.auth_json.clone())
            } else {
                content.auth_json.clone()
            };
            if !auth.is_empty() {
                let b64 = BASE64.encode(auth.as_bytes());
                let cmd = format!("echo {} | base64 -d > {}/auth.json", b64, codex_target_dir);
                executor
                    .execute(&target_shell_command(use_docker, &container, &cmd))
                    .await
                    .map_err(|e| anyhow::anyhow!("write auth.json: {}", e))?;
            }
        }
    }

    if let Some(mid) = agents_md_id {
        if let Some(content) = company_configs::get_company_config_content(&state.db, mid).await? {
            if !content.is_empty() {
                let b64 = BASE64.encode(content.as_bytes());
                let cmd = format!("echo {} | base64 -d > {}/AGENTS.md", b64, codex_target_dir);
                executor
                    .execute(&target_shell_command(use_docker, &container, &cmd))
                    .await
                    .map_err(|e| anyhow::anyhow!("write AGENTS.md: {}", e))?;
            }
        }
    }

    if let Some(cid) = claude_config_id {
        if let Some(content) = claude_configs::get_claude_config_content(&state.db, cid).await? {
            let env_body = render_claude_env_body(&content);
            let b64 = BASE64.encode(env_body.as_bytes());
            // Path used by `build_cli_command` to source env at task time.
            let env_path = if use_docker {
                "/agent/claude.env".to_string()
            } else {
                format!("{}/agent/claude.env", host_base)
            };
            // Ensure the directory exists (codex_target_dir already covers it
            // for docker; non-docker uses the same /agent subpath).
            let cmd = format!("echo {} | base64 -d > {}", b64, env_path);
            executor
                .execute(&target_shell_command(use_docker, &container, &cmd))
                .await
                .map_err(|e| anyhow::anyhow!("write claude.env: {}", e))?;

            // ~/.bashrc lives on the host shell; docker agents intentionally
            // skip this — the container has its own shell environment.
            if !use_docker {
                let cmd = bashrc_splice_command(agent_id, &host_base);
                executor
                    .execute(&cmd)
                    .await
                    .map_err(|e| anyhow::anyhow!("splice ~/.bashrc: {}", e))?;
            }
        }
    }

    Ok(())
}

/// Fan out [`push_for_agent`] to every agent whose `cli_inits` reference the
/// given `codex_config_id`. Per-agent failures are collected, not propagated.
pub async fn push_for_codex_config(state: &AppContext, codex_config_id: &str) -> PushReport {
    let agent_ids: Vec<String> = match sqlx::query_scalar!(
        "SELECT DISTINCT agent_id FROM agent_cli_inits WHERE codex_config_id = $1",
        codex_config_id
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "failed to query agents referencing codex_config");
            return PushReport::default();
        }
    };

    let mut report = PushReport::default();
    for aid in agent_ids {
        match push_for_agent(state, &aid).await {
            Ok(()) => report.pushed.push(aid),
            Err(e) => report.failed.push(PushFailure {
                agent_id: aid,
                error: e.to_string(),
            }),
        }
    }
    report
}
