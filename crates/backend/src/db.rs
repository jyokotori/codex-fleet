use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use std::str::FromStr;
use tracing::info;

use shared_kernel::AppConfig;

pub async fn create_pool(config: &AppConfig) -> anyhow::Result<PgPool> {
    let connect_opts = PgConnectOptions::from_str(&config.database_url)?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(connect_opts)
        .await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;
    info!("Database migrations applied");

    seed_default_iam(&pool, config).await?;

    Ok(pool)
}

fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    Ok(argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .to_string())
}

async fn seed_default_iam(pool: &PgPool, config: &AppConfig) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO roles (id, code, name, is_system, created_at) VALUES ($1, $2, $3, true, NOW()) ON CONFLICT (code) DO NOTHING",
    )
    .bind("role-admin")
    .bind("admin")
    .bind("Administrator")
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO roles (id, code, name, is_system, created_at) VALUES ($1, $2, $3, true, NOW()) ON CONFLICT (code) DO NOTHING",
    )
    .bind("role-member")
    .bind("member")
    .bind("Member")
    .execute(pool)
    .await?;

    let permissions = [
        ("perm-user-create", "user:create", "user", "create"),
        ("perm-user-list", "user:list", "user", "list"),
        (
            "perm-user-reset-password",
            "user:reset_password",
            "user",
            "reset_password",
        ),
        (
            "perm-user-change-status",
            "user:change_status",
            "user",
            "change_status",
        ),
        ("perm-user-unlock", "user:unlock", "user", "unlock"),
        (
            "perm-profile-change-password",
            "profile:change_password",
            "profile",
            "change_password",
        ),
    ];

    for (id, code, resource, action) in permissions {
        sqlx::query(
            "INSERT INTO permissions (id, code, resource, action, created_at) VALUES ($1, $2, $3, $4, NOW()) ON CONFLICT (code) DO NOTHING",
        )
        .bind(id)
        .bind(code)
        .bind(resource)
        .bind(action)
        .execute(pool)
        .await?;
    }

    let admin_role = sqlx::query("SELECT id FROM roles WHERE code = 'admin'")
        .fetch_one(pool)
        .await?;
    let admin_role_id: String = sqlx::Row::get(&admin_role, "id");

    for (_, code, _, _) in permissions {
        let perm = sqlx::query("SELECT id FROM permissions WHERE code = $1")
            .bind(code)
            .fetch_one(pool)
            .await?;
        let perm_id: String = sqlx::Row::get(&perm, "id");

        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_id, created_at) VALUES ($1, $2, NOW()) ON CONFLICT (role_id, permission_id) DO NOTHING",
        )
        .bind(&admin_role_id)
        .bind(perm_id)
        .execute(pool)
        .await?;
    }

    let user = sqlx::query("SELECT id FROM users WHERE username = $1")
        .bind(&config.initial_admin_username)
        .fetch_optional(pool)
        .await?;
    let user_id = if let Some(row) = user {
        sqlx::Row::get::<String, _>(&row, "id")
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        let password_hash = hash_password(&config.initial_admin_password)?;
        sqlx::query(
            "INSERT INTO users (id, username, display_name, password_hash, status, failed_attempts, created_at, updated_at) VALUES ($1, $2, $3, $4, 'active', 0, NOW(), NOW())",
        )
        .bind(&id)
        .bind(&config.initial_admin_username)
        .bind(&config.initial_admin_display_name)
        .bind(password_hash)
        .execute(pool)
        .await?;
        info!(
            "Default admin user '{}' created",
            config.initial_admin_username
        );
        id
    };

    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id, created_at) VALUES ($1, $2, NOW()) ON CONFLICT (user_id, role_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(admin_role_id)
    .execute(pool)
    .await?;

    Ok(())
}
