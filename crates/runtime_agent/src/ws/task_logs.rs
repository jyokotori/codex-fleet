use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::Response,
};

use shared_kernel::{AppContext, AppError};

pub async fn ws_task_logs_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Path(task_id): Path<String>,
) -> std::result::Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_task_logs_socket(socket, state, task_id)))
}

async fn handle_task_logs_socket(mut socket: WebSocket, state: AppContext, task_id: String) {
    // 1. Subscribe to broadcast channel first (before DB query to avoid race)
    let rx_opt = {
        let ch = state.task_channels.lock().await;
        ch.get(&task_id).map(|tx| tx.subscribe())
    };

    // 2. Query current DB state
    let row = sqlx::query!(
        "SELECT task_log, status FROM tasks WHERE id = $1",
        task_id
    )
    .fetch_optional(&state.db)
    .await;

    let (log, status) = match row {
        Ok(Some(r)) => (r.task_log, r.status),
        Ok(None) => {
            let _ = socket
                .send(Message::Text("Error: task not found".to_string().into()))
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

    // 3. Replay existing log
    if !log.is_empty() {
        if socket.send(Message::Text(log.clone().into())).await.is_err() {
            return;
        }
    }

    // 4. If task is already done, nothing more to stream
    if status != "agent_in_progress" {
        let done_msg = serde_json::json!({
            "type": "task_done",
            "status": status,
        })
        .to_string();
        let _ = socket.send(Message::Text(done_msg.into())).await;
        return;
    }

    // 5. Forward live broadcast events
    let mut rx = match rx_opt {
        Some(r) => r,
        None => {
            // Task finished between subscribe and DB query; re-read from DB
            let row2 = sqlx::query!(
                "SELECT task_log, status FROM tasks WHERE id = $1",
                task_id
            )
            .fetch_optional(&state.db)
            .await;
            if let Ok(Some(r2)) = row2 {
                let already_sent = log.len();
                let tail = &r2.task_log[already_sent.min(r2.task_log.len())..];
                if !tail.is_empty() {
                    let _ = socket.send(Message::Text(tail.to_string().into())).await;
                }
                let done_msg = serde_json::json!({
                    "type": "task_done",
                    "status": r2.status,
                })
                .to_string();
                let _ = socket.send(Message::Text(done_msg.into())).await;
            }
            return;
        }
    };

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Task completed — send final status
                        if let Ok(Some(r)) = sqlx::query!(
                            "SELECT status FROM tasks WHERE id = $1",
                            task_id
                        )
                        .fetch_optional(&state.db)
                        .await
                        {
                            let done_msg = serde_json::json!({
                                "type": "task_done",
                                "status": r.status,
                            })
                            .to_string();
                            let _ = socket.send(Message::Text(done_msg.into())).await;
                        }
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Replay from DB on lag
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
