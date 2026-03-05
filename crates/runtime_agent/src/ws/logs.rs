use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::Response,
};
use tokio::time::{interval, Duration};

use crate::api::agents::get_executor;
use shared_kernel::{AppContext, AppError};

pub async fn ws_logs_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
) -> std::result::Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_logs_socket(socket, state, agent_id)))
}

async fn handle_logs_socket(mut socket: WebSocket, state: AppContext, agent_id: String) {
    let (executor, agent_info) = match get_executor(&state, &agent_id).await {
        Ok(a) => a,
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error: {}", e).into()))
                .await;
            return;
        }
    };
    let container_name = agent_info.docker_container_name.unwrap_or_default();
    let tmux_session = agent_info.tmux_session;
    let use_docker = agent_info.use_docker;

    let mut ticker = interval(Duration::from_millis(500));
    let mut last_content = String::new();

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let cmd = if use_docker {
                    format!(
                        "docker exec {} tmux capture-pane -p -J -e -t {} 2>/dev/null || echo ''",
                        container_name, tmux_session
                    )
                } else {
                    format!(
                        "tmux capture-pane -p -J -e -t {} 2>/dev/null || echo ''",
                        tmux_session
                    )
                };

                match executor.execute(&cmd).await {
                    Ok(content) => {
                        if content != last_content {
                            // Send only the diff (new lines)
                            let new_content = if content.starts_with(&last_content) {
                                &content[last_content.len()..]
                            } else {
                                &content
                            };

                            if !new_content.is_empty() {
                                if socket.send(Message::Text(new_content.to_string().into())).await.is_err() {
                                    break;
                                }
                            }
                            last_content = content;
                        }
                    }
                    Err(e) => {
                        let _ = socket.send(Message::Text(format!("Log error: {}", e).into())).await;
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
