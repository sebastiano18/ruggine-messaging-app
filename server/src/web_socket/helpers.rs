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
    let mut other_participant_username: Option<String> = None;

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

                    // Crea la conversazione DM (senza titolo nel DB)
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
                    other_participant_username = Some(target_user.clone());

                    info!("Created DM conversation {} between {} and {}",
                          conversation_id, username, target_user);
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

    // Ottieni la sequenza per il messaggio
    let message_sequence = {
        let now = Utc::now().timestamp();

        sqlx::query("INSERT OR IGNORE INTO message_sequences (conversation_id, current_sequence, last_updated) VALUES (?, 0, ?)")
            .bind(&conversation_id_str)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        let row = sqlx::query("UPDATE message_sequences SET current_sequence = current_sequence + 1, last_updated = ? WHERE conversation_id = ? RETURNING current_sequence")
            .bind(now)
            .bind(&conversation_id_str)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::from)?;

        let seq: i64 = row.try_get("current_sequence").map_err(AppError::from)?;
        seq as u64
    };

    // Salvataggio del messaggio nel database con sequenza
    let id = Uuid::new_v4();
    let ts = Utc::now().timestamp();
    let id_str = id.to_string();

    sqlx::query("INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(&id_str)
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .bind(content)
        .bind(ts)
        .bind(message_sequence as i64)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

    // Commit della transazione
    tx.commit().await.map_err(AppError::from)?;

    info!("Message {} saved to DB with sequence {}", id, message_sequence);

    // Gestione diversa per nuove conversazioni vs esistenti
    if is_new_conversation {
        info!("New DM conversation created - sending notifications");

        // Prepara l'evento con i nomi utente corretti
        let event_data = json!({
            "type": "conversation_created",
            "conversation_id": conversation_id,
            "creator_id": user_id,
            "creator_username": username,
            "kind": "dm",
            "title": null,  // Il titolo resta null nel DB
            "other_participant": other_participant_username,  // Nome dell'altro partecipante
            "display_title": other_participant_username.clone().unwrap_or_else(|| "Direct Message".to_string()),
            "first_message": {
                "id": id,
                "author_id": user_id,
                "author_username": username,
                "content": content,
                "created_at": ts,
                "sequence": message_sequence
            }
        });

        // Invia evento sequenziato ai partecipanti
        for &participant_id in &participant_ids {
            if participant_id != user_id {  // Solo al ricevente
                // L'evento per il ricevente deve mostrare il nome del creatore come titolo
                let mut recipient_event = event_data.clone();
                recipient_event["display_title"] = json!(username);  // Il ricevente vede il nome del creatore

                match state.send_sequenced_event_to_user(
                    participant_id,
                    "conversation_created",
                    recipient_event,
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

        // Broadcast del messaggio per tutti i partecipanti
        let message_event = json!({
            "type": "chat_message",
            "id": id,
            "cid": conversation_id,
            "conversation_id": conversation_id,
            "author_id": user_id,
            "author_username": username,
            "content": content,
            "created_at": ts,
            "sequence": message_sequence
        });

        match broadcast_to_conversation(state, conversation_id, message_event).await {
            Ok(delivered) => {
                debug!("Message {} broadcast to {} receivers", id, delivered);
            }
            Err(e) => {
                debug!("Failed to broadcast new conversation message: {}", e);
            }
        }

        info!("Message {} stored in new conversation with sequence {}", id, message_sequence);
    } else {
        // Per conversazioni esistenti: broadcast normale con sequenza
        let event = json!({
            "type": "chat_message",
            "id": id,
            "cid": conversation_id,
            "conversation_id": conversation_id,
            "author_id": user_id,
            "author_username": username,
            "content": content,
            "created_at": ts,
            "sequence": message_sequence
        });

        match broadcast_to_conversation(state, conversation_id, event).await {
            Ok(delivered) if delivered > 0 => {
                info!("Message {} (seq={}) delivered immediately to {} receivers",
                      id, message_sequence, delivered);
            }
            Ok(_) => {
                info!("Message {} (seq={}) stored - no active receivers", id, message_sequence);
            }
            Err(e) => {
                warn!("Failed to broadcast message {} (seq={}): {}", id, message_sequence, e);
            }
        }
    }

    Ok(())
}

/// Gestisce notifiche dal canale utente
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

            info!("Processing conversation_created notification for user {} - conversation {}",
                  user_id, conversation_id);

            // Setup subscription alla conversazione
            if let Err(e) = setup_conversation_subscription(state, user_id, conversation_id).await {
                warn!("Failed to setup conversation subscription for user {}: {}", user_id, e);
            }

            // Forward dell'evento al client con il display_title corretto
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
            // Altri tipi di notifiche
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

    // Verifica che l'utente sia partecipante
    let is_participant: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?")
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if is_participant == 0 {
        return Err(AppError::Forbidden);
    }

    // Ottieni/crea il broadcast channel
    let conv_tx = state.get_or_create_broadcast_tx(conversation_id).await;
    let receiver_count = conv_tx.receiver_count();

    info!("Conversation {} broadcast channel ready for user {} ({} current receivers)",
          conversation_id, user_id, receiver_count);

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
    pub sequence: Option<i64>,
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

    let messages = sqlx::query(
        "SELECT m.id, m.author_id, u.username as author_username, m.content, m.created_at, m.sequence_num
         FROM messages m
         JOIN users u ON m.author_id = u.id
         WHERE m.conversation_id = ?
         ORDER BY m.created_at ASC
         LIMIT ?"
    )
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
            sequence: row.try_get("sequence_num").ok(),
        };
        response.push(message);
    }

    info!("Fetched {} messages for conversation {} (user {})",
          response.len(), conversation_id, user_id);

    Ok(response)
}

/// Cleanup
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    state.cleanup_empty_channels(user_id).await;
}