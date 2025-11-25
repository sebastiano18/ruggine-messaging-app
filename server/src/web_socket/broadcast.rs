use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
};
use super::utils::NewConversationData;
use super::actor::OutboundMsg;

/// Crea e salva un messaggio di sistema nel database
/// Usa l'owner della conversazione come author_id (per rispettare il constraint FK)
pub async fn create_system_message(
    state: &AppState,
    conversation_id: Uuid,
    content: String,
) -> Result<(Uuid, u64)> {
    let message_id = Uuid::new_v4();
    let timestamp = Utc::now().timestamp();

    // Ottieni l'owner della conversazione per usarlo come author_id
    let owner_id: String = sqlx::query_scalar(
        "SELECT owner_id FROM conversations WHERE id = ?"
    )
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await?;

    // Ottieni sequence number per il messaggio
    let sequence = state.get_next_message_sequence(conversation_id).await?;

    // Salva nel database usando l'owner come author_id
    // Il client riconoscerà comunque come messaggio di sistema dal contenuto speciale
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num)
         VALUES (?, ?, ?, ?, ?, ?)"
    )
        .bind(message_id.to_string())
        .bind(conversation_id.to_string())
        .bind(&owner_id)
        .bind(&content)
        .bind(timestamp)
        .bind(sequence as i64)
        .execute(&state.pool)
        .await?;

    info!("Created system message: {} in conversation {}", content, conversation_id);

    Ok((message_id, sequence))
}

/// Broadcast di un messaggio a tutti i partecipanti di una conversazione
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

/// Invia conferma di un messaggio al mittente
pub async fn send_message_confirmation(
    state: &AppState,
    msg_id: Uuid,
    conversation_id: Uuid,
    sequence: u64,
    client_msg_id: Option<String>,
    created_at: i64,
    user_id: Uuid,
) -> Result<()> {
    let confirmation = json!({
        "type": "message_confirmation",
        "client_msg_id": client_msg_id,
        "server_msg_id": msg_id.to_string(),
        "conversation_id": conversation_id,
        "sequence": sequence,
        "created_at": created_at,
        "status": "saved"
    });

    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    match user_tx.send(confirmation) {
        Ok(receiver_count) => {
            debug!(
                "Sent message confirmation to user {} ({} receivers)",
                user_id, receiver_count
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                "Failed to send message confirmation to user {}: {}",
                user_id, e
            );
            Err(AppError::Internal(format!(
                "Failed to send confirmation: {}",
                e
            )))
        }
    }
}

