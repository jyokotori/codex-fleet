use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, watch, Mutex, RwLock};

use sqlx::PgPool;

use crate::config::AppConfig;

pub type ProvisionTx = broadcast::Sender<String>;
pub type TaskTx = broadcast::Sender<String>;
pub type AbortTx = watch::Sender<bool>;

#[derive(Clone)]
struct CachedStatus {
    status: String,
    cached_at: Instant,
}

#[derive(Clone)]
pub struct AgentStatusCache {
    inner: Arc<RwLock<HashMap<String, CachedStatus>>>,
    ttl: Duration,
}

impl AgentStatusCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    pub async fn get(&self, id: &str) -> Option<String> {
        let map = self.inner.read().await;
        map.get(id)
            .filter(|c| c.cached_at.elapsed() < self.ttl)
            .map(|c| c.status.clone())
    }

    pub async fn get_many(&self, ids: &[String]) -> HashMap<String, String> {
        let map = self.inner.read().await;
        let mut result = HashMap::new();
        for id in ids {
            if let Some(cached) = map.get(id) {
                if cached.cached_at.elapsed() < self.ttl {
                    result.insert(id.clone(), cached.status.clone());
                }
            }
        }
        result
    }

    pub async fn set(&self, id: String, status: String) {
        let mut map = self.inner.write().await;
        map.insert(
            id,
            CachedStatus {
                status,
                cached_at: Instant::now(),
            },
        );
    }

    pub async fn set_many(&self, entries: Vec<(String, String)>) {
        let mut map = self.inner.write().await;
        let now = Instant::now();
        for (id, status) in entries {
            map.insert(
                id,
                CachedStatus {
                    status,
                    cached_at: now,
                },
            );
        }
    }

    pub async fn invalidate(&self, id: &str) {
        let mut map = self.inner.write().await;
        map.remove(id);
    }
}

#[derive(Clone)]
pub struct AppContext {
    pub db: PgPool,
    pub config: AppConfig,
    pub provision_channels: Arc<Mutex<HashMap<String, ProvisionTx>>>,
    pub task_channels: Arc<Mutex<HashMap<String, TaskTx>>>,
    pub task_abort_signals: Arc<Mutex<HashMap<String, AbortTx>>>,
    pub agent_status_cache: AgentStatusCache,
}
