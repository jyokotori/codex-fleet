use axum::{
    extract::{Extension, Path, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use shared_kernel::{AppContext, AppError, AuthContext, Result};

#[derive(Serialize)]
pub struct ThirdPartyAppConfig {
    pub provider: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct UpsertThirdPartyAppConfigRequest {
    pub config: serde_json::Value,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct IntegrationsStatus {
    pub dingtalk: ProviderStatus,
}

#[derive(Serialize)]
pub struct ProviderStatus {
    pub enabled: bool,
}

fn require_admin(auth: &AuthContext) -> Result<()> {
    if auth.has_role("admin") {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Admin role required to manage integrations".into(),
        ))
    }
}

pub async fn list_integrations(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<ThirdPartyAppConfig>>> {
    require_admin(&auth)?;

    let rows = sqlx::query!(
        r#"SELECT provider, config_json, enabled, created_at, updated_at
           FROM third_party_app_configs
           ORDER BY provider"#
    )
    .fetch_all(&state.db)
    .await?;

    let configs = rows
        .into_iter()
        .map(|r| ThirdPartyAppConfig {
            config: serde_json::from_str(&r.config_json).unwrap_or(serde_json::json!({})),
            provider: r.provider,
            enabled: r.enabled,
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(configs))
}

pub async fn get_integration(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Path(provider): Path<String>,
) -> Result<Json<ThirdPartyAppConfig>> {
    require_admin(&auth)?;

    let row = sqlx::query!(
        r#"SELECT provider, config_json, enabled, created_at, updated_at
           FROM third_party_app_configs
           WHERE provider = $1"#,
        provider
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Integration {} not found", provider)))?;

    Ok(Json(ThirdPartyAppConfig {
        config: serde_json::from_str(&row.config_json).unwrap_or(serde_json::json!({})),
        provider: row.provider,
        enabled: row.enabled,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }))
}

pub async fn upsert_integration(
    State(state): State<AppContext>,
    Extension(auth): Extension<AuthContext>,
    Path(provider): Path<String>,
    Json(req): Json<UpsertThirdPartyAppConfigRequest>,
) -> Result<Json<ThirdPartyAppConfig>> {
    require_admin(&auth)?;

    let config_str = serde_json::to_string(&req.config).unwrap_or_else(|_| "{}".into());
    let now = Utc::now();

    let row = sqlx::query!(
        r#"INSERT INTO third_party_app_configs (provider, config_json, enabled, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $4)
           ON CONFLICT (provider) DO UPDATE
           SET config_json = EXCLUDED.config_json,
               enabled = EXCLUDED.enabled,
               updated_at = EXCLUDED.updated_at
           RETURNING provider, config_json, enabled, created_at, updated_at"#,
        provider,
        config_str,
        req.enabled,
        now,
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(ThirdPartyAppConfig {
        config: serde_json::from_str(&row.config_json).unwrap_or(serde_json::json!({})),
        provider: row.provider,
        enabled: row.enabled,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }))
}

pub async fn integrations_status(
    State(state): State<AppContext>,
) -> Result<Json<IntegrationsStatus>> {
    let dingtalk = dingtalk_enabled(&state.db).await;
    Ok(Json(IntegrationsStatus {
        dingtalk: ProviderStatus { enabled: dingtalk },
    }))
}

pub async fn dingtalk_enabled(db: &sqlx::PgPool) -> bool {
    let row = match sqlx::query!(
        "SELECT config_json, enabled FROM third_party_app_configs WHERE provider = 'dingtalk'"
    )
    .fetch_optional(db)
    .await
    {
        Ok(Some(r)) => r,
        _ => return false,
    };
    if !row.enabled {
        return false;
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&row.config_json).unwrap_or(serde_json::json!({}));
    let key = parsed.get("app_key").and_then(|v| v.as_str()).unwrap_or("");
    let secret = parsed
        .get("app_secret")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let robot = parsed
        .get("robot_code")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    !key.is_empty() && !secret.is_empty() && !robot.is_empty()
}