/// Invia conferma di creazione conversazione (per duplicati)
pub async fn send_conversation_confirmation(
    state: &AppState,
    conversation_id: Uuid,
    client_temp_id: String,
    user_id: Uuid,
) -> Result<()> {
    // Recupera i dettagli della conversazione dal database
    let conv_row = sqlx::query("SELECT kind, owner_id, created_at, title FROM conversations WHERE id = ?")
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    let kind: String = conv_row.try_get("kind").map_err(AppError::from)?;
    let owner_id: String = conv_row.try_get("owner_id").map_err(AppError::from)?;
    let created_at: i64 = conv_row.try_get("created_at").map_err(AppError::from)?;
    let title: Option<String> = conv_row.try_get("title").ok();

    // Per DM, il display_title è l'username dell'altro partecipante
    let display_title = if kind == "dm" {
        // Query per ottenere l'username dell'altro partecipante
        let other_username: Option<String> = sqlx::query_scalar(
            "SELECT u.username
             FROM participants p
             JOIN users u ON p.user_id = u.id
             WHERE p.conversation_id = ? AND p.user_id != ?
             LIMIT 1"
        )
            .bind(conversation_id.to_string())
            .bind(user_id.to_string())
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();

        other_username.unwrap_or_else(|| "Unknown".to_string())
    } else {
        // Per i gruppi, usa il title della conversazione
        title.unwrap_or_else(|| "Group".to_string())
    };

    // Recupera last_read_sequence del partecipante
    let last_read_sequence: i64 = sqlx::query_scalar(
        "SELECT last_read_sequence FROM participants WHERE conversation_id = ? AND user_id = ?"
    )
        .bind(conversation_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(0);

    // Recupera solo i campi necessari dell'ultimo messaggio
    let last_message_data = sqlx::query(
        "SELECT id, author_id, author_username, content, created_at, sequence_num
         FROM messages
         WHERE conversation_id = ?
         ORDER BY sequence_num DESC
         LIMIT 1"
    )
        .bind(conversation_id.to_string())
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();

    let last_msg_seq = last_message_data.as_ref()
        .and_then(|row| row.try_get::<i64, _>("sequence_num").ok())
        .unwrap_or(0);

    let mut conversation_obj = json!({
        "id": conversation_id,
        "client_temp_id": client_temp_id,
        "kind": kind,
        "owner_id": owner_id,
        "created_at": created_at,
        "display_title": display_title,
        "last_read_sequence": last_read_sequence,
        "last_msg_seq": last_msg_seq,
        "status": "already_exists"
    });

    // Aggiungi last_message se presente
    if let Some(row) = last_message_data {
        if let (Ok(id), Ok(author_id), Ok(author_username), Ok(content), Ok(created_at)) = (
            row.try_get::<String, _>("id"),
            row.try_get::<String, _>("author_id"),
            row.try_get::<String, _>("author_username"),
            row.try_get::<String, _>("content"),
            row.try_get::<i64, _>("created_at"),
        ) {
            conversation_obj["last_message"] = json!({
                "id": id,
                "author_id": author_id,
                "author_username": author_username,
                "content": content,
                "created_at": created_at,
                "sequence_num": row.try_get::<Option<i64>, _>("sequence_num").ok().flatten(),
            });
        }
    }

    let confirmation = json!({
        "type": "conversation_confirmation",
        "conversation": conversation_obj
    });

    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    match user_tx.send(confirmation) {
        Ok(_) => {
            info!(
                "Sent conversation confirmation to user {} for conversation {}",
                user_id, conversation_id
            );
            Ok(())
        }
        Err(e) => {
            warn!("Failed to send conversation confirmation: {}", e);
            Err(AppError::Internal(format!(
                "Failed to send confirmation: {}",
                e
            )))
        }
    }
}

/// Invia eventi di creazione conversazione a tutti i partecipanti
pub async fn send_conversation_created_events(
    state: &AppState,
    data: NewConversationData,
) -> Result<()> {
    let participants = vec![
        (
            data.creator_id,
            data.creator_username.clone(),
            data.other_participant_username.clone(),
        ),
        (
            data.other_participant_id,
            data.other_participant_username.clone(),
            data.creator_username.clone(),
        ),
    ];

    for (participant_id, _participant_username, other_username) in participants {
        let mut event = json!({
            "type": "conversation_created_complete",
            "conversation": {
                "id": data.conversation_id,
                "kind": "dm",
                "title": null,
                "display_title": other_username,
                "owner_id": data.creator_id,
                "created_at": data.created_at,
                "message_count": if data.initial_message_id != Uuid::nil() { 1 } else { 0 },
                "participants": [
                    {
                        "user_id": data.creator_id.to_string(),
                        "username": data.creator_username.clone(),
                        "role": "member"
                    },
                    {
                        "user_id": data.other_participant_id.to_string(),
                        "username": data.other_participant_username.clone(),
                        "role": "member"
                    }
                ]
            }
        });

        // Aggiungi last_message se c'è un messaggio iniziale
        if data.initial_message_id != Uuid::nil() {
            let mut last_msg = json!({
                "id": data.initial_message_id,
                "author_id": data.creator_id,
                "author_username": data.creator_username.clone(),
                "content": data.initial_message_content.clone(),
                "created_at": data.created_at,
                "sequence_num": data.initial_message_sequence
            });

            if let Some(ref client_id) = data.client_msg_id {
                last_msg["client_msg_id"] = json!(client_id);
            }

            event["conversation"]["last_message"] = last_msg;
        }

        // Per il creatore, aggiungi client_temp_id e usa tipo diverso
        let event_type = if participant_id == data.creator_id {
            if let Some(ref temp_id) = data.client_temp_id {
                event["conversation"]["client_temp_id"] = json!(temp_id);
                event["type"] = json!("conversation_confirmation");
                "conversation_confirmation"
            } else {
                "conversation_created_complete"
            }
        } else {
            "conversation_created_complete"
        };

        // Invia evento sequenziato
        match state
            .send_sequenced_event_to_user(
                participant_id,
                event_type,
                event,
                Some(data.conversation_id),
            )
            .await
        {
            Ok(seq) => {
                info!(
                    "Sent {} (seq={}) to user {}",
                    event_type, seq, participant_id
                );
            }
            Err(e) => {
                warn!(
                    "Failed to send {} to user {}: {}",
                    event_type, participant_id, e
                );
            }
        }
    }

    Ok(())
}

/// Configura l'iscrizione dell'utente al broadcast channel della conversazione
pub async fn setup_conversation_subscription(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<()> {
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    let is_participant: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?",
    )
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if is_participant == 0 {
        return Err(AppError::Forbidden);
    }

    let conv_tx = state.get_or_create_broadcast_tx(conversation_id).await;
    let receiver_count = conv_tx.receiver_count();

    info!(
        "Conversation {} broadcast channel ready for user {} ({} current receivers)",
        conversation_id, user_id, receiver_count
    );

    Ok(())
}

/// Cleanup canali vuoti quando un utente si disconnette
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    state.cleanup_empty_channels(user_id).await;
}

