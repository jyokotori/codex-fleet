use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::Response,
};
use tokio::time::{interval, Duration};

use shared_kernel::{AppContext, AppError};

pub async fn ws_provision_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
) -> std::result::Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_provision_socket(socket, state, agent_id)))
}

async fn handle_provision_socket(mut socket: WebSocket, state: AppContext, agent_id: String) {
    // Send full historical log first, then poll for incremental updates
    let initial = sqlx::query!(
        "SELECT provision_log, status FROM agents WHERE id = $1",
        agent_id
    )
    .fetch_optional(&state.db)
    .await;

    let (mut last_len, initial_status) = match initial {
        Ok(Some(row)) => {
            let log = row.provision_log.clone();
            let status = row.status.clone();
            if !log.is_empty() {
                if socket
                    .send(Message::Text(log.clone().into()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            // If already done, send terminal frame immediately
            if status != "provisioning" {
                let done_msg = serde_json::json!({
                    "done": true,
                    "status": status,
                })
                .to_string();
                let _ = socket.send(Message::Text(done_msg.into())).await;
                return;
            }
            (log.len(), status)
        }
        Ok(None) => {
            let _ = socket
                .send(Message::Text("Error: agent not found".to_string().into()))
                .await;
            return;
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error: {}", e).into()))
                .await;
            return;
        }
    };

    let _ = initial_status;

    let mut ticker = interval(Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let row = sqlx::query!(
                    "SELECT provision_log, status FROM agents WHERE id = $1",
                    agent_id
                )
                .fetch_optional(&state.db)
                .await;

                match row {
                    Ok(Some(r)) => {
                        let log = r.provision_log;
                        let status = r.status;

                        // Send incremental diff
                        if log.len() > last_len {
                            let new_part = &log[last_len..];
                            if socket.send(Message::Text(new_part.to_string().into())).await.is_err() {
                                break;
                            }
                            last_len = log.len();
                        }

                        // Check if done
                        if status != "provisioning" {
                            let done_msg = serde_json::json!({
                                "done": true,
                                "status": status,
                            })
                            .to_string();
                            let _ = socket.send(Message::Text(done_msg.into())).await;
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = socket.send(Message::Text("Error: agent not found".to_string().into())).await;
                        break;
                    }
                    Err(e) => {
                        let _ = socket.send(Message::Text(format!("Error: {}", e).into())).await;
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
