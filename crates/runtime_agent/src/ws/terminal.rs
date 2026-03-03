use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    response::Response,
};
use serde::Deserialize;

use crate::api::agents::get_executor;
use shared_kernel::{AppContext, AppError};

#[derive(Deserialize, Default)]
pub struct TerminalQuery {
    pub window: Option<String>,
}

pub async fn ws_terminal_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
    Query(query): Query<TerminalQuery>,
) -> std::result::Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_terminal_socket(socket, state, agent_id, query.window)))
}

async fn handle_terminal_socket(
    mut socket: WebSocket,
    state: AppContext,
    agent_id: String,
    window: Option<String>,
) {
    let (executor, agent_info) = match get_executor(&state, &agent_id).await {
        Ok(a) => a,
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error: {}", e).into()))
                .await;
            return;
        }
    };
    let tmux_session = agent_info.tmux_session;
    let use_docker = agent_info.use_docker;

    // Build the tmux target: session or session:window
    let tmux_target = match &window {
        Some(w) if !w.is_empty() => format!("{}:{}", tmux_session, w),
        _ => tmux_session.clone(),
    };

    // Interactive terminal: relay input from WebSocket to tmux, output via capture-pane polling
    let mut last_output = String::new();
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let cmd = if use_docker {
                    let container_name = agent_info.docker_container_name.as_deref().unwrap_or("");
                    format!(
                        "docker exec {} tmux capture-pane -p -J -e -t {} 2>/dev/null || echo ''",
                        container_name, tmux_target
                    )
                } else {
                    format!(
                        "tmux capture-pane -p -J -e -t {} 2>/dev/null || echo ''",
                        tmux_target
                    )
                };
                if let Ok(output) = executor.execute(&cmd).await {
                    if output != last_output {
                        let new_part = if output.starts_with(&last_output) {
                            &output[last_output.len()..]
                        } else {
                            &output
                        };
                        if !new_part.is_empty() {
                            if socket.send(Message::Text(new_part.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        last_output = output;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let send_cmd = if use_docker {
                            let container_name = agent_info.docker_container_name.as_deref().unwrap_or("");
                            format!(
                                "docker exec {} tmux send-keys -t {} '{}'",
                                container_name, tmux_target,
                                text.replace('\'', "\\'")
                            )
                        } else {
                            format!(
                                "tmux send-keys -t {} '{}'",
                                tmux_target,
                                text.replace('\'', "\\'")
                            )
                        };
                        if executor.execute(&send_cmd).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        if let Ok(s) = String::from_utf8(data.to_vec()) {
                            let send_cmd = if use_docker {
                                let container_name = agent_info.docker_container_name.as_deref().unwrap_or("");
                                format!(
                                    "docker exec {} tmux send-keys -t {} '{}'",
                                    container_name, tmux_target,
                                    s.replace('\'', "\\'")
                                )
                            } else {
                                format!(
                                    "tmux send-keys -t {} '{}'",
                                    tmux_target,
                                    s.replace('\'', "\\'")
                                )
                            };
                            let _ = executor.execute(&send_cmd).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
