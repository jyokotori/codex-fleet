use sqlx::PgPool;
use tracing;

/// Send notifications for a task status change.
/// Looks up the given notification config IDs, checks if each is enabled
/// and subscribed to the event, then POSTs the payload to the webhook URL.
pub async fn send_task_notification(
    db: &PgPool,
    notification_ids: &[String],
    new_status: &str,
    payload: serde_json::Value,
) {
    if notification_ids.is_empty() {
        return;
    }

    let configs = match sqlx::query!(
        "SELECT id, config_json, events_json FROM notification_configs WHERE id = ANY($1) AND enabled = true",
        notification_ids
    )
    .fetch_all(db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("Failed to query notification_configs: {e}");
            return;
        }
    };

    let client = reqwest::Client::new();

    for config in configs {
        let events: Vec<String> =
            serde_json::from_str(&config.events_json).unwrap_or_default();
        if !events.iter().any(|e| e == new_status) {
            continue;
        }

        let config_data: serde_json::Value = match serde_json::from_str(&config.config_json) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let url = match config_data.get("url").and_then(|u| u.as_str()) {
            Some(u) => u.to_string(),
            None => continue,
        };

        let mut builder = client.post(&url).json(&payload);

        if let Some(headers) = config_data.get("headers").and_then(|h| h.as_object()) {
            for (k, v) in headers {
                if let Some(v_str) = v.as_str() {
                    builder = builder.header(k.as_str(), v_str);
                }
            }
        }

        if let Err(e) = builder.send().await {
            tracing::warn!("Webhook notification to {} failed: {e}", url);
        } else {
            tracing::info!("Webhook notification sent to {} for event {}", url, new_status);
        }
    }
}
