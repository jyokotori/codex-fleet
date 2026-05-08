//! Transport-level helpers for talking to a remote agent runtime: the SSH
//! [`Executor`], the docker-vs-host command shim, and the helpers used to
//! resolve on-disk paths. These are infrastructure primitives — application
//! services depend on them, not the other way around.

use shared_kernel::{AppContext, AppError, Result};
use sqlx::Row;

use crate::infrastructure::crypto::Crypto;
use crate::ssh::client::{SshClient, SshClientPool};

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

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Shell environment preamble for non-docker SSH exec (loads nvm etc.)
pub const HOST_ENV_SETUP: &str =
    r#"export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"; [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"; "#;

/// Wrap `cmd` so it runs in the right execution context — inside the docker
/// container for docker agents, or directly on the host (with shared env
/// preamble) for non-docker agents.
pub fn target_shell_command(use_docker: bool, container_name: &str, cmd: &str) -> String {
    if use_docker {
        format!("docker exec {} sh -lc {}", container_name, shell_quote(cmd))
    } else {
        format!("{}{}", HOST_ENV_SETUP, cmd)
    }
}

/// `workdir` is the agent's absolute workspace path
/// (e.g. `/home/demo/.codex-fleet/{id}/workspace`); strip the trailing
/// `/workspace` to recover the agent base directory.
pub fn agent_base_dir_from_workdir(workdir: &str) -> Option<String> {
    workdir
        .strip_suffix("/workspace")
        .filter(|base| !base.is_empty())
        .map(ToString::to_string)
}

#[derive(Debug, Clone)]
pub struct AgentRow {
    pub docker_container_name: Option<String>,
    pub workdir: String,
    pub use_docker: bool,
    pub status: String,
}

/// Look up an agent + its server, decrypt the SSH password if any, and open
/// an SSH connection ready to dispatch commands against.
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
