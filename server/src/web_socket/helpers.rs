use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

use super::actor::OutboundMsg;

pub async fn broadcast_to_conversation(
    state: &AppState,
    conversation_id: Uuid,
    payload: Value,
) -> Result<usize> {
    let tx = state.get_or_create_broadcast_tx(conversation_id).await;
    match tx.send(payload) {
        Ok(n) => {
            info!("broadcast {} subs for {}", n, conversation_id);
            Ok(n)
        }
        Err(e) => {
            warn!("broadcast fail {}: {}", conversation_id, e);
            Err(AppError::Internal(format!("broadcast error: {e}")))
        }
    }
}

/// Gestisce messaggi di chat con sequenze per nuove conversazioni
pub async fn handle_chat_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    let cid = value
        .get("cid")
        .or_else(|| value.get("conversation_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing conversation id (cid)".into()))?;

    let conversation_id =
        Uuid::parse_str(cid).map_err(|_| AppError::BadRequest("invalid conversation id".into()))?;

    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing content".into()))?;

    let target_username = value
        .get("target_username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    // Inizia una transazione per consistenza
    let mut tx = state.pool.begin().await.map_err(AppError::from)?;

    let conversation_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
        .bind(&conversation_id_str)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::from)?;

    let mut is_new_conversation = false;
    let mut participant_ids = Vec::new();

    if conversation_exists == 0 {
        info!("Creating new DM conversation {} for first message from user {}", conversation_id, user_id);
        is_new_conversation = true;

        if let Some(ref target_user) = target_username {
            let target_row = sqlx::query("SELECT id FROM users WHERE username = ?")
                .bind(target_user)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?;

            match target_row {
                Some(row) => {
                    let id_str: String = row.try_get("id").map_err(AppError::from)?;
                    let target_user_id = Uuid::parse_str(&id_str)
                        .map_err(|_| AppError::BadRequest("Invalid target user ID".into()))?;

                    // Crea la conversazione DM
                    sqlx::query("INSERT INTO conversations(id, kind, title, owner_id, created_at) VALUES(?, 'dm', NULL, ?, strftime('%s','now'))")
                        .bind(&conversation_id_str)
                        .bind(&user_id_str)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::from)?;

                    // Aggiungi entrambi i partecipanti
                    sqlx::query("INSERT INTO participants(conversation_id, user_id, role) VALUES(?, ?, 'member')")
                        .bind(&conversation_id_str)
                        .bind(&user_id_str)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::from)?;

                    let target_id_str = target_user_id.to_string();
                    sqlx::query("INSERT INTO participants(conversation_id, user_id, role) VALUES(?, ?, 'member')")
                        .bind(&conversation_id_str)
                        .bind(&target_id_str)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::from)?;

                    participant_ids = vec![user_id, target_user_id];

                    info!("Created DM conversation {} and added both users {} and {}",
                          conversation_id, user_id, target_user_id);
                }
                None => {
                    return Err(AppError::BadRequest(
                        format!("Target user '{}' not found", target_user)
                    ));
                }
            }
        } else {
            return Err(AppError::BadRequest("Target username required for new DM".into()));
        }
    } else {
        // Conversazione esistente - controllo autorizzazione normale
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?")
            .bind(&conversation_id_str)
            .bind(&user_id_str)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::from)?;

        if count == 0 {
            return Err(AppError::Forbidden);
        }
    }

    // Salvataggio del messaggio nel database
    let id = Uuid::new_v4();
    let ts = Utc::now().timestamp();
    let id_str = id.to_string();

    sqlx::query("INSERT INTO messages (id, conversation_id, author_id, content, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id_str)
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .bind(content)
        .bind(ts)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

    // Commit della transazione prima delle notifiche
    tx.commit().await.map_err(AppError::from)?;

    info!("Message {} saved to DB", id);

    // Gestione diversa per nuove conversazioni vs esistenti
    if is_new_conversation {
        // Per nuove conversazioni: invia eventi sequenziati ai partecipanti
        info!("New conversation created - sending sequenced notifications");

        let event_data = json!({
            "type": "conversation_created",
            "conversation_id": conversation_id,
            "creator_id": user_id,
            "kind": "dm",
            "title": null,
            "first_message": {
                "id": id,
                "author_id": user_id,
                "author_username": username,
                "content": content,
                "created_at": ts
            }
        });

        // CORRETTO: Invia evento sequenziato SOLO ai riceventi (non al creatore)
        for &participant_id in &participant_ids {
            if participant_id != user_id {  // Escludi il creatore
                match state.send_sequenced_event_to_user(
                    participant_id,
                    "conversation_created",
                    event_data.clone(),
                    Some(conversation_id)
                ).await {
                    Ok(sequence) => {
                        info!("Sent conversation_created event (seq={}) to recipient {}", sequence, participant_id);
                    }
                    Err(e) => {
                        warn!("Failed to send conversation_created event to recipient {}: {}", participant_id, e);
                    }
                }
            }
        }

        info!("Message {} stored in new conversation with sequence events", id);
    } else {
        // Per conversazioni esistenti: broadcast normale
        let event = json!({
            "type": "chat_message",
            "id": id,
            "cid": conversation_id,
            "author_id": user_id,
            "author_username": username,
            "content": content,
            "created_at": ts
        });

        match broadcast_to_conversation(state, conversation_id, event).await {
            Ok(delivered) if delivered > 0 => {
                info!("Message {} delivered immediately to {} receivers", id, delivered);
            }
            Ok(_) => {
                info!("Message {} stored - no active receivers", id);
            }
            Err(e) => {
                warn!("Failed to broadcast message {}: {}", id, e);
            }
        }
    }

    Ok(())
}

