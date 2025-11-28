use crate::error::{AppError, Result};
use crate::web_socket::actor::OutboundMsg;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// === Cache per Message Confirmations ===
#[derive(Clone)]
pub struct MessageConfirmationCache {
    // server_msg_id -> (client_msg_id, timestamp)
    entries: Arc<RwLock<HashMap<Uuid, (String, Instant)>>>,
}

impl MessageConfirmationCache {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(&self, server_id: Uuid, client_id: String) {
        let mut map = self.entries.write().await;
        map.insert(server_id, (client_id, Instant::now()));

        // Cleanup automatico se troppo grande
        if map.len() > 10000 {
            let cutoff = Instant::now() - std::time::Duration::from_secs(300);
            map.retain(|_, (_, time)| *time > cutoff);
        }
    }

    pub async fn get(&self, server_id: &Uuid) -> Option<String> {
        let map = self.entries.read().await;
        map.get(server_id)
            .filter(|(_, time)| time.elapsed() < std::time::Duration::from_secs(300))
            .map(|(id, _)| id.clone())
    }

    pub async fn cleanup(&self) {
        let mut map = self.entries.write().await;
        let cutoff = Instant::now() - std::time::Duration::from_secs(300);
        let before = map.len();
        map.retain(|_, (_, time)| *time > cutoff);
        let after = map.len();
        if before != after {
            debug!(
                "Cleaned up {} expired message confirmations",
                before - after
            );
        }
    }
}

// === NUOVO: Cache per Conversation Confirmations ===
// === Cache per Conversation Confirmations ===
#[derive(Clone)]
pub struct ConversationConfirmationCache {
    // server_conv_id -> (client_temp_id, timestamp)
    entries: Arc<RwLock<HashMap<Uuid, (String, Instant)>>>,
    // client_temp_id -> (server_conv_id, timestamp)
    reverse_entries: Arc<RwLock<HashMap<String, (Uuid, Instant)>>>,
}

impl ConversationConfirmationCache {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            reverse_entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert(&self, server_id: Uuid, client_temp_id: String) {
        let now = Instant::now();

        // Inserisci in entrambe le mappe
        {
            let mut map = self.entries.write().await;
            map.insert(server_id, (client_temp_id.clone(), now));

            // Cleanup automatico se troppo grande
            if map.len() > 1000 {
                let cutoff = now - std::time::Duration::from_secs(600); // 10 minuti per conversazioni
                map.retain(|_, (_, time)| *time > cutoff);
            }
        }

        {
            let mut reverse_map = self.reverse_entries.write().await;
            reverse_map.insert(client_temp_id, (server_id, now));

            // Cleanup anche reverse map
            if reverse_map.len() > 1000 {
                let cutoff = now - std::time::Duration::from_secs(600);
                reverse_map.retain(|_, (_, time)| *time > cutoff);
            }
        }
    }

    pub async fn get(&self, server_id: &Uuid) -> Option<String> {
        let map = self.entries.read().await;
        map.get(server_id)
            .filter(|(_, time)| time.elapsed() < std::time::Duration::from_secs(600)) // 10 minuti
            .map(|(id, _)| id.clone())
    }

    pub async fn get_by_temp_id(&self, client_temp_id: &str) -> Option<Uuid> {
        let map = self.reverse_entries.read().await;
        map.get(client_temp_id)
            .filter(|(_, time)| time.elapsed() < std::time::Duration::from_secs(600))
            .map(|(id, _)| *id)
    }

    pub async fn cleanup(&self) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(600);

        {
            let mut map = self.entries.write().await;
            let before = map.len();
            map.retain(|_, (_, time)| *time > cutoff);
            let after = map.len();
            if before != after {
                debug!(
                    "Cleaned up {} expired conversation confirmations",
                    before - after
                );
            }
        }

