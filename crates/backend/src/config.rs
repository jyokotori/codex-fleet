use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub master_key: String,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            port: env::var("PORT")
                .unwrap_or_else(|_| "3000".into())
                .parse()
                .expect("PORT must be a number"),
            master_key: env::var("CODEX_MASTER_KEY")
                .unwrap_or_else(|_| "dev-master-key-change-in-production!".into()),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:///tmp/codex-fleet.db".into()),
        }
    }
}
