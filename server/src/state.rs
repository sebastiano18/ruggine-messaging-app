use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    // Canali per messaggi delle conversazioni (compatibile con codice esistente)
    pub channels: Arc<RwLock<HashMap<Uuid, broadcast::Sender<serde_json::Value>>>>,
    // NUOVO: Canali per notifiche utente (nuove conversazioni, etc.)
    pub user_notification_channels: Arc<RwLock<HashMap<Uuid, broadcast::Sender<serde_json::Value>>>>,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool, jwt_secret: String) -> Self {
        Self {
            pool,
            jwt_secret,
            channels: Arc::new(RwLock::new(HashMap::new())),
            user_notification_channels: Arc::new(RwLock::new(HashMap::new())),
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

    /// Ottiene o crea canale notifiche per un utente
    pub async fn get_or_create_user_notification_channel(
        &self,
        user_id: Uuid,
    ) -> broadcast::Sender<serde_json::Value> {
        {
            let map = self.user_notification_channels.read().await;
            if let Some(tx) = map.get(&user_id) {
                return tx.clone();
            }
        }

        let mut map = self.user_notification_channels.write().await;
        if let Some(tx) = map.get(&user_id) {
            return tx.clone();
        }

        let (tx, _rx) = broadcast::channel::<serde_json::Value>(256);
        map.insert(user_id, tx.clone());
        info!("Created user notification channel for {}", user_id);
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

    /// CORE: Notifica creazione di nuova conversazione a tutti i partecipanti
    pub async fn notify_conversation_created(
        &self,
        conversation_id: Uuid,
        participant_ids: &[Uuid],
        creator_id: Uuid,
        conversation_kind: &str,
        conversation_title: Option<&str>,
    ) -> usize {
        let notification = serde_json::json!({
            "type": "conversation_created",
            "conversation_id": conversation_id,
            "creator_id": creator_id,
            "kind": conversation_kind,
            "title": conversation_title,
            "timestamp": chrono::Utc::now().timestamp()
        });

        let mut total_notified = 0;

        for &participant_id in participant_ids {
            let user_tx = self.get_or_create_user_notification_channel(participant_id).await;

            match user_tx.send(notification.clone()) {
                Ok(receivers) => {
                    total_notified += receivers;
                    info!("Notified user {} of new conversation {} ({} active receivers)",
                          participant_id, conversation_id, receivers);
                }
                Err(_) => {
                    info!("No active receivers for user {} notification", participant_id);
                }
            }
        }

        info!("Sent conversation creation notification to {} total receivers", total_notified);
        total_notified
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

    /// Cleanup canali vuoti (migliorato)
    pub async fn cleanup_empty_channels(&self, user_id: Uuid) {
        let user_conversations = self.get_user_channels(user_id).await
            .unwrap_or_default();

        let mut cleaned = 0;

        // Cleanup conversation channels
        {
            let mut conv_map = self.channels.write().await;
            for (conv_id, tx) in &user_conversations {
                if tx.receiver_count() == 0 {
                    conv_map.remove(conv_id);
                    cleaned += 1;
                }
            }
        }

        // Cleanup user notification channel
        {
            let mut user_map = self.user_notification_channels.write().await;
            if let Some(tx) = user_map.get(&user_id) {
                if tx.receiver_count() == 0 {
                    user_map.remove(&user_id);
                    cleaned += 1;
                }
            }
        }

        if cleaned > 0 {
            info!("Cleaned up {} empty channels for user {}", cleaned, user_id);
        }
    }
}