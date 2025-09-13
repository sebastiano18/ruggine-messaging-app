use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    pub channels: Arc<RwLock<HashMap<Uuid, broadcast::Sender<serde_json::Value>>>>,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool, jwt_secret: String) -> Self {
        Self {
            pool,
            jwt_secret,
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Factory method thread-safe per ottenere o creare un broadcast channel
    /// Questo metodo risolve le race condition nella creazione dei canali
    pub async fn get_or_create_broadcast_tx(
        &self,
        conv_id: Uuid,
    ) -> broadcast::Sender<serde_json::Value> {
        // Prima prova con un read lock (caso comune: il canale esiste già)
        {
            let map = self.channels.read().await;
            if let Some(tx) = map.get(&conv_id) {
                return tx.clone();
            }
        }

        // Se non esiste, usa write lock per crearlo
        let mut map = self.channels.write().await;

        // Double-check: qualcun altro potrebbe averlo creato nel frattempo
        if let Some(tx) = map.get(&conv_id) {
            return tx.clone();
        }

        // Crea il nuovo canale
        let (tx, _rx) = broadcast::channel::<serde_json::Value>(1024);
        map.insert(conv_id, tx.clone());
        info!("Created new broadcast channel for conversation {}", conv_id);

        tx
    }

    /// Metodo helper per ottenere tutti i canali di un utente in modo thread-safe
    pub async fn get_user_channels(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(Uuid, broadcast::Sender<serde_json::Value>)>, sqlx::Error> {
        // Prima ottieni le conversation IDs dal database
        let conv_ids: Vec<String> =
            sqlx::query_scalar(r#"SELECT conversation_id FROM participants WHERE user_id = ?"#)
                .bind(user_id.to_string())
                .fetch_all(&self.pool)
                .await?;

        let mut result = Vec::new();

        // Converti e ottieni i canali
        for conv_id_str in conv_ids {
            if let Ok(conv_id) = Uuid::parse_str(&conv_id_str) {
                let tx = self.get_or_create_broadcast_tx(conv_id).await;
                result.push((conv_id, tx));
            }
        }

        Ok(result)
    }

    /// Ottiene statistiche sui canali (utile per monitoring)
    pub async fn get_channel_stats(&self) -> (usize, usize) {
        let map = self.channels.read().await;
        let total_channels = map.len();
        let total_receivers: usize = map.values().map(|tx| tx.receiver_count()).sum();
        (total_channels, total_receivers)
    }

    /// Rimuove un canale specifico se è vuoto (thread-safe)
    pub async fn try_remove_empty_channel(&self, conv_id: Uuid) -> bool {
        let mut map = self.channels.write().await;

        if let Some(tx) = map.get(&conv_id) {
            if tx.receiver_count() == 0 {
                map.remove(&conv_id);
                info!("Removed empty channel for conversation {}", conv_id);
                return true;
            }
        }

        false
    }
}
