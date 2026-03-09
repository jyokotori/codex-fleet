use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    AppState,
};

#[derive(Serialize)]
pub struct NotificationConfig {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub config_json: String,
    pub enabled: bool,
    pub events_json: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateNotificationRequest {
    pub name: String,
    pub r#type: String,
    pub config_json: String,
    pub enabled: Option<bool>,
    pub events_json: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateNotificationRequest {
    pub name: Option<String>,
    pub config_json: Option<String>,
    pub enabled: Option<bool>,
    pub events_json: Option<String>,
}

pub async fn list_notifications(
    State(state): State<AppState>,
) -> Result<Json<Vec<NotificationConfig>>> {
    let rows = sqlx::query!(
        r#"SELECT id, name, "type", config_json, enabled, events_json, created_at FROM notification_configs ORDER BY created_at DESC"#
    )
    .fetch_all(&state.db)
    .await?;

    let configs = rows
        .into_iter()
        .map(|r| NotificationConfig {
            id: r.id,
            name: r.name,
            r#type: r.r#type,
            config_json: r.config_json,
            enabled: r.enabled,
            events_json: r.events_json,
            created_at: r.created_at.to_string(),
        })
        .collect();

    Ok(Json(configs))
}

pub async fn create_notification(
    State(state): State<AppState>,
    Json(req): Json<CreateNotificationRequest>,
) -> Result<Json<NotificationConfig>> {
    // Validate config_json is valid JSON
    serde_json::from_str::<serde_json::Value>(&req.config_json)
        .map_err(|e| AppError::BadRequest(format!("Invalid config_json: {}", e)))?;

    let id = Uuid::new_v4().to_string();
    let enabled = req.enabled.unwrap_or(true);
    let events_json = req
        .events_json
        .unwrap_or_else(|| r#"["agent_completed","agent_failed"]"#.into());
    let now = Utc::now();

    sqlx::query!(
        r#"INSERT INTO notification_configs (id, name, "type", config_json, enabled, events_json, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        id, req.name, req.r#type, req.config_json, enabled, events_json, now
    )
    .execute(&state.db)
    .await?;

    Ok(Json(NotificationConfig {
        id,
        name: req.name,
        r#type: req.r#type,
        config_json: req.config_json,
        enabled,
        events_json,
        created_at: now.to_string(),
    }))
}

pub async fn update_notification(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateNotificationRequest>,
) -> Result<Json<NotificationConfig>> {
    let existing = sqlx::query!(
        r#"SELECT id, name, "type", config_json, enabled, events_json, created_at FROM notification_configs WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Notification {} not found", id)))?;

    let name = req.name.unwrap_or(existing.name);
    let config_json = req.config_json.unwrap_or(existing.config_json);
    let enabled = req.enabled.unwrap_or(existing.enabled);
    let events_json = req.events_json.unwrap_or(existing.events_json);

    sqlx::query!(
        "UPDATE notification_configs SET name=$1, config_json=$2, enabled=$3, events_json=$4 WHERE id=$5",
        name, config_json, enabled, events_json, id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(NotificationConfig {
        id,
        name,
        r#type: existing.r#type,
        config_json,
        enabled,
        events_json,
        created_at: existing.created_at.to_string(),
    }))
}

pub async fn delete_notification(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM notification_configs WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Notification {} not found", id)));
    }

    Ok(Json(serde_json::json!({"message": "Notification config deleted"})))
}

/// Send notification to all enabled configs that match the event
pub async fn send_notification(
    state: &AppState,
    event: &str,
    payload: serde_json::Value,
) -> anyhow::Result<()> {
    let configs = sqlx::query!(
        "SELECT config_json, events_json FROM notification_configs WHERE enabled = true"
    )
    .fetch_all(&state.db)
    .await?;

    for config in configs {
        let events: Vec<String> = serde_json::from_str(&config.events_json).unwrap_or_default();
        if !events.contains(&event.to_string()) {
            continue;
        }

        let config_data: serde_json::Value = serde_json::from_str(&config.config_json)?;
        if let Some(url) = config_data.get("url").and_then(|u| u.as_str()) {
            let client = reqwest::Client::new();
            let mut builder = client.post(url).json(&payload);

            if let Some(headers) = config_data.get("headers").and_then(|h| h.as_object()) {
                for (k, v) in headers {
                    if let Some(v_str) = v.as_str() {
                        builder = builder.header(k.as_str(), v_str);
                    }
                }
            }

            if let Err(e) = builder.send().await {
                tracing::warn!("Webhook notification failed: {}", e);
            }
        }
    }

    Ok(())
}