/// Gestisce notifiche dal canale utente - aggiornato per eventi sequenziati
pub async fn handle_user_notification(
    state: &AppState,
    notification: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    let notification_type = notification
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match notification_type {
        "conversation_created" => {
            let conversation_id = notification
                .get("conversation_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| AppError::BadRequest("invalid conversation_id".into()))?;

            let creator_id = notification
                .get("creator_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            info!("Processing conversation_created notification for user {} - conversation {}",
                  user_id, conversation_id);

            // Setup subscription alla conversazione
            if let Err(e) = setup_conversation_subscription(state, user_id, conversation_id).await {
                warn!("Failed to setup conversation subscription for user {}: {}", user_id, e);
            }

            // Se l'evento ha sequence, rimandalo al client così com'è
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx.send(OutboundMsg::Text(notification_txt)).await.is_err() {
                    warn!("Failed to send conversation_created event to user {}", user_id);
                } else {
                    info!("Forwarded conversation_created event to user {}", user_id);
                }
            }

            Ok(())
        }
        _ => {
            // Altri tipi di notifiche - forward diretto se hanno sequence
            if notification.get("sequence").is_some() {
                if let Ok(notification_txt) = serde_json::to_string(&notification) {
                    if out_tx.send(OutboundMsg::Text(notification_txt)).await.is_err() {
                        warn!("Failed to send sequenced event to user {}", user_id);
                    } else {
                        debug!("Forwarded sequenced event to user {}", user_id);
                    }
                }
            } else {
                warn!("Unknown user notification type: {}", notification_type);
            }
            Ok(())
        }
    }
}

/// Configura l'iscrizione dell'utente al broadcast channel della conversazione
async fn setup_conversation_subscription(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<()> {
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    // Verifica che l'utente sia effettivamente partecipante della conversazione
    let is_participant: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?")
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if is_participant == 0 {
        return Err(AppError::Forbidden);
    }

    // Ottieni/crea il broadcast channel per questa conversazione
    let conv_tx = state.get_or_create_broadcast_tx(conversation_id).await;
    let receiver_count = conv_tx.receiver_count();

    info!("Conversation {} broadcast channel ready for user {} ({} current receivers)",
          conversation_id, user_id, receiver_count);

    Ok(())
}

/// Invia fetch events automatici per tutte le conversazioni dell'utente al momento della connessione
pub async fn send_initial_fetch_events(
    state: &AppState,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    let user_id_str = user_id.to_string();

    // Ottieni tutte le conversazioni dell'utente con il conteggio messaggi
    let rows = sqlx::query("SELECT DISTINCT p.conversation_id, (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = p.conversation_id) as message_count FROM participants p WHERE p.user_id = ?")
        .bind(&user_id_str)
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let mut total_sent = 0;

    for row in rows {
        let conversation_id_str: String = row.try_get("conversation_id")
            .map_err(|e| AppError::Internal(format!("Failed to get conversation_id: {}", e)))?;

        let message_count: i64 = row.try_get("message_count")
            .map_err(|e| AppError::Internal(format!("Failed to get message_count: {}", e)))?;

        // Invia fetch event solo se ci sono messaggi
        if message_count > 0 {
            if let Ok(conversation_id) = Uuid::parse_str(&conversation_id_str) {
                let fetch_event = json!({
                    "type": "fetch_conversation_messages",
                    "conversation_id": conversation_id,
                    "message_count": message_count,
                    "reason": "initial_connection",
                    "timestamp": Utc::now().timestamp()
                });

                if let Ok(txt) = serde_json::to_string(&fetch_event) {
                    if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                        warn!("Failed to send initial fetch event to user {}", user_id);
                        break;
                    } else {
                        total_sent += 1;
                        debug!("Sent initial fetch event to user {} for conversation {} ({} messages)",
                              user_id, conversation_id, message_count);
                    }
                }
            }
        }
    }

    info!("Sent {} initial fetch events to user {}", total_sent, user_id);
    Ok(())
}

/// Response struct per l'API di fetch messaggi
#[derive(serde::Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub author_id: String,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
}

/// Endpoint API per fetch messaggi conversazione
pub async fn get_conversation_messages_api(
    state: &AppState,
    conversation_id: Uuid,
    user_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<MessageResponse>> {
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    // Verifica autorizzazione
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?")
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if count == 0 {
        return Err(AppError::Forbidden);
    }

    let limit = limit.unwrap_or(50).min(100);

    let messages = sqlx::query("SELECT m.id, m.author_id, u.username as author_username, m.content, m.created_at FROM messages m JOIN users u ON m.author_id = u.id WHERE m.conversation_id = ? ORDER BY m.created_at ASC LIMIT ?")
        .bind(&conversation_id_str)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let mut response = Vec::new();
    for row in messages {
        let message = MessageResponse {
            id: row.try_get("id").map_err(AppError::from)?,
            author_id: row.try_get("author_id").map_err(AppError::from)?,
            author_username: row.try_get("author_username").map_err(AppError::from)?,
            content: row.try_get("content").map_err(AppError::from)?,
            created_at: row.try_get("created_at").map_err(AppError::from)?,
        };
        response.push(message);
    }

    info!("Fetched {} messages for conversation {} (user {})",
          response.len(), conversation_id, user_id);

    Ok(response)
}

/// Cleanup (allineato con i nuovi metodi di state.rs)
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    state.cleanup_empty_channels(user_id).await;
}