use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, Mutex};

use sqlx::PgPool;

use crate::config::AppConfig;

pub type ProvisionTx = broadcast::Sender<String>;
pub type TaskTx = broadcast::Sender<String>;

#[derive(Clone)]
pub struct AppContext {
    pub db: PgPool,
    pub config: AppConfig,
    pub provision_channels: Arc<Mutex<HashMap<String, ProvisionTx>>>,
    pub task_channels: Arc<Mutex<HashMap<String, TaskTx>>>,
}
