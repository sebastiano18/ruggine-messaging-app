use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    pub channels: Arc<RwLock<HashMap<i64, broadcast::Sender<serde_json::Value>>>>,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool, jwt_secret: String) -> Self {
        Self {
            pool,
            jwt_secret,
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}
