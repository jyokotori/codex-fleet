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

#[derive(Serialize, Deserialize, Clone)]
pub struct CodexConfig {
    pub id: String,
    pub name: String,
    pub config_toml: String,
    pub auth_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct CreateCodexConfigRequest {
    pub name: String,
    pub config_toml: Option<String>,
    pub auth_json: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateCodexConfigRequest {
    pub name: Option<String>,
    pub config_toml: Option<String>,
    pub auth_json: Option<String>,
}

pub async fn list_codex_configs(
    State(state): State<AppState>,
) -> Result<Json<Vec<CodexConfig>>> {
    let rows = sqlx::query!(
        "SELECT id, name, config_toml, auth_json, created_at, updated_at FROM codex_configs ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    let configs = rows
        .into_iter()
        .map(|r| CodexConfig {
            id: r.id,
            name: r.name,
            config_toml: r.config_toml,
            auth_json: r.auth_json,
            created_at: r.created_at.to_string(),
            updated_at: r.updated_at.to_string(),
        })
        .collect();

    Ok(Json(configs))
}

pub async fn create_codex_config(
    State(state): State<AppState>,
    Json(req): Json<CreateCodexConfigRequest>,
) -> Result<Json<CodexConfig>> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let config_toml = req.config_toml.unwrap_or_default();
    let auth_json = req.auth_json.unwrap_or_default();

    sqlx::query!(
        "INSERT INTO codex_configs (id, name, config_toml, auth_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
        id, req.name, config_toml, auth_json, now, now
    )
    .execute(&state.db)
    .await?;

    Ok(Json(CodexConfig {
        id,
        name: req.name,
        config_toml,
        auth_json,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn update_codex_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCodexConfigRequest>,
) -> Result<Json<CodexConfig>> {
    let existing = sqlx::query!(
        "SELECT id, name, config_toml, auth_json, created_at FROM codex_configs WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("CodexConfig {} not found", id)))?;

    let name = req.name.unwrap_or(existing.name);
    let config_toml = req.config_toml.unwrap_or(existing.config_toml);
    let auth_json = req.auth_json.unwrap_or(existing.auth_json);
    let now = Utc::now();

    sqlx::query!(
        "UPDATE codex_configs SET name=?, config_toml=?, auth_json=?, updated_at=? WHERE id=?",
        name, config_toml, auth_json, now, id
    )
    .execute(&state.db)
    .await?;

    Ok(Json(CodexConfig {
        id,
        name,
        config_toml,
        auth_json,
        created_at: existing.created_at.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn delete_codex_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM codex_configs WHERE id = ?", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("CodexConfig {} not found", id)));
    }

    Ok(Json(serde_json::json!({"message": "CodexConfig deleted"})))
}