        {
            let mut reverse_map = self.reverse_entries.write().await;
            reverse_map.retain(|_, (_, time)| *time > cutoff);
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    // Canali per messaggi delle conversazioni (compatibile con codice esistente)
    pub channels: Arc<RwLock<HashMap<Uuid, broadcast::Sender<serde_json::Value>>>>,
    // Canali per notifiche utente (nuove conversazioni, etc.)
    pub user_notification_channels:
        Arc<RwLock<HashMap<Uuid, broadcast::Sender<serde_json::Value>>>>,
    // Cache per client_msg_id dei messaggi
    pub message_confirmation_cache: MessageConfirmationCache,
    // NUOVO: Cache per client_temp_id delle conversazioni
    pub conversation_confirmation_cache: ConversationConfirmationCache,
}

// === Strutture di supporto ===

#[derive(Debug, Clone, Serialize)]
pub struct UserEvent {
    pub sequence: u64,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub conversation_id: Option<Uuid>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SequencedMessage {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub author_id: Uuid,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub sequence: u64,
}

impl AppState {
    pub fn new(pool: sqlx::SqlitePool, jwt_secret: String) -> Self {
        Self {
            pool,
            jwt_secret,
            channels: Arc::new(RwLock::new(HashMap::new())),
            user_notification_channels: Arc::new(RwLock::new(HashMap::new())),
            message_confirmation_cache: MessageConfirmationCache::new(),
            conversation_confirmation_cache: ConversationConfirmationCache::new(), // NUOVO
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
        let conv_ids: Vec<String> =
            sqlx::query_scalar("SELECT conversation_id FROM participants WHERE user_id = ?")
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

        // Cleanup message confirmation cache periodicamente
        self.message_confirmation_cache.cleanup().await;

        // NUOVO: Cleanup conversation confirmation cache
        self.conversation_confirmation_cache.cleanup().await;

        if cleaned > 0 {
            info!("Cleaned up {} empty channels for user {}", cleaned, user_id);
        }
    }

    // === Sistema di Sequenze per User Events ===

    /// Ottieni prossimo numero di sequenza per un utente (atomico)
    pub async fn get_next_user_sequence(&self, user_id: Uuid) -> Result<u64> {
        let user_id_str = user_id.to_string();

        // INSERT OR IGNORE per creare record se non esiste
        sqlx::query(
            "INSERT OR IGNORE INTO user_sequences (user_id, current_sequence) VALUES (?, 0)",
        )
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

    // === Sistema di Sequenze per Messaggi ===

    /// Ottieni prossimo numero di sequenza per una conversazione (atomico)
    pub async fn get_next_message_sequence(&self, conversation_id: Uuid) -> Result<u64> {
        let conv_id_str = conversation_id.to_string();
        let now = chrono::Utc::now().timestamp();

        // INSERT OR IGNORE per creare record se non esiste
        sqlx::query("INSERT OR IGNORE INTO message_sequences (conversation_id, current_sequence, last_updated) VALUES (?, 0, ?)")
            .bind(&conv_id_str)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;

        // UPDATE atomico + RETURNING per ottenere il nuovo valore
        let row = sqlx::query("UPDATE message_sequences SET current_sequence = current_sequence + 1, last_updated = ? WHERE conversation_id = ? RETURNING current_sequence")
            .bind(now)
            .bind(&conv_id_str)
            .fetch_one(&self.pool)
            .await
            .map_err(AppError::from)?;

        let new_seq: i64 = row.try_get("current_sequence").map_err(AppError::from)?;
        Ok(new_seq as u64)
    }

    /// Ottieni numero di sequenza corrente per una conversazione (senza incrementare)
    pub async fn get_current_message_sequence(&self, conversation_id: Uuid) -> Result<u64> {
        let conv_id_str = conversation_id.to_string();

        let row =
            sqlx::query("SELECT current_sequence FROM message_sequences WHERE conversation_id = ?")
                .bind(&conv_id_str)
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

    // === Storage e Recovery ===

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
        self.store_user_event(user_id, sequence, event_type, &event_data, conversation_id)
            .await?;

        // Costruisci evento con metadati (SENZA user_id per evitare collisioni)
        let mut enriched_event = event_data;
        enriched_event["type"] = json!("user_event");
        enriched_event["sequence"] = json!(sequence);
        enriched_event["event_type"] = json!(event_type);

        if let Some(cid) = conversation_id {
            enriched_event["conversation_id"] = json!(cid);
        }

        // Invia tramite WebSocket (best effort)
        let user_tx = self.get_or_create_user_notification_channel(user_id).await;
        match user_tx.send(enriched_event) {
            Ok(receiver_count) => {
                debug!(
                "Sent sequenced event (seq={}) to user {} ({} receivers)",
                sequence, user_id, receiver_count
            );
            }
            Err(_) => {
                debug!(
                "No active receivers for user {} event (seq={}), stored for recovery",
                user_id, sequence
            );
            }
        }

        Ok(sequence)
    }

    // === Recovery Methods ===

    /// Recupera user events mancanti dal database
    pub async fn get_user_events_since(
        &self,
        user_id: Uuid,
        since_sequence: u64,
        limit: i64,
    ) -> Result<Vec<UserEvent>> {
        let user_id_str = user_id.to_string();

        let rows = sqlx::query(
            "SELECT sequence_num, event_type, event_data, conversation_id, created_at
             FROM user_events
             WHERE user_id = ? AND sequence_num > ?
             ORDER BY sequence_num ASC
             LIMIT ?",
        )
            .bind(&user_id_str)
            .bind(since_sequence as i64)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;

        let mut events = Vec::new();
        for row in rows {
            let sequence: i64 = row.try_get("sequence_num").map_err(AppError::from)?;
            let event_type: String = row.try_get("event_type").map_err(AppError::from)?;
            let event_data_str: String = row.try_get("event_data").map_err(AppError::from)?;
            let conversation_id_str: Option<String> =
                row.try_get("conversation_id").map_err(AppError::from)?;
            let created_at: i64 = row.try_get("created_at").map_err(AppError::from)?;

            let event_data: serde_json::Value = serde_json::from_str(&event_data_str)
                .map_err(|e| AppError::Internal(e.to_string()))?;

            let conversation_id = conversation_id_str.and_then(|s| Uuid::parse_str(&s).ok());

            events.push(UserEvent {
                sequence: sequence as u64,
                event_type,
                event_data,
                conversation_id,
                created_at,
            });
        }

        Ok(events)
    }

    /// Recupera messaggi mancanti per una conversazione
    pub async fn get_messages_since_sequence(
        &self,
        conversation_id: Uuid,
        since_sequence: u64,
        limit: i64,
    ) -> Result<Vec<SequencedMessage>> {
        let conv_id_str = conversation_id.to_string();

        let rows = sqlx::query(
            "SELECT m.id, m.author_id, u.username as author_username,
                    m.content, m.created_at, m.sequence_num
             FROM messages m
             JOIN users u ON m.author_id = u.id
             WHERE m.conversation_id = ? AND m.sequence_num > ?
             ORDER BY m.sequence_num ASC
             LIMIT ?",
        )
            .bind(&conv_id_str)
            .bind(since_sequence as i64)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;

        let mut messages = Vec::new();
        for row in rows {
            let id_str: String = row.try_get("id").map_err(AppError::from)?;
            let author_id_str: String = row.try_get("author_id").map_err(AppError::from)?;
            let sequence: Option<i64> = row.try_get("sequence_num").map_err(AppError::from)?;

            messages.push(SequencedMessage {
                id: Uuid::parse_str(&id_str)
                    .map_err(|_| AppError::Internal("Invalid UUID".into()))?,
                conversation_id,
                author_id: Uuid::parse_str(&author_id_str)
                    .map_err(|_| AppError::Internal("Invalid UUID".into()))?,
                author_username: row.try_get("author_username").map_err(AppError::from)?,
                content: row.try_get("content").map_err(AppError::from)?,
                created_at: row.try_get("created_at").map_err(AppError::from)?,
                sequence: sequence.map(|s| s as u64).unwrap_or(0),
            });
        }

        Ok(messages)
    }

    // === Ping Handlers ===

    pub async fn handle_ping(
        &self,
        user_id: Uuid,
        client_user_seq: Option<u64>,
        out_tx: &mpsc::Sender<OutboundMsg>,
    ) -> Result<()> {
        let current_user_seq = self.get_current_user_sequence(user_id).await?;

        let mut response = json!({
            "type": "pong",
            "timestamp": chrono::Utc::now().timestamp(),
            "current_user_sequence": current_user_seq,
        });

        let mut gap_detected = false;

        if let Some(client_seq) = client_user_seq {
            if client_seq < current_user_seq {
                gap_detected = true;
                response["user_events_gap"] = json!({
                    "detected": true,
                    "client_seq": client_seq,
                    "server_seq": current_user_seq,
                    "gap_size": current_user_seq - client_seq
                });
            }
            self.update_user_ping_tracking(user_id, client_seq).await?;
        }

        response["gaps_detected"] = json!(gap_detected);

        if let Ok(txt) = serde_json::to_string(&response) {
            out_tx
                .send(OutboundMsg::Text(txt))
                .await
                .map_err(|e| AppError::Internal(format!("Failed to send pong: {}", e)))?;
        }

        Ok(())
    }

    // === Resume Senders ===

    /// Invia resume di user events mancanti (MODIFICATO per includere client_temp_id)
    pub async fn send_user_events_resume(
        &self,
        user_id: Uuid,
        events: Vec<UserEvent>,
        out_tx: &mpsc::Sender<OutboundMsg>,
    ) -> Result<()> {
        // NUOVO: Prepara gli eventi con last_message aggiornato e client_temp_id
        let mut enriched_events = Vec::new();

        for e in events {
            let mut event = e.event_data.clone();

            // Se è un evento di conversazione (created_complete O confirmation), aggiorna e aggiungi client_temp_id
            if e.event_type == "conversation_created_complete"
                || e.event_type == "conversation_confirmation" {
                if let Some(conv_data) = event.get("conversation").and_then(|c| c.as_object()) {
                    if let Some(conv_id_value) = conv_data.get("id") {
                        if let Some(conv_id_str) = conv_id_value.as_str() {
                            if let Ok(conv_id) = Uuid::parse_str(conv_id_str) {
                                // NUOVO: Aggiungi client_temp_id se presente nella cache
                                if let Some(client_temp_id) =
                                    self.conversation_confirmation_cache.get(&conv_id).await
                                {
                                    if let Some(conversation) = event.get_mut("conversation") {
                                        conversation["client_temp_id"] = json!(client_temp_id);
                                        debug!(
                                            "Added client_temp_id {} to conversation {} in resume",
                                            client_temp_id, conv_id
                                        );
                                    }
                                }

                                // Recupera l'ultimo messaggio attuale per questa conversazione
                                if let Ok(last_msg) =
                                    self.get_latest_message_for_conversation(conv_id).await
                                {
                                    if let Some(msg_data) = last_msg {
                                        // Aggiorna il last_message nell'evento
                                        if let Some(conversation) = event.get_mut("conversation") {
                                            conversation["last_message"] = msg_data;
                                            debug!(
                                                "Updated last_message for conversation {} in resume",
                                                conv_id
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // CRITICAL FIX: Aggiungi "type" basato su "event_type" per compatibilità client
            event["type"] = json!(&e.event_type);
            event["sequence"] = json!(e.sequence);
            event["event_type"] = json!(&e.event_type);
            event["created_at"] = json!(e.created_at);
            if let Some(conv_id) = e.conversation_id {
                event["conversation_id"] = json!(conv_id);
            }

            enriched_events.push(event);
        }

        let resume_msg = json!({
            "type": "user_events_resume",
            "events": enriched_events,
            "count": enriched_events.len(),
            "timestamp": chrono::Utc::now().timestamp()
        });

        if let Ok(txt) = serde_json::to_string(&resume_msg) {
            out_tx.send(OutboundMsg::Text(txt)).await.map_err(|e| {
                AppError::Internal(format!("Failed to send user events resume: {}", e))
            })?;
        }

        info!(
            "Sent {} user events in resume to user {}",
            enriched_events.len(),
            user_id
        );
        Ok(())
    }

    /// Invia resume di messaggi mancanti (MODIFICATO per includere client_msg_id)
    pub async fn send_messages_resume(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        messages: Vec<SequencedMessage>,
        out_tx: &mpsc::Sender<OutboundMsg>,
    ) -> Result<()> {
        info!("PREPARING RESUME for {} messages", messages.len());

        // Arricchisci i messaggi con client_msg_id dalla cache
        let mut enriched_messages = Vec::new();
        let messages_len = messages.len();

        for msg in messages {
            let client_msg_id = self.message_confirmation_cache.get(&msg.id).await;

            // AGGIUNGI QUESTO LOG
            info!(
                "RESUME: Message {} has cached client_msg_id: {:?}",
                msg.id, client_msg_id
            );

            let mut msg_json = json!({
                "id": msg.id,
                "author_id": msg.author_id,
                "author_username": &msg.author_username,
                "content": &msg.content,
                "created_at": msg.created_at,
                "sequence_num": msg.sequence
            });

            // Aggiungi client_msg_id se disponibile in cache
            if let Some(client_id) = client_msg_id {
                msg_json["client_msg_id"] = json!(client_id);
                info!(
                    "INCLUDING client_msg_id {} in resume for message {}",
                    client_id, msg.id
                );
            } else {
                info!("NO client_msg_id found in cache for message {}", msg.id);
            }

            enriched_messages.push(msg_json);
        }

        let resume_msg = json!({
            "type": "messages_resume",
            "conversation_id": conversation_id,
            "messages": enriched_messages,
            "count": messages_len,
            "timestamp": chrono::Utc::now().timestamp()
        });

        if let Ok(txt) = serde_json::to_string(&resume_msg) {
            out_tx.send(OutboundMsg::Text(txt)).await.map_err(|e| {
                AppError::Internal(format!("Failed to send messages resume: {}", e))
            })?;
        }

        info!(
            "Sent {} messages in resume for conversation {} to user {}",
            messages_len, conversation_id, user_id
        );
        Ok(())
    }

    // === Helper Methods ===

    /// Aggiorna tracking del ping utente
    async fn update_user_ping_tracking(&self, user_id: Uuid, client_sequence: u64) -> Result<()> {
        let user_id_str = user_id.to_string();
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "UPDATE user_sequences
             SET last_ping_sequence = ?, last_ping_at = ?
             WHERE user_id = ?",
        )
            .bind(client_sequence as i64)
            .bind(now)
            .bind(&user_id_str)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;

        // Marca eventi come delivered se il client è aggiornato
        if client_sequence >= self.get_current_user_sequence(user_id).await? {
            sqlx::query(
                "UPDATE user_events
                 SET delivered = TRUE
                 WHERE user_id = ? AND sequence_num <= ? AND delivered = FALSE",
            )
                .bind(&user_id_str)
                .bind(client_sequence as i64)
                .execute(&self.pool)
                .await
                .map_err(AppError::from)?;
        }

        Ok(())
    }

    // Aggiungi questo metodo helper dopo gli altri metodi in AppState
    async fn get_latest_message_for_conversation(
        &self,
        conversation_id: Uuid,
    ) -> Result<Option<Value>> {
        let conv_id_str = conversation_id.to_string();

        let query = r#"
            SELECT
                m.id,
                m.author_id,
                u.username as author_username,
                m.content,
                m.created_at,
                m.sequence_num
            FROM messages m
            INNER JOIN users u ON m.author_id = u.id
            WHERE m.conversation_id = ?
            ORDER BY m.created_at DESC
            LIMIT 1
        "#;

        let row = sqlx::query(query)
            .bind(&conv_id_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)?;

        if let Some(row) = row {
            let mut message = json!({
                "id": row.try_get::<String, _>("id").unwrap_or_default(),
                "author_id": row.try_get::<String, _>("author_id").unwrap_or_default(),
                "author_username": row.try_get::<String, _>("author_username").unwrap_or_default(),
                "content": row.try_get::<String, _>("content").unwrap_or_default(),
                "created_at": row.try_get::<i64, _>("created_at").unwrap_or(0)
            });

            if let Ok(Some(seq)) = row.try_get::<Option<i64>, _>("sequence_num") {
                message["sequence_num"] = json!(seq);
            }

            // Aggiungi client_msg_id se presente in cache
            if let Ok(msg_id) = Uuid::parse_str(&row.try_get::<String, _>("id").unwrap_or_default())
            {
                if let Some(client_id) = self.message_confirmation_cache.get(&msg_id).await {
                    message["client_msg_id"] = json!(client_id);
                }
            }

            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    // === NUOVE FUNZIONI PER ARRICCHIMENTO EVENTI ===

    /// Recupera user_events e arricchisce le notifications con messaggi completi
    ///
    /// Questa funzione:
    /// 1. Recupera eventi dal DB (possono essere notifications leggere)
    /// 2. Identifica quali eventi sono "new_message_notification"
    /// 3. Fa batch SELECT per recuperare tutti i messaggi in 1 query
    /// 4. Arricchisce le notifications convertendole in "new_message" con contenuto completo
    /// 5. Ritorna eventi nel formato che il client si aspetta
    pub async fn get_user_events_since_enriched(
        &self,
        user_id: Uuid,
        from_sequence: u64,
        limit: i64,
    ) -> Result<Vec<Value>> {
        let user_id_str = user_id.to_string();

        // 1. Recupera eventi dal DB
        let rows = sqlx::query(
            "SELECT id, event_type, event_data, sequence_num, conversation_id, created_at
             FROM user_events
             WHERE user_id = ? AND sequence_num > ?
             ORDER BY sequence_num ASC
             LIMIT ?"
        )
            .bind(&user_id_str)
            .bind(from_sequence as i64)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // 2. Estrai tutti i message_id dalle notifications
        let mut message_ids_to_fetch = Vec::new();

        for row in &rows {
            let event_type: String = row.try_get("event_type").map_err(AppError::from)?;

            if event_type == "new_message_notification" {
                let event_data_str: String = row.try_get("event_data").map_err(AppError::from)?;

                if let Ok(event_data) = serde_json::from_str::<Value>(&event_data_str) {
                    if let Some(msg_id_str) = event_data.get("message_id").and_then(|v| v.as_str()) {
                        if let Ok(msg_id) = Uuid::parse_str(msg_id_str) {
                            message_ids_to_fetch.push(msg_id);
                        }
                    }
                }
            }
        }

        // 3.  Batch SELECT tutti i messaggi (1 query invece di N)
        let messages_map = if !message_ids_to_fetch.is_empty() {
            info!(
                "Enriching {} new_message_notification events for user {}",
                message_ids_to_fetch.len(),
                user_id
            );
            self.batch_get_messages(&message_ids_to_fetch).await?
        } else {
            HashMap::new()
        };

        // 4. Arricchisci gli eventi
        let mut enriched_events = Vec::new();

        for row in rows {
            let event_type: String = row.try_get("event_type").map_err(AppError::from)?;
            let event_data_str: String = row.try_get("event_data").map_err(AppError::from)?;
            let sequence: i64 = row.try_get("sequence_num").map_err(AppError::from)?;
            let created_at: i64 = row.try_get("created_at").map_err(AppError::from)?;

            let mut event_data: Value = serde_json::from_str(&event_data_str)
                .map_err(|e| AppError::Internal(format!("JSON parse error: {}", e)))?;

            let final_event_type = if event_type == "new_message_notification" {
                // Arricchisci notification con messaggio completo
                if let Some(message_id_str) = event_data.get("message_id").and_then(|v| v.as_str()) {
                    if let Ok(message_id) = Uuid::parse_str(message_id_str) {
                        if let Some(full_message) = messages_map.get(&message_id) {
                            // Converti notification in new_message (formato che client si aspetta)
                            event_data = json!({
                    "type": "new_message",
                    "conversation_id": event_data.get("conversation_id")
                        .unwrap_or(&json!(null)),
                    "conversation_sequence": event_data.get("message_sequence")
                        .unwrap_or(&json!(null)),
                    "message": full_message
                });
                        } else {
                            warn!(
                    "Message {} not found for enrichment (may have been deleted)",
                    message_id
                );
                        }
                    }
                }

                
                "new_message"
            } else {
                
                &event_type
            };

            // Aggiungi metadata per compatibilità client
            event_data["type"] = json!(&final_event_type);
            event_data["sequence"] = json!(sequence);
            event_data["event_type"] = json!(&final_event_type);
            event_data["created_at"] = json!(created_at);

            enriched_events.push(event_data);
        }

        info!(
            "Enriched {} events for user {} (from sequence {})",
            enriched_events.len(),
            user_id,
            from_sequence
        );

        Ok(enriched_events)
    }

    /// Helper: Batch SELECT messaggi per ID (1 query per tutti)
    ///
    /// Recupera tutti i messaggi richiesti in una singola query usando WHERE IN
    /// Ritorna una HashMap per lookup veloce: message_id -> messaggio completo
    async fn batch_get_messages(&self, message_ids: &[Uuid]) -> Result<HashMap<Uuid, Value>> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }

        info!("Batch fetching {} messages for enrichment", message_ids.len());

        // Costruisci query con placeholders
        let placeholders = message_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query_str = format!(
            "SELECT m.id, m.conversation_id, m.author_id, u.username as author_username,
                    m.content, m.created_at, m.sequence_num
             FROM messages m
             INNER JOIN users u ON m.author_id = u.id
             WHERE m.id IN ({})",
            placeholders
        );

        // Bind tutti i message_id
        let mut query = sqlx::query(&query_str);
        for id in message_ids {
            query = query.bind(id.to_string());
        }

        let rows = query.fetch_all(&self.pool).await.map_err(AppError::from)?;

        // Costruisci HashMap per lookup veloce
        let mut messages_map = HashMap::new();

        for row in rows {
            let id_str: String = row.try_get("id").map_err(AppError::from)?;
            let id = Uuid::parse_str(&id_str)
                .map_err(|_| AppError::Internal("Invalid message UUID in database".into()))?;

            let mut message = json!({
                "id": id,
                "conversation_id": row.try_get::<String, _>("conversation_id")
                    .map_err(AppError::from)?,
                "author_id": row.try_get::<String, _>("author_id")
                    .map_err(AppError::from)?,
                "author_username": row.try_get::<String, _>("author_username")
                    .map_err(AppError::from)?,
                "content": row.try_get::<String, _>("content")
                    .map_err(AppError::from)?,
                "created_at": row.try_get::<i64, _>("created_at")
                    .map_err(AppError::from)?,
                "sequence_num": row.try_get::<i64, _>("sequence_num")
                    .map_err(AppError::from)?
            });

            // Aggiungi client_msg_id dalla cache se disponibile
            if let Some(client_id) = self.message_confirmation_cache.get(&id).await {
                message["client_msg_id"] = json!(client_id);
            }

            messages_map.insert(id, message);
        }

        info!(
            "Batch fetched {} out of {} requested messages",
            messages_map.len(),
            message_ids.len()
        );

        Ok(messages_map)
    }
}