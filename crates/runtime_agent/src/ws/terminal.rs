use std::collections::HashSet;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    response::Response,
};
use russh::ChannelMsg;
use serde::Deserialize;

use crate::api::agents::get_server_credentials;
use crate::ssh::terminal::{open_pty_channel, ClientHandler};
use shared_kernel::{AppContext, AppError};

#[derive(Deserialize)]
struct ResizeMsg {
    #[serde(rename = "type")]
    msg_type: String,
    cols: u32,
    rows: u32,
}

#[derive(Deserialize, Default)]
pub struct TerminalQuery {
    pub resume_thread_id: Option<String>,
    #[allow(dead_code)]
    pub token: Option<String>,
}

pub async fn ws_terminal_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
    Query(query): Query<TerminalQuery>,
) -> std::result::Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| {
        handle_terminal_socket(socket, state, agent_id, query.resume_thread_id)
    }))
}

fn error_json(msg: &str) -> String {
    serde_json::json!({"type":"error","message":msg}).to_string()
}

/// Run a one-shot command on the SSH handle and return stdout.
async fn ssh_exec(handle: &russh::client::Handle<ClientHandler>, cmd: &str) -> String {
    let mut ch = match handle.channel_open_session().await {
        Ok(ch) => ch,
        Err(_) => return String::new(),
    };
    if ch.exec(true, cmd.as_bytes()).await.is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    loop {
        match ch.wait().await {
            Some(ChannelMsg::Data { data }) => buf.extend_from_slice(&data),
            Some(ChannelMsg::ExtendedData { data, .. }) => buf.extend_from_slice(&data),
            Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
            _ => {}
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

fn parse_pids(output: &str) -> HashSet<String> {
    output
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()))
        .collect()
}

async fn handle_terminal_socket(
    mut socket: WebSocket,
    state: AppContext,
    agent_id: String,
    resume_thread_id: Option<String>,
) {
    let (creds, agent_info) = match get_server_credentials(&state, &agent_id).await {
        Ok(a) => a,
        Err(e) => {
            let _ = socket
                .send(Message::Text(error_json(&e.to_string()).into()))
                .await;
            return;
        }
    };

    let command = if agent_info.use_docker {
        let container = agent_info.docker_container_name.as_deref().unwrap_or("");
        format!(
            "docker exec -it {} bash -c 'cd /workspace && exec bash'",
            container
        )
    } else {
        format!("bash -c 'cd {} && exec bash'", agent_info.workdir)
    };

    // Build pgrep command and kill prefix for resume process tracking
    let (pgrep_cmd, kill_prefix) = if let Some(ref tid) = resume_thread_id {
        let pattern = format!(
            "[c]odex resume {tid}|[c]laude resume {tid}|[g]emini resume {tid}|[o]pencode resume {tid}"
        );
        if agent_info.use_docker {
            let container = agent_info.docker_container_name.as_deref().unwrap_or("");
            (
                Some(format!(
                    "docker exec {} pgrep -f '{}' 2>/dev/null || true",
                    container, pattern
                )),
                Some(format!("docker exec {} kill", container)),
            )
        } else {
            (
                Some(format!("pgrep -f '{}' 2>/dev/null || true", pattern)),
                Some("kill".to_string()),
            )
        }
    } else {
        (None, None)
    };

    let default_cols = 120u32;
    let default_rows = 30u32;

    let (mut channel, handle) = match open_pty_channel(
        &creds.ip,
        creds.port,
        &creds.username,
        &creds.auth_type,
        creds.password.as_deref(),
        creds.ssh_key_content.as_deref(),
        default_cols,
        default_rows,
        &command,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    error_json(&format!("SSH connect failed: {}", e)).into(),
                ))
                .await;
            return;
        }
    };

    // Snapshot existing resume PIDs so we only kill processes WE start
    let pre_pids = if let Some(ref cmd) = pgrep_cmd {
        parse_pids(&ssh_exec(&handle, cmd).await)
    } else {
        HashSet::new()
    };

    // Main loop: bidirectional WS <-> SSH
    loop {
        tokio::select! {
            ssh_msg = channel.wait() => {
                match ssh_msg {
                    Some(ChannelMsg::Data { data }) => {
                        if socket.send(Message::Binary(data.to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        if socket.send(Message::Binary(data.to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                        let _ = socket
                            .send(Message::Text(error_json("SSH session ended").into()))
                            .await;
                        break;
                    }
                    _ => {}
                }
            }
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Binary(data))) => {
                        if channel.data(&data[..]).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(resize) = serde_json::from_str::<ResizeMsg>(&text) {
                            if resize.msg_type == "resize" {
                                let _ = channel.window_change(
                                    resize.cols, resize.rows, 0, 0
                                ).await;
                                continue;
                            }
                        }
                        if channel.data(text.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }

    // Cleanup: kill only resume processes started during THIS session
    if let (Some(ref pgrep), Some(ref kill_pfx)) = (&pgrep_cmd, &kill_prefix) {
        let post_pids = parse_pids(&ssh_exec(&handle, pgrep).await);
        let new_pids: Vec<_> = post_pids.difference(&pre_pids).collect();
        if !new_pids.is_empty() {
            let pids_str = new_pids
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let cmd = format!("{} {} 2>/dev/null ; true", kill_pfx, pids_str);
            ssh_exec(&handle, &cmd).await;
        }
    }
}
