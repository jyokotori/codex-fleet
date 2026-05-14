use std::env;
use urlencoding::encode;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub port: u16,
    pub master_key: String,
    pub database_url: String,
    pub db_max_connections: u32,
    pub db_acquire_timeout_secs: u64,
    pub jwt_secret: String,
    pub access_token_minutes: i64,
    pub refresh_token_days: i64,
    pub max_login_failures: i32,
    pub lock_minutes: i64,
    pub initial_admin_username: String,
    pub initial_admin_password: String,
    pub initial_admin_display_name: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let pg_user = env::var("POSTGRES_USER").unwrap_or_else(|_| "codexfleet".into());
        let pg_password = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "codexfleet".into());
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".into());
        let pg_db = env::var("POSTGRES_DB").unwrap_or_else(|_| "codexfleet".into());

        let database_url = format!(
            "postgres://{}:{}@{}:{}/{}",
            encode(&pg_user),
            encode(&pg_password),
            pg_host,
            pg_port,
            pg_db,
        );

        Self {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .expect("PORT must be a number"),
            master_key: env::var("CODEX_MASTER_KEY")
                .unwrap_or_else(|_| "dev-master-key-change-in-production!".into()),
            database_url,
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            db_acquire_timeout_secs: env::var("DB_ACQUIRE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-jwt-secret-change-in-production".into()),
            access_token_minutes: env::var("ACCESS_TOKEN_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            refresh_token_days: env::var("REFRESH_TOKEN_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            max_login_failures: env::var("MAX_LOGIN_FAILURES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            lock_minutes: env::var("LOGIN_LOCK_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15),
            initial_admin_username: env::var("INITIAL_ADMIN_USERNAME")
                .unwrap_or_else(|_| "codex".into()),
            initial_admin_password: env::var("INITIAL_ADMIN_PASSWORD")
                .unwrap_or_else(|_| "codex".into()),
            initial_admin_display_name: env::var("INITIAL_ADMIN_DISPLAY_NAME")
                .unwrap_or_else(|_| "Codex Admin".into()),
        }
    }
}
