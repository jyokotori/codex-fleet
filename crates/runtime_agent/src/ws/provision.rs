use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::Response,
};

use shared_kernel::{AppContext, AppError};

pub async fn ws_provision_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppContext>,
    Path(agent_id): Path<String>,
) -> std::result::Result<Response, AppError> {
    Ok(ws.on_upgrade(move |socket| handle_provision_socket(socket, state, agent_id)))
}

async fn handle_provision_socket(mut socket: WebSocket, state: AppContext, agent_id: String) {
    // 1. Subscribe to broadcast channel FIRST (before DB query to avoid race condition
    //    where events emitted between the DB read and subscribe would be missed).
    let rx_opt = {
        let ch = state.provision_channels.lock().await;
        ch.get(&agent_id).map(|tx| tx.subscribe())
    };

    // 2. Query current DB state
    let row = sqlx::query!(
        "SELECT provision_log, provision_steps, status FROM agents WHERE id = $1",
        agent_id
    )
    .fetch_optional(&state.db)
    .await;

    let (log, steps_json, status): (String, serde_json::Value, String) = match row {
        Ok(Some(r)) => (r.provision_log, r.provision_steps, r.status),
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

    // 3. Send provision_init with current step states so client can restore UI without replay
    let init_msg = serde_json::json!({
        "t": "provision_init",
        "steps": steps_json,
        "ts": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    });
    if socket
        .send(Message::Text(init_msg.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    // 4. Replay historical JSONL log lines (terminal output only, step states come from provision_init)
    for line in log.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if socket
            .send(Message::Text(trimmed.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
    }

    // 5. If provisioning is already done, nothing more to stream
    if status != "provisioning" {
        return;
    }

    // 6. Forward live broadcast events
    let mut rx = match rx_opt {
        Some(r) => r,
        None => {
            // Provisioning finished between subscribe and DB query (or channel already removed);
            // re-read tail from DB and replay any missed events.
            let row2 = sqlx::query!(
                "SELECT provision_log, status FROM agents WHERE id = $1",
                agent_id
            )
            .fetch_optional(&state.db)
            .await;
            if let Ok(Some(r2)) = row2 {
                let already_sent = log.len();
                let tail = &r2.provision_log[already_sent.min(r2.provision_log.len())..];
                for line in tail.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let _ = socket.send(Message::Text(trimmed.to_string().into())).await;
                }
            }
            return;
        }
    };

    // Stream live events until provision_done or client disconnect
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        let trimmed = msg.trim().to_string();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if socket.send(Message::Text(trimmed.clone().into())).await.is_err() {
                            break;
                        }
                        // Stop streaming after provision_done
                        if let Ok(ev) = serde_json::from_str::<serde_json::Value>(&trimmed) {
                            if ev.get("t").and_then(|v| v.as_str()) == Some("provision_done") {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let _ = socket.send(Message::Text(
                            r#"{"t":"warn","step":0,"text":"[WS] Reconnecting: lagged behind","ts":0}"#.to_string().into()
                        )).await;
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
