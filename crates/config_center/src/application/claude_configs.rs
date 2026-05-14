//! Read-only accessor for `claude_configs` exposed across module boundaries.

use sqlx::PgPool;

pub struct ClaudeConfigContent {
    pub anthropic_base_url: String,
    pub anthropic_auth_token: String,
    pub anthropic_model: String,
    pub default_opus_model: String,
    pub default_sonnet_model: String,
    pub default_haiku_model: String,
    pub subagent_model: String,
    pub effort_level: String,
}

pub async fn get_claude_config_content(
    db: &PgPool,
    id: &str,
) -> Result<Option<ClaudeConfigContent>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT anthropic_base_url, anthropic_auth_token, anthropic_model,
                default_opus_model, default_sonnet_model, default_haiku_model,
                subagent_model, effort_level
         FROM claude_configs WHERE id = $1",
        id
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| ClaudeConfigContent {
        anthropic_base_url: r.anthropic_base_url,
        anthropic_auth_token: r.anthropic_auth_token,
        anthropic_model: r.anthropic_model,
        default_opus_model: r.default_opus_model,
        default_sonnet_model: r.default_sonnet_model,
        default_haiku_model: r.default_haiku_model,
        subagent_model: r.subagent_model,
        effort_level: r.effort_level,
    }))
}
