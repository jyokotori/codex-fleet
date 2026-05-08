//! Push codex `config.toml` / `auth.json` / `AGENTS.md` from the database to
//! the remote agent runtime (docker container or non-docker host).
//!
//! This is the orchestration layer for "make a config change take effect on
//! a live agent without re-provisioning". It is invoked from:
//! - `update_agent` (when `cli_inits` are changed)
//! - the propagate endpoint exposed for editing a Codex config and pushing
//!   the new content to every agent that references it
//!
//! Only the `codex` cli_init entry is materialised — other CLIs don't have
//! per-agent config files yet (matches `provision_agent` behaviour).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Serialize;
use shared_kernel::AppContext;

use crate::infrastructure::agent_runtime::{
    agent_base_dir_from_workdir, get_executor, target_shell_command,
};
use crate::infrastructure::crypto::Crypto;
use config_center::application::{codex_configs, company_configs};

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

/// Push the codex config / AGENTS.md referenced by the agent's current
/// `cli_inits` to the remote target. No-op if the agent has no `codex` entry
/// or the entry has neither `codex_config_id` nor `agents_md_id`.
pub async fn push_for_agent(state: &AppContext, agent_id: &str) -> anyhow::Result<()> {
    let codex_init = sqlx::query!(
        r#"SELECT codex_config_id, agents_md_id
           FROM agent_cli_inits
           WHERE agent_id = $1 AND cli_type = 'codex'"#,
        agent_id
    )
    .fetch_optional(&state.db)
    .await?;
    let codex_init = match codex_init {
        Some(r) => r,
        None => return Ok(()),
    };
    let codex_config_id = codex_init.codex_config_id.as_deref();
    let agents_md_id = codex_init.agents_md_id.as_deref();
    if codex_config_id.is_none() && agents_md_id.is_none() {
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

    let target_dir = if use_docker {
        "/root/.codex".to_string()
    } else {
        agent_base_dir_from_workdir(&agent_row.workdir)
            .map(|b| format!("{}/agent", b))
            .unwrap_or_else(|| format!("$HOME/.codex-fleet/{}/agent", agent_id))
    };

    let mkdir = format!("mkdir -p {}", target_dir);
    executor
        .execute(&target_shell_command(use_docker, &container, &mkdir))
        .await
        .map_err(|e| anyhow::anyhow!("mkdir {}: {}", target_dir, e))?;

    let crypto = Crypto::new(&state.config.master_key);

    if let Some(cid) = codex_config_id {
        if let Some(content) = codex_configs::get_codex_config_content(&state.db, cid).await? {
            if !content.config_toml.is_empty() {
                let b64 = BASE64.encode(content.config_toml.as_bytes());
                let cmd = format!("echo {} | base64 -d > {}/config.toml", b64, target_dir);
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
                let cmd = format!("echo {} | base64 -d > {}/auth.json", b64, target_dir);
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
                let cmd = format!("echo {} | base64 -d > {}/AGENTS.md", b64, target_dir);
                executor
                    .execute(&target_shell_command(use_docker, &container, &cmd))
                    .await
                    .map_err(|e| anyhow::anyhow!("write AGENTS.md: {}", e))?;
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
