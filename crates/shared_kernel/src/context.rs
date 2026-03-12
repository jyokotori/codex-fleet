use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, watch, Mutex};

use sqlx::PgPool;

use crate::config::AppConfig;

pub type ProvisionTx = broadcast::Sender<String>;
pub type TaskTx = broadcast::Sender<String>;
pub type AbortTx = watch::Sender<bool>;

#[derive(Clone)]
pub struct AppContext {
    pub db: PgPool,
    pub config: AppConfig,
    pub provision_channels: Arc<Mutex<HashMap<String, ProvisionTx>>>,
    pub task_channels: Arc<Mutex<HashMap<String, TaskTx>>>,
    pub task_abort_signals: Arc<Mutex<HashMap<String, AbortTx>>>,
}
