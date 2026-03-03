use sqlx::PgPool;

use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppContext {
    pub db: PgPool,
    pub config: AppConfig,
}
