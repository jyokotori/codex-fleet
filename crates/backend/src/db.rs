use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use std::str::FromStr;
use tracing::info;

use crate::{config::Config, crypto::Crypto};

pub async fn create_pool(config: &Config) -> anyhow::Result<PgPool> {
    let connect_opts = PgConnectOptions::from_str(&config.database_url)?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(connect_opts)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;
    info!("Database migrations applied");

    // Seed default user
    seed_default_user(&pool, &Crypto::new(&config.master_key)).await?;

    Ok(pool)
}

async fn seed_default_user(pool: &PgPool, crypto: &Crypto) -> anyhow::Result<()> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE username = 'codex'")
            .fetch_one(pool)
            .await?;

    if count.0 == 0 {
        let id = uuid::Uuid::new_v4().to_string();
        let password_encrypted = crypto.encrypt("codex")?;
        sqlx::query(
            "INSERT INTO users (id, username, display_name, password_encrypted) VALUES ($1, 'codex', 'Codex Admin', $2)",
        )
        .bind(&id)
        .bind(&password_encrypted)
        .execute(pool)
        .await?;
        info!("Default user 'codex' created");
    }

    Ok(())
}
