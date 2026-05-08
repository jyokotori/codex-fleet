//! Read-only accessors for `codex_configs` exposed across module boundaries.

use sqlx::PgPool;

pub struct CodexConfigContent {
    pub config_toml: String,
    pub auth_json: String,
}

pub async fn get_codex_config_content(
    db: &PgPool,
    id: &str,
) -> Result<Option<CodexConfigContent>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT config_toml, auth_json FROM codex_configs WHERE id = $1",
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| CodexConfigContent {
        config_toml: r.config_toml,
        auth_json: r.auth_json,
    }))
}
