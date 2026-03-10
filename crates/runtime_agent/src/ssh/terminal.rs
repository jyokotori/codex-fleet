use std::sync::Arc;

use russh::client::{self, Handle, Msg};
use russh::keys::key;
use russh::Channel;

pub struct ClientHandler;

#[async_trait::async_trait]
impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

pub async fn connect_russh(
    ip: &str,
    port: u16,
    username: &str,
    auth_type: &str,
    password: Option<&str>,
    ssh_key_content: Option<&str>,
) -> anyhow::Result<Handle<ClientHandler>> {
    let config = client::Config {
        ..Default::default()
    };

    let mut handle = client::connect(Arc::new(config), (ip, port), ClientHandler).await?;

    match auth_type {
        "password" => {
            let pw = password.ok_or_else(|| anyhow::anyhow!("Password required"))?;
            let auth_ok = handle.authenticate_password(username, pw).await?;
            if !auth_ok {
                return Err(anyhow::anyhow!("Password authentication failed"));
            }
        }
        "key" => {
            let key_str = ssh_key_content.ok_or_else(|| anyhow::anyhow!("SSH key required"))?;
            let key_pair = russh_keys::decode_secret_key(key_str, None)?;
            let auth_ok = handle
                .authenticate_publickey(username, Arc::new(key_pair))
                .await?;
            if !auth_ok {
                return Err(anyhow::anyhow!("Key authentication failed"));
            }
        }
        "passwordless" => {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            let key_path = std::path::PathBuf::from(&home).join(".ssh/codex_fleet_rsa");
            let key_pair = russh_keys::load_secret_key(&key_path, None)?;
            let auth_ok = handle
                .authenticate_publickey(username, Arc::new(key_pair))
                .await?;
            if !auth_ok {
                return Err(anyhow::anyhow!("Passwordless authentication failed"));
            }
        }
        _ => return Err(anyhow::anyhow!("Unknown auth_type: {}", auth_type)),
    }

    Ok(handle)
}

/// Open an interactive PTY channel. Returns the channel for read/write and keeps the handle alive.
pub async fn open_pty_channel(
    ip: &str,
    port: u16,
    username: &str,
    auth_type: &str,
    password: Option<&str>,
    ssh_key_content: Option<&str>,
    cols: u32,
    rows: u32,
    command: &str,
) -> anyhow::Result<(Channel<Msg>, Handle<ClientHandler>)> {
    let handle = connect_russh(ip, port, username, auth_type, password, ssh_key_content).await?;
    let channel = handle.channel_open_session().await?;

    channel
        .request_pty(false, "xterm-256color", cols, rows, 0, 0, &[])
        .await?;
    channel.exec(false, command).await?;

    Ok((channel, handle))
}

/// Open a non-interactive exec channel for streaming command output.
pub async fn open_exec_channel(
    ip: &str,
    port: u16,
    username: &str,
    auth_type: &str,
    password: Option<&str>,
    ssh_key_content: Option<&str>,
    command: &str,
) -> anyhow::Result<(Channel<Msg>, Handle<ClientHandler>)> {
    let handle = connect_russh(ip, port, username, auth_type, password, ssh_key_content).await?;
    let channel = handle.channel_open_session().await?;

    channel.exec(true, command).await?;

    Ok((channel, handle))
}
