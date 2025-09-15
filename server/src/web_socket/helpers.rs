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

/// Gestisce messaggi di chat con notifiche per nuove conversazioni
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

    let conversation_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ?",
    )
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
            let target_id: Option<String> = sqlx::query_scalar(
                "SELECT id FROM users WHERE username = ?"
            )
                .bind(target_user)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::from)?;

            match target_id {
                Some(id_str) => {
                    let target_user_id = Uuid::parse_str(&id_str)
                        .map_err(|_| AppError::BadRequest("Invalid target user ID".into()))?;

                    // Crea la conversazione DM
                    sqlx::query(
                        "INSERT INTO conversations(id, kind, title, owner_id, created_at)
                         VALUES(?, 'dm', NULL, ?, strftime('%s','now'))",
                    )
                        .bind(&conversation_id_str)
                        .bind(&user_id_str)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::from)?;

                    // Aggiungi entrambi i partecipanti
                    sqlx::query(
                        "INSERT INTO participants(conversation_id, user_id, role)
                         VALUES(?, ?, 'member')",
                    )
                        .bind(&conversation_id_str)
                        .bind(&user_id_str)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::from)?;

                    let target_id_str = target_user_id.to_string();
                    sqlx::query(
                        "INSERT INTO participants(conversation_id, user_id, role)
                         VALUES(?, ?, 'member')",
                    )
                        .bind(&conversation_id_str)
                        .bind(&target_id_str)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::from)?;

                    // Prepara lista partecipanti per le notifiche
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
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"#,
        )
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

    sqlx::query(
        r#"INSERT INTO messages (id, conversation_id, author_id, content, created_at)
           VALUES (?, ?, ?, ?, ?)"#,
    )
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
        // Per nuove conversazioni: solo notifiche (no broadcast)
        info!("New conversation created - sending notifications to participants");

        state.notify_conversation_created(
            conversation_id,
            &participant_ids,
            user_id,
            "dm",
            None
        ).await;

        info!("Message {} stored in new conversation - will be fetched by clients", id);
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

/// NUOVO: Gestisce notifiche dal canale utente (conversation_created)
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
            let conversation_id_str = notification
                .get("conversation_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::BadRequest("missing conversation_id in notification".into()))?;

            let conversation_id = Uuid::parse_str(conversation_id_str)
                .map_err(|_| AppError::BadRequest("invalid conversation_id in notification".into()))?;

            let creator_id = notification
                .get("creator_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            info!("Processing conversation_created notification for user {} - conversation {}",
                  user_id, conversation_id);

            // Invia notifica al client della nuova conversazione
            let client_notification = json!({
                "type": "new_conversation_available",
                "conversation_id": conversation_id,
                "creator_id": notification.get("creator_id"),
                "kind": notification.get("kind"),
                "title": notification.get("title"),
                "timestamp": notification.get("timestamp")
            });

            if let Ok(txt) = serde_json::to_string(&client_notification) {
                if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                    warn!("Failed to send new conversation notification to user {}", user_id);
                }
            }

            // Triggera fetch automatico SOLO se l'utente NON è il creatore
            if let Some(creator) = creator_id {
                if user_id != creator {
                    send_fetch_event_for_conversation(
                        state,
                        user_id,
                        conversation_id,
                        "new_conversation",
                        out_tx
                    ).await;
                } else {
                    info!("Skipping fetch for conversation creator {} - they already have the message", user_id);
                }
            } else {
                // Se non riusciamo a determinare il creatore, invia fetch per sicurezza
                send_fetch_event_for_conversation(
                    state,
                    user_id,
                    conversation_id,
                    "new_conversation",
                    out_tx
                ).await;
            }

            Ok(())
        }
        _ => {
            warn!("Unknown user notification type: {}", notification_type);
            Ok(())
        }
    }
}

/// Invia fetch event per una conversazione specifica
async fn send_fetch_event_for_conversation(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
    reason: &str,
    out_tx: &mpsc::Sender<OutboundMsg>,
) {
    let conversation_id_str = conversation_id.to_string();

    // Conta messaggi nella conversazione
    let message_count: i64 = match sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = ?"
    )
        .bind(&conversation_id_str)
        .fetch_one(&state.pool)
        .await {
        Ok(count) => count,
        Err(e) => {
            warn!("Failed to count messages for conversation {}: {}", conversation_id, e);
            return;
        }
    };

    // Invia fetch event solo se ci sono messaggi
    if message_count > 0 {
        let fetch_event = json!({
            "type": "fetch_conversation_messages",
            "conversation_id": conversation_id,
            "message_count": message_count,
            "reason": reason,
            "timestamp": Utc::now().timestamp()
        });

        if let Ok(txt) = serde_json::to_string(&fetch_event) {
            if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                warn!("Failed to send fetch event to user {}", user_id);
            } else {
                info!("Sent fetch event to user {} for conversation {} ({} messages, reason: {})",
                      user_id, conversation_id, message_count, reason);
            }
        }
    } else {
        debug!("No messages to fetch for conversation {} (user {}, reason: {})",
              conversation_id, user_id, reason);
    }
}

/// Invia fetch events automatici per tutte le conversazioni dell'utente al momento della connessione
pub async fn send_initial_fetch_events(
    state: &AppState,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    let user_id_str = user_id.to_string();

    // Ottieni tutte le conversazioni dell'utente con il conteggio messaggi
    let rows = sqlx::query(
        r#"SELECT DISTINCT p.conversation_id,
           (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = p.conversation_id) as message_count
           FROM participants p
           WHERE p.user_id = ?"#
    )
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
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
    )
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if count == 0 {
        return Err(AppError::Forbidden);
    }

    let limit = limit.unwrap_or(50).min(100);

    let messages = sqlx::query!(
        "SELECT m.id, m.author_id, u.username as author_username, m.content, m.created_at
         FROM messages m
         JOIN users u ON m.author_id = u.id
         WHERE m.conversation_id = ?
         ORDER BY m.created_at ASC
         LIMIT ?",
        conversation_id_str,
        limit
    )
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let response: Vec<MessageResponse> = messages
        .into_iter()
        .map(|row| MessageResponse {
            id: row.id.expect("Message ID should exist"),
            author_id: row.author_id,
            author_username: row.author_username,
            content: row.content,
            created_at: row.created_at,
        })
        .collect();

    info!("Fetched {} messages for conversation {} (user {})",
          response.len(), conversation_id, user_id);

    Ok(response)
}

/// Cleanup (allineato con i nuovi metodi di state.rs)
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    state.cleanup_empty_channels(user_id).await;
}