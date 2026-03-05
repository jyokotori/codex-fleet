use std::env;
use urlencoding::encode;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub master_key: String,
    pub database_url: String,
}

impl Config {
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

        Config {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .expect("PORT must be a number"),
            master_key: env::var("CODEX_MASTER_KEY")
                .unwrap_or_else(|_| "dev-master-key-change-in-production!".into()),
            database_url,
        }
    }
}
