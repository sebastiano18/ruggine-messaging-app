use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
    services::conversation_service::ConversationService,
    services::message_service::MessageService,
};
use crate::web_socket::actor::OutboundMsg;
use crate::web_socket::broadcast::{broadcast_to_conversation, send_message_confirmation};
use crate::web_socket::handlers::conversation::handle_message_with_new_conversation;
use crate::web_socket::utils::{extract_conversation_id, get_conversation_participants, verify_participant};

/// Router principale per gestire i messaggi in arrivo

/// Handlers for message operations (send, mark read, delete)

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

    let is_participant = verify_participant(state, conversation_id, user_id).await?;
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

    let participants = get_conversation_participants(state, conversation_id).await?;

    info!(
        "Creating user events for {} participants in conversation {}",
        participants.len(),
        conversation_id
    );

    let mut message_event_data = json!({
        "id": msg_id,
        "conversation_id": conversation_id,
        "author_id": user_id,
        "author_username": username,
        "content": content,
        "created_at": ts,
        "conversation_sequence": message_sequence,
    });

    if let Some(ref client_id) = client_msg_id {
        message_event_data["client_msg_id"] = json!(client_id);
    }

    for participant_id in participants {
        let event_payload = json!({
            "type": "new_message",
            "conversation_id": conversation_id,
            "conversation_sequence": message_sequence,
            "message": message_event_data.clone()
        });

        match state
            .send_sequenced_event_to_user(
                participant_id,
                "new_message",
                event_payload,
                Some(conversation_id),
            )
            .await
        {
            Ok(user_seq) => {
                debug!(
                    "Event seq={} created for user {} - msg {} conv {}",
                    user_seq, participant_id, msg_id, conversation_id
                );
            }
            Err(e) => {
                warn!("Failed to create event for user {}: {}", participant_id, e);
            }
        }
    }

    info!("User events created for all participants");

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
    // 1. Estrai conversation_id dal messaggio
    let conversation_id_str = msg
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing conversation_id".into()))?;

    let conversation_id = Uuid::parse_str(conversation_id_str)
        .map_err(|_| AppError::BadRequest("Invalid conversation_id format".into()))?;

    // 2. Estrai sequence_num dal messaggio
    let sequence_num = msg
        .get("sequence_num")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::BadRequest("Missing or invalid sequence_num".into()))?;

    let user_id_str = user_id.to_string();
    let conv_id_str = conversation_id.to_string();

    // 3. Valida che l'utente sia partecipante (security check)
    let is_participant: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM participants WHERE conversation_id = ? AND user_id = ?)"
    )
        .bind(&conv_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(false);

    if !is_participant {
        warn!(
            "User {} attempted to mark_read conversation {} (not a participant)",
            user_id, conversation_id
        );
        return Ok(()); // Ignora silenziosamente per sicurezza
    }

    // 4. Aggiorna last_read_sequence nel database
    sqlx::query(
        r#"
        UPDATE participants
        SET last_read_sequence = MAX(last_read_sequence, ?)
        WHERE conversation_id = ? AND user_id = ?
        "#
    )
        .bind(sequence_num)
        .bind(&conv_id_str)
        .bind(&user_id_str)
        .execute(&state.pool)
        .await?;

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
    use crate::services::message_service::MessageService;

    // 1. Extract message_id (mid)
    let message_id_str = value
        .get("mid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("Missing message_id (mid)".into()))?;

    let message_id = Uuid::parse_str(message_id_str)
        .map_err(|_| crate::error::AppError::BadRequest("Invalid message_id format".into()))?;

    info!("Processing delete request for message {} from user {}", message_id, user_id);

    // 2. Call the service to delete the message. This also performs authorization.
    // The service returns the conversation_id on success.
    let conversation_id = MessageService::delete_message(&state.pool, message_id, user_id).await?;

    info!("Message {} in conversation {} deleted successfully from DB", message_id, conversation_id);

    // 3. Get all participants of the conversation to notify them.
    let participants = ConversationService::list_participant_ids(&state.pool, conversation_id).await?;

    // 4. Broadcast the deletion event to all participants.
    ConversationService::broadcast_message_deleted(
        state,
        conversation_id,
        message_id,
        participants,
    ).await;

    // 5. (Optional) Send an ACK to the original sender
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