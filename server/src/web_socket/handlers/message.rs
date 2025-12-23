use chrono::Utc;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
    services::conversation_service::ConversationService,
    services::message_service::MessageService,
    repositories::conversation_repo::ConversationRepo,
};
use crate::services::participant_service::ParticipantService;
use crate::web_socket::actor::OutboundMsg;
use crate::web_socket::broadcast::{broadcast_to_conversation, send_message_confirmation};
use crate::web_socket::handlers::conversation::handle_message_with_new_conversation;
use crate::web_socket::utils::{batch_insert_user_events, extract_conversation_id};


pub async fn handle_chat_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    info!("HANDLING CHAT MESSAGE: {}", value);

    let has_temp_id = value.get("client_temp_id").is_some();
    let has_target = value.get("target_username").is_some();

    if has_temp_id && has_target {
        if let Some(temp_id) = value.get("client_temp_id").and_then(|v| v.as_str()) {
            if let Some(conversation_id) = state
                .conversation_confirmation_cache
                .get_by_temp_id(temp_id)
                .await
            {
                info!(
                    "Conversation for temp_id {} already exists: {}",
                    temp_id, conversation_id
                );
            } else {
                info!(
                    "First message for temp_id {}, creating conversation",
                    temp_id
                );
                return handle_message_with_new_conversation(state, value, user_id, username).await;
            }
        }
    }

    let conversation_id = extract_conversation_id(state, value).await?;
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("Message content cannot be empty".into()))?;

    let client_msg_id = value
        .get("client_msg_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let is_participant = ConversationRepo::is_participant(&state.pool, conversation_id, user_id).await?;
    if !is_participant {
        return Err(AppError::Forbidden);
    }

    let message_sequence = state.get_next_message_sequence(conversation_id).await?;

    let msg_id = Uuid::new_v4();
    let msg_id_str = msg_id.to_string();
    let ts = Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES (?, ?, ?, ?, ?, ?)"
    )
        .bind(&msg_id_str)
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .bind(content)
        .bind(ts)
        .bind(message_sequence as i64)
        .execute(&state.pool)
        .await
        .map_err(AppError::from)?;

    info!(
        "Message {} saved with sequence {}",
        msg_id, message_sequence
    );

    sqlx::query(
        "UPDATE participants
         SET last_read_sequence = ?
         WHERE conversation_id = ? AND user_id = ?"
    )
        .bind(message_sequence as i64)
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .execute(&state.pool)
        .await
        .map_err(AppError::from)?;

    debug!(
        "Auto-marked message seq {} as read for author {}",
        message_sequence, user_id
    );

    if let Some(ref client_id) = client_msg_id {
        state
            .message_confirmation_cache
            .insert(msg_id, client_id.clone())
            .await;
        info!("Cached client_msg_id {} for message {}", client_id, msg_id);
    }

    send_message_confirmation(
        state,
        msg_id,
        conversation_id,
        message_sequence,
        client_msg_id.clone(),
        ts,
        user_id,
    )
        .await?;

    let participants = ConversationService::list_participant_ids(&state.pool, conversation_id).await?;

    info!(
        "Creating user events for {} participants in conversation {}",
        participants.len(),
        conversation_id
    );

    if participants.len() > 1 {
        let recipients: Vec<Uuid> = participants
            .into_iter()
            .filter(|&p| p != user_id)
            .collect();

        if !recipients.is_empty() {
            // Salva notification LEGGERA nel DB (per recovery offline users)
            let notification_payload = json!({
                "message_id": msg_id,
                "message_sequence": message_sequence,
                "conversation_id": conversation_id,
                "timestamp": ts
            });

            match batch_insert_user_events(
                &state.pool,
                &recipients,
                "new_message_notification",
                &notification_payload,
                Some(conversation_id),
            ).await {
                Ok(user_sequences) => {
                    info!(
                        "Batch inserted {} lightweight notifications for message {}",
                        user_sequences.len(),
                        msg_id
                    );

                    // Costruisci messaggio COMPLETO (hai già tutti i dati qui!)
                    let mut message_data = json!({
                        "id": msg_id,
                        "conversation_id": conversation_id,
                        "author_id": user_id,
                        "author_username": username,
                        "content": content,
                        "created_at": ts,
                        "sequence_num": message_sequence,
                    });

                    if let Some(ref client_id) = client_msg_id {
                        message_data["client_msg_id"] = json!(client_id);
                    }

                    let full_event = json!({
                        "type": "new_message",
                        "conversation_id": conversation_id,
                        "conversation_sequence": message_sequence,
                        "message": message_data
                    });

                    // Invia messaggio COMPLETO agli utenti online (via user_notification_channel)
                    let channels = state.user_notification_channels.read().await;
                    for (recipient_id, user_seq) in recipients.iter().zip(&user_sequences) {
                        if let Some(tx) = channels.get(recipient_id) {
                            let mut event = full_event.clone();
                            event["sequence"] = json!(user_seq);
                            let _ = tx.send(event);  // ← Messaggio completo, non notification!
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to batch insert user_events: {}", e);
                }
            }
        }
    }

    // Broadcast alla conversazione
    let mut broadcast_msg = json!({
        "type": "chat_message",
        "id": msg_id,
        "conversation_id": conversation_id,
        "author_id": user_id,
        "author_username": username,
        "content": content,
        "created_at": ts,
        "sequence": message_sequence
    });

    if let Some(ref client_id) = client_msg_id {
        broadcast_msg["client_msg_id"] = json!(client_id);
    }

    match broadcast_to_conversation(state, conversation_id, broadcast_msg).await {
        Ok(delivered) if delivered > 0 => {
            info!("Message {} delivered to {} receivers", msg_id, delivered);
        }
        Ok(_) => {
            info!("Message {} stored for future delivery", msg_id);
        }
        Err(e) => {
            warn!("Failed to broadcast message {}: {}", msg_id, e);
        }
    }

    Ok(())
}



pub async fn handle_mark_read(
    state: &AppState,
    user_id: Uuid,
    msg: &serde_json::Value,
) -> Result<()> {
    let conversation_id_str = msg
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing conversation_id".into()))?;

    let conversation_id = Uuid::parse_str(conversation_id_str)
        .map_err(|_| AppError::BadRequest("Invalid conversation_id format".into()))?;

    let sequence_num = msg
        .get("sequence_num")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::BadRequest("Missing or invalid sequence_num".into()))?;

    let is_participant = ConversationRepo::is_participant(&state.pool, conversation_id, user_id).await?;

    if !is_participant {
        warn!(
            "User {} attempted to mark_read conversation {} (not a participant)",
            user_id, conversation_id
        );
        return Ok(());
    }

    ParticipantService::mark_read(&state.pool, conversation_id, user_id, sequence_num).await?;

    info!(
        "User {} marked conversation {} as read up to sequence {}",
        user_id, conversation_id, sequence_num
    );

    Ok(())
}

pub async fn handle_delete_message(
    state: &AppState,
    value: &serde_json::Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> crate::error::Result<()> {
    let message_id_str = value
        .get("mid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("Missing message_id (mid)".into()))?;

    let message_id = Uuid::parse_str(message_id_str)
        .map_err(|_| crate::error::AppError::BadRequest("Invalid message_id format".into()))?;

    info!("Processing delete request for message {} from user {}", message_id, user_id);

    let conversation_id = MessageService::delete_message(&state.pool, message_id, user_id).await?;

    info!("Message {} in conversation {} deleted successfully from DB", message_id, conversation_id);

    let participants = ConversationService::list_participant_ids(&state.pool, conversation_id).await?;

    ConversationService::broadcast_message_deleted(
        state,
        conversation_id,
        message_id,
        participants,
    ).await;

    let ack = json!({
        "type": "delete_message_ack",
        "message_id": message_id,
        "status": "ok"
    });
    if let Ok(txt) = serde_json::to_string(&ack) {
        if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
            warn!("Failed to send delete_message_ack to user {}", user_id);
        }
    }

    Ok(())
}