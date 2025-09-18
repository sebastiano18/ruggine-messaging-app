use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn, error, debug};
use uuid::Uuid;
use serde_json::{Value, json};
use sqlx::Row;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    // Canali per messaggi delle conversazioni (compatibile con codice esistente)
    pub channels: Arc<RwLock<HashMap<Uuid, broadcast::Sender<serde_json::Value>>>>,
    // Canali per notifiche utente (nuove conversazioni, etc.)
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

    // === Metodi per gestione canali broadcast ===

    pub async fn get_or_create_broadcast_tx(
        &self,
        conv_id: Uuid,
    ) -> broadcast::Sender<serde_json::Value> {
        // Prima prova con read lock
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

        let (tx, _rx) = broadcast::channel::<serde_json::Value>(1024);
        map.insert(conv_id, tx.clone());
        info!("Created new broadcast channel for conversation {}", conv_id);
        tx
    }

    pub async fn get_or_create_user_notification_channel(
        &self,
        user_id: Uuid,
    ) -> broadcast::Sender<serde_json::Value> {
        // Prima prova con read lock
        {
            let map = self.user_notification_channels.read().await;
            if let Some(tx) = map.get(&user_id) {
                return tx.clone();
            }
        }

        // Se non esiste, usa write lock per crearlo
        let mut map = self.user_notification_channels.write().await;
        // Double-check
        if let Some(tx) = map.get(&user_id) {
            return tx.clone();
        }

        let (tx, _rx) = broadcast::channel::<Value>(256);
        map.insert(user_id, tx.clone());
        info!("Created user notification channel for {}", user_id);
        tx
    }

    pub async fn get_user_channels(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(Uuid, broadcast::Sender<Value>)>> {
        let conv_ids: Vec<String> = sqlx::query_scalar("SELECT conversation_id FROM participants WHERE user_id = ?")
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await?;

        let mut result = Vec::new();
        for conv_id_str in conv_ids {
            if let Ok(conv_id) = Uuid::parse_str(&conv_id_str) {
                let tx = self.get_or_create_broadcast_tx(conv_id).await;
                result.push((conv_id, tx));
            }
        }
        Ok(result)
    }

    pub async fn get_channel_stats(&self) -> (usize, usize) {
        let map = self.channels.read().await;
        let total_channels = map.len();
        let total_receivers: usize = map.values().map(|tx| tx.receiver_count()).sum();
        (total_channels, total_receivers)
    }

    pub async fn cleanup_empty_channels(&self, user_id: Uuid) {
        let user_conversations = self.get_user_channels(user_id).await.unwrap_or_default();
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

    // === Sistema di Sequenze per Eventi Utente ===

    /// Ottieni prossimo numero di sequenza per un utente (atomico)
    pub async fn get_next_user_sequence(&self, user_id: Uuid) -> Result<u64> {
        let user_id_str = user_id.to_string();

        // INSERT OR IGNORE per creare record se non esiste
        sqlx::query("INSERT OR IGNORE INTO user_sequences (user_id, current_sequence) VALUES (?, 0)")
            .bind(&user_id_str)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;

        // UPDATE atomico + RETURNING per ottenere il nuovo valore
        let row = sqlx::query("UPDATE user_sequences SET current_sequence = current_sequence + 1 WHERE user_id = ? RETURNING current_sequence")
            .bind(&user_id_str)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::from)?;

        let new_seq: i64 = row.try_get("current_sequence").map_err(AppError::from)?;
        Ok(new_seq as u64)
    }

    /// Ottieni numero di sequenza corrente per un utente (senza incrementare)
    pub async fn get_current_user_sequence(&self, user_id: Uuid) -> Result<u64> {
        let user_id_str = user_id.to_string();

        let row = sqlx::query("SELECT current_sequence FROM user_sequences WHERE user_id = ?")
            .bind(&user_id_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)?;

        let seq = if let Some(row) = row {
            let seq: i64 = row.try_get("current_sequence").map_err(AppError::from)?;
            seq as u64
        } else {
            0
        };

        Ok(seq)
    }

    /// Salva evento nel database per recovery
    pub async fn store_user_event(
        &self,
        user_id: Uuid,
        sequence_num: u64,
        event_type: &str,
        event_data: &Value,
        conversation_id: Option<Uuid>,
    ) -> Result<()> {
        let user_id_str = user_id.to_string();
        let event_data_str = event_data.to_string();
        let conversation_id_str = conversation_id.map(|id| id.to_string());
        let now = chrono::Utc::now().timestamp();

        sqlx::query("INSERT INTO user_events (user_id, sequence_num, event_type, event_data, conversation_id, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&user_id_str)
            .bind(sequence_num as i64)
            .bind(event_type)
            .bind(&event_data_str)
            .bind(&conversation_id_str)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;

        Ok(())
    }

    /// Invia evento sequenziato all'utente (store nel DB + WebSocket)
    pub async fn send_sequenced_event_to_user(
        &self,
        user_id: Uuid,
        event_type: &str,
        event_data: Value,
        conversation_id: Option<Uuid>,
    ) -> Result<u64> {
        // Ottieni prossima sequence
        let sequence = self.get_next_user_sequence(user_id).await?;

        // Store evento per recovery
        self.store_user_event(user_id, sequence, event_type, &event_data, conversation_id).await?;

        // Aggiungi sequence ai dati dell'evento
        let mut enriched_event = event_data;
        enriched_event["sequence"] = json!(sequence);
        enriched_event["user_id"] = json!(user_id);

        // Invia tramite WebSocket (best effort)
        let user_tx = self.get_or_create_user_notification_channel(user_id).await;
        match user_tx.send(enriched_event) {
            Ok(receiver_count) => {
                debug!("Sent sequenced event (seq={}) to user {} ({} receivers)",
                      sequence, user_id, receiver_count);
            }
            Err(_) => {
                debug!("No active receivers for user {} event (seq={}), stored for recovery",
                      user_id, sequence);
            }
        }

        Ok(sequence)
    }

    /// Gestisce ping con sequence - rileva gap e recupera eventi
    pub async fn handle_ping_with_sequence(
        &self,
        user_id: Uuid,
        client_last_sequence: u64,
        out_tx: &tokio::sync::mpsc::Sender<super::web_socket::actor::OutboundMsg>,
    ) -> Result<()> {
        let user_id_str = user_id.to_string();
        let now = chrono::Utc::now().timestamp();

        // Aggiorna tracking ping
        sqlx::query("INSERT OR REPLACE INTO user_sequences (user_id, current_sequence, last_ping_sequence, last_ping_at) VALUES (?, COALESCE((SELECT current_sequence FROM user_sequences WHERE user_id = ?), 0), ?, ?)")
            .bind(&user_id_str)
            .bind(&user_id_str)
            .bind(client_last_sequence as i64)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;

        // Ottieni sequence corrente del server
        let server_sequence = self.get_current_user_sequence(user_id).await?;

        let pong_response = if client_last_sequence < server_sequence {
            // Gap rilevato! Recupera eventi mancanti
            let gap_size = server_sequence - client_last_sequence;
            warn!("Gap detected for user {}: client={}, server={}, gap={}",
                  user_id, client_last_sequence, server_sequence, gap_size);

            // Ottieni eventi mancanti dal database
            let missed_events = sqlx::query("SELECT sequence_num, event_type, event_data FROM user_events WHERE user_id = ? AND sequence_num > ? AND sequence_num <= ? ORDER BY sequence_num")
                .bind(&user_id_str)
                .bind(client_last_sequence as i64)
                .bind(server_sequence as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(AppError::from)?;

            // Invia ogni evento mancante
            for event in &missed_events {
                let sequence_num: i64 = event.try_get("sequence_num").map_err(AppError::from)?;
                let event_data_str: String = event.try_get("event_data").map_err(AppError::from)?;

                if let Ok(mut event_data) = serde_json::from_str::<Value>(&event_data_str) {
                    event_data["sequence"] = json!(sequence_num);
                    event_data["recovery"] = json!(true);

                    if let Ok(recovery_txt) = serde_json::to_string(&event_data) {
                        let msg = super::web_socket::actor::OutboundMsg::Text(recovery_txt);
                        if out_tx.send(msg).await.is_err() {
                            warn!("Failed to send recovery event to user {}", user_id);
                            break;
                        }
                    }
                }
            }

            info!("Recovered {} missed events for user {}", missed_events.len(), user_id);

            // Pong con info gap
            json!({
                "type": "pong",
                "server_sequence": server_sequence,
                "your_sequence": client_last_sequence,
                "gap_detected": true,
                "gap_size": gap_size,
                "events_recovered": missed_events.len(),
                "timestamp": now
            })
        } else {
            // Nessun gap - pong normale
            json!({
                "type": "pong",
                "server_sequence": server_sequence,
                "your_sequence": client_last_sequence,
                "gap_detected": false,
                "timestamp": now
            })
        };

        // Invia risposta pong
        if let Ok(pong_txt) = serde_json::to_string(&pong_response) {
            let msg = super::web_socket::actor::OutboundMsg::Text(pong_txt);
            let _ = out_tx.send(msg).await;
        }

        Ok(())
    }

    /// Cleanup eventi vecchi (chiama periodicamente)
    pub async fn cleanup_old_events(&self, older_than_days: i64) -> Result<u64> {
        let cutoff = chrono::Utc::now().timestamp() - (older_than_days * 24 * 60 * 60);

        let result = sqlx::query("DELETE FROM user_events WHERE created_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;

        if result.rows_affected() > 0 {
            info!("Cleaned up {} old user events", result.rows_affected());
        }

        Ok(result.rows_affected())
    }

    // === Metodi per notifiche legacy (compatibilità) ===

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
}