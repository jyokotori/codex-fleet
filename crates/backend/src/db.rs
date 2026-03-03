use sqlx::{sqlite::{SqliteConnectOptions, SqlitePoolOptions}, SqlitePool};
use std::str::FromStr;
use tracing::info;

use crate::{config::Config, crypto::Crypto};

pub async fn create_pool(config: &Config) -> anyhow::Result<SqlitePool> {
    // Ensure parent directory exists for file-based SQLite
    if let Some(path) = config.database_url.strip_prefix("sqlite://") {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
    }

    let connect_opts = SqliteConnectOptions::from_str(&config.database_url)?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_opts)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;
    info!("Database migrations applied");

    // Seed default user
    seed_default_user(&pool, &Crypto::new(&config.master_key)).await?;

    Ok(pool)
}

async fn seed_default_user(pool: &SqlitePool, crypto: &Crypto) -> anyhow::Result<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE username = 'codex'")
        .fetch_one(pool)
        .await?;

    if count.0 == 0 {
        let id = uuid::Uuid::new_v4().to_string();
        let password_encrypted = crypto.encrypt("codex")?;
        sqlx::query(
            "INSERT INTO users (id, username, display_name, password_encrypted) VALUES (?, 'codex', 'Codex Admin', ?)"
        )
        .bind(&id)
        .bind(&password_encrypted)
        .execute(pool)
        .await?;
        info!("Default user 'codex' created");
    }

    Ok(())
}
