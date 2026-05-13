use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sqlx::Row;

use shared_kernel::Result;

use crate::application::token::hash_token;

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Non-sensitive display form: first 6 chars + ellipsis + last 4 chars.
/// Short tokens fall back to a coarse mask.
pub fn token_preview(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 12 {
        return "•".repeat(chars.len().min(8));
    }
    let head: String = chars.iter().take(6).collect();
    let tail: String = chars
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

pub async fn find_user_id_by_token(db: &sqlx::PgPool, token: &str) -> Result<Option<String>> {
    let hash = hash_token(token);
    let row = sqlx::query("SELECT user_id FROM api_access_tokens WHERE token_hash = $1")
        .bind(&hash)
        .fetch_optional(db)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("user_id")))
}

pub async fn get_token_preview_for_user(
    db: &sqlx::PgPool,
    user_id: &str,
) -> Result<Option<String>> {
    let row = sqlx::query("SELECT token_preview FROM api_access_tokens WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("token_preview")))
}

/// Generates a new token, stores only its hash + preview, returns the raw token.
pub async fn upsert_token_for_user(db: &sqlx::PgPool, user_id: &str) -> Result<String> {
    let raw = generate_token();
    let hash = hash_token(&raw);
    let preview = token_preview(&raw);
    sqlx::query(
        "INSERT INTO api_access_tokens (user_id, token_hash, token_preview, created_at, updated_at)
         VALUES ($1, $2, $3, NOW(), NOW())
         ON CONFLICT (user_id) DO UPDATE
            SET token_hash = EXCLUDED.token_hash,
                token_preview = EXCLUDED.token_preview,
                updated_at = NOW()",
    )
    .bind(user_id)
    .bind(&hash)
    .bind(&preview)
    .execute(db)
    .await?;
    Ok(raw)
}
