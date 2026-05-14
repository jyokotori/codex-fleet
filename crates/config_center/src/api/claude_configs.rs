use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shared_kernel::{AppContext, AppError, Result};

/// Sentinel returned by list/get when a token is set; the same value sent back
/// by the client on update is treated as "leave token unchanged".
const TOKEN_MASK: &str = "********";

fn mask_token(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    TOKEN_MASK.to_string()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ClaudeConfig {
    pub id: String,
    pub name: String,
    pub anthropic_base_url: String,
    /// Always masked when sent over the API. The frontend renders this back
    /// into the form field on edit; sending it unchanged via PUT is a no-op.
    pub anthropic_auth_token: String,
    pub anthropic_model: String,
    pub default_opus_model: String,
    pub default_sonnet_model: String,
    pub default_haiku_model: String,
    pub subagent_model: String,
    pub effort_level: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct CreateClaudeConfigRequest {
    pub name: String,
    pub anthropic_base_url: Option<String>,
    pub anthropic_auth_token: Option<String>,
    pub anthropic_model: Option<String>,
    pub default_opus_model: Option<String>,
    pub default_sonnet_model: Option<String>,
    pub default_haiku_model: Option<String>,
    pub subagent_model: Option<String>,
    pub effort_level: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateClaudeConfigRequest {
    pub name: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub anthropic_auth_token: Option<String>,
    pub anthropic_model: Option<String>,
    pub default_opus_model: Option<String>,
    pub default_sonnet_model: Option<String>,
    pub default_haiku_model: Option<String>,
    pub subagent_model: Option<String>,
    pub effort_level: Option<String>,
}

pub async fn list_claude_configs(
    State(state): State<AppContext>,
) -> Result<Json<Vec<ClaudeConfig>>> {
    let rows = sqlx::query!(
        "SELECT id, name, anthropic_base_url, anthropic_auth_token, anthropic_model,
                default_opus_model, default_sonnet_model, default_haiku_model,
                subagent_model, effort_level, created_at, updated_at
         FROM claude_configs ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    let configs = rows
        .into_iter()
        .map(|r| ClaudeConfig {
            id: r.id,
            name: r.name,
            anthropic_base_url: r.anthropic_base_url,
            anthropic_auth_token: mask_token(&r.anthropic_auth_token),
            anthropic_model: r.anthropic_model,
            default_opus_model: r.default_opus_model,
            default_sonnet_model: r.default_sonnet_model,
            default_haiku_model: r.default_haiku_model,
            subagent_model: r.subagent_model,
            effort_level: r.effort_level,
            created_at: r.created_at.to_string(),
            updated_at: r.updated_at.to_string(),
        })
        .collect();

    Ok(Json(configs))
}

pub async fn create_claude_config(
    State(state): State<AppContext>,
    Json(req): Json<CreateClaudeConfigRequest>,
) -> Result<Json<ClaudeConfig>> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let anthropic_base_url = req
        .anthropic_base_url
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());
    let mut anthropic_auth_token = req.anthropic_auth_token.unwrap_or_default();
    // Defensive: never let the mask sentinel land in the DB.
    if anthropic_auth_token == TOKEN_MASK {
        anthropic_auth_token = String::new();
    }
    let anthropic_model = req
        .anthropic_model
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "claude-opus-4-7[1m]".to_string());
    let default_opus_model = req.default_opus_model.unwrap_or_default();
    let default_sonnet_model = req.default_sonnet_model.unwrap_or_default();
    let default_haiku_model = req.default_haiku_model.unwrap_or_default();
    let subagent_model = req.subagent_model.unwrap_or_default();
    let effort_level = req
        .effort_level
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "xhigh".to_string());

    sqlx::query!(
        "INSERT INTO claude_configs (id, name, anthropic_base_url, anthropic_auth_token,
            anthropic_model, default_opus_model, default_sonnet_model, default_haiku_model,
            subagent_model, effort_level, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        id,
        req.name,
        anthropic_base_url,
        anthropic_auth_token,
        anthropic_model,
        default_opus_model,
        default_sonnet_model,
        default_haiku_model,
        subagent_model,
        effort_level,
        now,
        now,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ClaudeConfig {
        id,
        name: req.name,
        anthropic_base_url,
        anthropic_auth_token: mask_token(&anthropic_auth_token),
        anthropic_model,
        default_opus_model,
        default_sonnet_model,
        default_haiku_model,
        subagent_model,
        effort_level,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn update_claude_config(
    State(state): State<AppContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateClaudeConfigRequest>,
) -> Result<Json<ClaudeConfig>> {
    let existing = sqlx::query!(
        "SELECT id, name, anthropic_base_url, anthropic_auth_token, anthropic_model,
                default_opus_model, default_sonnet_model, default_haiku_model,
                subagent_model, effort_level, created_at
         FROM claude_configs WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("ClaudeConfig {} not found", id)))?;

    let name = req.name.unwrap_or(existing.name);
    let anthropic_base_url = req.anthropic_base_url.unwrap_or(existing.anthropic_base_url);
    // Treat masked value (or absent field) as "keep existing token".
    let anthropic_auth_token = match req.anthropic_auth_token {
        None => existing.anthropic_auth_token,
        Some(v) if v == TOKEN_MASK => existing.anthropic_auth_token,
        Some(v) => v,
    };
    let anthropic_model = req.anthropic_model.unwrap_or(existing.anthropic_model);
    let default_opus_model = req
        .default_opus_model
        .unwrap_or(existing.default_opus_model);
    let default_sonnet_model = req
        .default_sonnet_model
        .unwrap_or(existing.default_sonnet_model);
    let default_haiku_model = req
        .default_haiku_model
        .unwrap_or(existing.default_haiku_model);
    let subagent_model = req.subagent_model.unwrap_or(existing.subagent_model);
    let effort_level = req.effort_level.unwrap_or(existing.effort_level);
    let now = Utc::now();

    sqlx::query!(
        "UPDATE claude_configs SET name=$1, anthropic_base_url=$2, anthropic_auth_token=$3,
            anthropic_model=$4, default_opus_model=$5, default_sonnet_model=$6,
            default_haiku_model=$7, subagent_model=$8, effort_level=$9, updated_at=$10
         WHERE id=$11",
        name,
        anthropic_base_url,
        anthropic_auth_token,
        anthropic_model,
        default_opus_model,
        default_sonnet_model,
        default_haiku_model,
        subagent_model,
        effort_level,
        now,
        id,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(ClaudeConfig {
        id,
        name,
        anthropic_base_url,
        anthropic_auth_token: mask_token(&anthropic_auth_token),
        anthropic_model,
        default_opus_model,
        default_sonnet_model,
        default_haiku_model,
        subagent_model,
        effort_level,
        created_at: existing.created_at.to_string(),
        updated_at: now.to_string(),
    }))
}

pub async fn delete_claude_config(
    State(state): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!("DELETE FROM claude_configs WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "ClaudeConfig {} not found",
            id
        )));
    }

    Ok(Json(serde_json::json!({"message": "ClaudeConfig deleted"})))
}
