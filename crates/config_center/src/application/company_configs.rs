//! Read-only accessors for `company_configs` exposed across module boundaries.

use sqlx::PgPool;

pub async fn get_company_config_content(
    db: &PgPool,
    id: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT content FROM company_configs WHERE id = $1", id)
        .fetch_optional(db)
        .await
}