/// Gestisce notifiche utente e setup subscription conversazioni
pub async fn handle_user_notification(
    state: &AppState,
    notification: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<Option<Uuid>> {
    let notification_type = notification
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match notification_type {
        "conversation_created_complete" | "conversation_confirmation" => {
            let conversation_id = notification
                .get("conversation")
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| AppError::BadRequest("invalid conversation_id".into()))?;

            info!(
                "Processing {} for user {} - conversation {}",
                notification_type, user_id, conversation_id
            );

            // Setup subscription al broadcast channel
            if let Err(e) = setup_conversation_subscription(state, user_id, conversation_id).await {
                warn!(
                    "Failed to setup conversation subscription for user {}: {}",
                    user_id, e
                );
            }

            // Forward dell'evento al client
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx
                    .send(OutboundMsg::Text(notification_txt))
                    .await
                    .is_err()
                {
                    warn!("Failed to send {} to user {}", notification_type, user_id);
                } else {
                    info!("Forwarded {} to user {}", notification_type, user_id);
                }
            }

            // Ritorna il conversation_id per permettere setup aggiuntivo (es. stream manager)
            Ok(Some(conversation_id))
        }
        "conversation_deleted" => {
            // Forward dell'evento al client
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx
                    .send(OutboundMsg::Text(notification_txt))
                    .await
                    .is_err()
                {
                    warn!("Failed to send {} to user {}", notification_type, user_id);
                } else {
                    debug!("Forwarded {} to user {}", notification_type, user_id);
                }
            }
            Ok(None)
        }
        _ => {
            // Forward altre notifiche
            if notification.get("sequence").is_some() {
                if let Ok(notification_txt) = serde_json::to_string(&notification) {
                    if out_tx
                        .send(OutboundMsg::Text(notification_txt))
                        .await
                        .is_err()
                    {
                        warn!("Failed to send sequenced event to user {}", user_id);
                    } else {
                        debug!("Forwarded sequenced event to user {}", user_id);
                    }
                }
            } else {
                debug!(
                    "Received notification type '{}' for user {}",
                    notification_type, user_id
                );
                if let Ok(txt) = serde_json::to_string(&notification) {
                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                }
            }
            Ok(None)
        }
    }
}