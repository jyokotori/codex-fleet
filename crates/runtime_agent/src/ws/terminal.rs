use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::Response,
};
use russh::ChannelMsg;
use serde::Deserialize;

use crate::api::agents::get_server_credentials;
use crate::ssh::terminal::open_pty_channel;
use shared_kernel::{AppContext, AppError};

#[derive(Deserialize)]
struct ResizeMsg {
    #[serde(rename = "type")]
    msg_type: String,
    cols: u32,
    rows: u32,
}

pub async fn ws_terminal_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
) -> std::result::Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_terminal_socket(socket, state, agent_id)))
}

fn error_json(msg: &str) -> String {
    serde_json::json!({"type":"error","message":msg}).to_string()
}

async fn handle_terminal_socket(mut socket: WebSocket, state: AppContext, agent_id: String) {
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

    let default_cols = 120u32;
    let default_rows = 30u32;

    let (mut channel, _handle) = match open_pty_channel(
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

    // Main loop: bidirectional WS <-> SSH
    // Both channel.wait() (&mut self) and channel.data()/window_change() (&self)
    // can't be called concurrently on the same channel. But tokio::select! ensures
    // only one branch runs at a time, so this is safe.
    loop {
        tokio::select! {
            // SSH output -> WS binary
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
            // WS input -> SSH
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
}
