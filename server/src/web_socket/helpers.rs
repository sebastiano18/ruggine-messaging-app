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


/// Gestisce messaggi di chat con evento completo per nuove conversazioni
pub async fn handle_chat_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    // AGGIUNGI QUESTO LOG PER VEDERE IL JSON COMPLETO
    info!("RECEIVED CHAT MESSAGE JSON: {}", value);

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

    // NUOVO: Estrai client_msg_id se presente
    let client_msg_id = value
        .get("client_msg_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // AGGIUNGI QUESTO LOG
    info!("EXTRACTED client_msg_id: {:?} from message", client_msg_id);

    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    // Variabili per salvare dati prima del commit
    let mut is_new_conversation = false;
    let mut participant_ids = Vec::new();
    let mut other_participant_username: Option<String> = None;
    let mut other_participant_id: Option<Uuid> = None;

    // Inizia una transazione per consistenza
    let mut tx = state.pool.begin().await.map_err(AppError::from)?;

    let conversation_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
        .bind(&conversation_id_str)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::from)?;

    if conversation_exists == 0 {
        info!("Creating new DM conversation {} for first message from user {}", conversation_id, user_id);
        is_new_conversation = true;

        if let Some(ref target_user) = target_username {
            let target_row = sqlx::query("SELECT id, username FROM users WHERE username = ?")
                .bind(target_user)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?;

            match target_row {
                Some(row) => {
                    let id_str: String = row.try_get("id").map_err(AppError::from)?;
                    let target_username_actual: String = row.try_get("username").map_err(AppError::from)?;
                    let target_user_id = Uuid::parse_str(&id_str)
                        .map_err(|_| AppError::BadRequest("Invalid target user ID".into()))?;

                    other_participant_username = Some(target_username_actual.clone());
                    other_participant_id = Some(target_user_id);

                    sqlx::query("INSERT INTO conversations(id, kind, title, owner_id, created_at) VALUES(?, 'dm', NULL, ?, strftime('%s','now'))")
                        .bind(&conversation_id_str)
                        .bind(&user_id_str)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::from)?;

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

                    info!("Created DM conversation {} between {} and {}",
                          conversation_id, username, target_username_actual);
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

    // NUOVO: Salva client_msg_id in cache se presente
    if let Some(ref client_id) = client_msg_id {
        state.message_confirmation_cache.insert(id, client_id.clone()).await;
        info!("SUCCESSFULLY CACHED client_msg_id {} for server message {}", client_id, id);
    } else {
        info!("NO client_msg_id to cache for message {}", id);
    }

    // MODIFICATO: Invia sempre conferma al mittente con ID e sequenza
    {
        
         let confirmation = json!({
             "type": "message_confirmation",
             "client_msg_id": client_msg_id,
             "server_msg_id": id.to_string(),
             "conversation_id": conversation_id,
             "sequence": message_sequence,
             "created_at": ts,
             "status": "saved"
         });
 
         // Invia direttamente al canale utente del mittente
         let user_tx = state.get_or_create_user_notification_channel(user_id).await;
         match user_tx.send(confirmation) {
             Ok(receiver_count) => {
                 debug!("Sent message confirmation to sender {} ({} receivers)", user_id, receiver_count);
             }
             Err(e) => {
                 warn!("Failed to send confirmation to sender {}: {}", user_id, e);
             }
         }
        
    }

    // Gestione diversa per nuove conversazioni vs esistenti
    if is_new_conversation {
        info!("New DM conversation created - sending complete data via WebSocket");

        let complete_event = json!({
            "type": "conversation_created_complete",
            "conversation": {
                "id": conversation_id,
                "kind": "dm",
                "title": null,
                "display_title": "",
                "owner_id": user_id,
                "created_at": ts,
                "message_count": 1,
                "participants": [
                    {
                        "user_id": user_id.to_string(),
                        "username": username,
                        "role": "member"
                    },
                    {
                        "user_id": other_participant_id.unwrap().to_string(),
                        "username": other_participant_username.clone().unwrap(),
                        "role": "member"
                    }
                ],
                "last_message": {
                    "id": id,
                    "author_id": user_id,
                    "author_username": username,
                    "content": content,
                    "created_at": ts,
                    "sequence_num": message_sequence,
                    // NUOVO: Include client_msg_id nel last_message se presente
                    "client_msg_id": client_msg_id.clone()
                }
            }
        });

        for &participant_id in &participant_ids {
            let mut personalized_event = complete_event.clone();

            if participant_id == user_id {
                personalized_event["conversation"]["display_title"] =
                    json!(other_participant_username.clone().unwrap());
            } else {
                personalized_event["conversation"]["display_title"] = json!(username);
            }

            match state.send_sequenced_event_to_user(
                participant_id,
                "conversation_created_complete",
                personalized_event,
                Some(conversation_id)
            ).await {
                Ok(seq) => {
                    info!("Sent complete conversation data (seq={}) to user {}", seq, participant_id);
                }
                Err(e) => {
                    warn!("Failed to send complete conversation to user {}: {}", participant_id, e);
                }
            }
        }

        info!("New conversation {} created with complete data sent to all participants", conversation_id);

    } else {
        // Per conversazioni esistenti: broadcast normale del messaggio
        let mut event = json!({
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

        // NUOVO: Include client_msg_id nel broadcast se presente
        if let Some(ref client_id) = client_msg_id {
            event["client_msg_id"] = json!(client_id);
        }

        match broadcast_to_conversation(state, conversation_id, event).await {
            Ok(delivered) if delivered > 0 => {
                info!("Message {} (seq={}) delivered to {} active receivers",
                      id, message_sequence, delivered);
            }
            Ok(_) => {
                info!("Message {} (seq={}) stored for future delivery", id, message_sequence);
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
        "conversation_created_complete" => {
            // L'evento contiene già tutti i dati della conversazione
            let conversation_id = notification
                .get("conversation")
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| AppError::BadRequest("invalid conversation_id".into()))?;

            info!("Processing complete conversation data for user {} - conversation {}",
                  user_id, conversation_id);

            // Setup subscription al broadcast channel della conversazione per messaggi futuri
            if let Err(e) = setup_conversation_subscription(state, user_id, conversation_id).await {
                warn!("Failed to setup conversation subscription for user {}: {}", user_id, e);
            }

            // Forward dell'evento completo al client - contiene già tutto ciò che serve
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx.send(OutboundMsg::Text(notification_txt)).await.is_err() {
                    warn!("Failed to send conversation_created_complete to user {}", user_id);
                } else {
                    info!("Forwarded complete conversation data to user {}", user_id);
                }
            }

            Ok(())
        }
        _ => {
            // Altri tipi di notifiche (eventi sequenziati, etc.)
            if notification.get("sequence").is_some() {
                if let Ok(notification_txt) = serde_json::to_string(&notification) {
                    if out_tx.send(OutboundMsg::Text(notification_txt)).await.is_err() {
                        warn!("Failed to send sequenced event to user {}", user_id);
                    } else {
                        debug!("Forwarded sequenced event to user {}", user_id);
                    }
                }
            } else {
                debug!("Received notification type '{}' for user {}", notification_type, user_id);
                // Forward altre notifiche generiche
                if let Ok(txt) = serde_json::to_string(&notification) {
                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                }
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
    let is_participant: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
    )
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if is_participant == 0 {
        return Err(AppError::Forbidden);
    }

    // Ottieni/crea il broadcast channel per messaggi futuri
    let conv_tx = state.get_or_create_broadcast_tx(conversation_id).await;
    let receiver_count = conv_tx.receiver_count();

    info!("Conversation {} broadcast channel ready for user {} ({} current receivers)",
          conversation_id, user_id, receiver_count);

    Ok(())
}

/// Cleanup canali vuoti quando un utente si disconnette
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    state.cleanup_empty_channels(user_id).await;
}