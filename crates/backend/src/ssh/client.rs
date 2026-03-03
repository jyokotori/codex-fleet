use async_ssh2_tokio::client::{AuthMethod, Client, ServerCheckMethod};
use std::path::PathBuf;
use tracing::debug;

pub struct SshClient {
    client: Client,
}

impl SshClient {
    pub async fn execute(&self, cmd: &str) -> anyhow::Result<String> {
        debug!("SSH execute: {}", cmd);
        let output = self.client.execute(cmd).await?;
        if output.exit_status != 0 && !output.stderr.is_empty() {
            tracing::warn!("SSH command stderr: {}", output.stderr);
        }
        Ok(output.stdout)
    }
}

pub struct SshClientPool;

impl SshClientPool {
    /// Connect using key-file (passwordless) auth. Uses the backend's own SSH key.
    pub async fn connect_passwordless(
        ip: &str,
        port: u16,
        username: &str,
        key_path: &std::path::Path,
    ) -> anyhow::Result<SshClient> {
        let auth = AuthMethod::with_key_file(key_path.to_str().unwrap(), None::<&str>);
        let client = Client::connect((ip, port), username, auth, ServerCheckMethod::NoCheck).await?;
        Ok(SshClient { client })
    }

    /// Connect using password auth (only for initial key installation).
    pub async fn connect_with_password(
        ip: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> anyhow::Result<SshClient> {
        let auth = AuthMethod::with_password(password);
        let client = Client::connect((ip, port), username, auth, ServerCheckMethod::NoCheck).await?;
        Ok(SshClient { client })
    }

    /// General-purpose connect used by agent/ws code. Passwordless uses the backend's key.
    pub async fn connect(
        ip: &str,
        port: u16,
        username: &str,
        auth_type: &str,
        password: Option<&str>,
        ssh_key: Option<&str>,
    ) -> anyhow::Result<SshClient> {
        match auth_type {
            "passwordless" => {
                let key_path = ensure_ssh_key().await?;
                Self::connect_passwordless(ip, port, username, &key_path).await
            }
            "password" => {
                let pw = password.ok_or_else(|| anyhow::anyhow!("Password required"))?;
                Self::connect_with_password(ip, port, username, pw).await
            }
            "key" => {
                let key = ssh_key.ok_or_else(|| anyhow::anyhow!("SSH key required"))?;
                let auth = AuthMethod::with_key(key, None::<&str>);
                let client = Client::connect((ip, port), username, auth, ServerCheckMethod::NoCheck).await?;
                Ok(SshClient { client })
            }
            _ => Err(anyhow::anyhow!("Unknown auth_type: {}", auth_type)),
        }
    }

    pub async fn test_connection(
        ip: &str,
        port: u16,
        username: &str,
        auth_type: &str,
        password: Option<&str>,
        ssh_key: Option<&str>,
    ) -> anyhow::Result<String> {
        let client = Self::connect(ip, port, username, auth_type, password, ssh_key).await?;
        let output = client.execute("echo 'connection-ok' && uname -a").await?;
        Ok(output)
    }
}

/// Find (or generate) the codex-fleet dedicated SSH key.
/// Uses RSA (PEM format) because libssh2 (used by async-ssh2-tokio) has broad
/// RSA support across all versions, avoiding OpenSSH-format ed25519 compatibility issues.
pub async fn ensure_ssh_key() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let ssh_dir = PathBuf::from(&home).join(".ssh");
    let key_path = ssh_dir.join("codex_fleet_rsa");

    if key_path.exists() {
        return Ok(key_path);
    }

    // Key doesn't exist yet — generate a new unencrypted RSA 2048 key.
    // RSA PEM format is universally supported by libssh2; ed25519 OpenSSH format is not.
    std::fs::create_dir_all(&ssh_dir)?;

    let output = tokio::process::Command::new("ssh-keygen")
        .args(["-t", "rsa", "-b", "2048", "-N", "", "-q", "-f"])
        .arg(&key_path)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("ssh-keygen not found: {}", e))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to generate SSH key: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(key_path)
}

/// Read the public key content for a given private key path.
pub fn read_public_key(private_key_path: &PathBuf) -> anyhow::Result<String> {
    let pub_path = format!("{}.pub", private_key_path.display());
    let content = std::fs::read_to_string(&pub_path)?;
    Ok(content.trim().to_string())
}
