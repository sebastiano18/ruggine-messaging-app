use crate::services::conversation_service::ConversationService;
use axum::extract::ws::{Message, WebSocket};
use futures::{StreamExt, stream::SplitStream};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{
    actor::OutboundMsg,
    helpers::{
        handle_chat_message, handle_create_conversation, handle_incoming_message,
        handle_user_events_resume_request,
    },
};
use crate::state::AppState;

/// Gestisce il messaggio mark_read dal client
async fn handle_mark_read(
    state: &AppState,
    user_id: Uuid,
    msg: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Estrai conversation_id dal messaggio
    let conversation_id_str = msg
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing conversation_id")?;

    let conversation_id = Uuid::parse_str(conversation_id_str)
        .map_err(|_| "Invalid conversation_id format")?;

    // 2. Estrai sequence_num dal messaggio
    let sequence_num = msg
        .get("sequence_num")
        .and_then(|v| v.as_i64())
        .ok_or("Missing or invalid sequence_num")?;

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

pub fn spawn_reader(
    mut ws_rx: SplitStream<WebSocket>,
    state: AppState,
    user_id: Uuid,
    username: String,
    out_tx: mpsc::Sender<OutboundMsg>,
    stop_tx: watch::Sender<bool>,
    stop_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut message_count = 0u32;
        let mut window_start = Instant::now();
        const MAX_MESSAGES_PER_MINUTE: u32 = 60;
        const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

        let mut total_messages_processed = 0u64;
        let mut last_heartbeat = Instant::now();
        const CLIENT_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(120);

        info!("Reader task started for user {}", user_id);

        while let Some(msg) = ws_rx.next().await {
            if stop_rx.has_changed().unwrap_or(false) {
                info!("Reader received stop signal for user {}", user_id);
                break;
            }

            match msg {
                Ok(Message::Text(text)) => {
                    debug!(
                        "Received text message from user {}: {} chars",
                        user_id,
                        text.len()
                    );

                    let now = Instant::now();
                    if now.duration_since(window_start) > RATE_LIMIT_WINDOW {
                        message_count = 0;
                        window_start = now;
                    }
                    message_count += 1;

                    if message_count > MAX_MESSAGES_PER_MINUTE {
                        warn!(
                            "Rate limit exceeded for user {} (message #{})",
                            user_id, message_count
                        );
                        let error_response = json!({
                            "type": "error",
                            "message": "Rate limit exceeded",
                            "retry_after": RATE_LIMIT_WINDOW.as_secs() - now.duration_since(window_start).as_secs()
                        });

                        if let Ok(txt) = serde_json::to_string(&error_response) {
                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                        }
                        continue;
                    }

                    let mut value = match serde_json::from_str::<Value>(&text) {
                        Ok(mut v) => {
                            v["author_id"] = Value::String(user_id.to_string());
                            v["author_username"] = Value::String(username.clone());
                            v["timestamp"] = Value::Number(serde_json::Number::from(
                                chrono::Utc::now().timestamp(),
                            ));
                            v
                        }
                        Err(e) => {
                            warn!(
                                "Invalid JSON from user {}: {} - treating as plain text",
                                user_id, e
                            );
                            json!({
                                "type": "text",
                                "content": text,
                                "author_id": user_id.to_string(),
                                "author_username": username.clone(),
                                "timestamp": chrono::Utc::now().timestamp()
                            })
                        }
                    };

                    let message_type = value
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown");
                    debug!(
                        "Processing message type '{}' from user {}",
                        message_type, user_id
                    );

                    match message_type {
                        "subscribe" => {
                            info!("User {} subscribing to updates", username);

                            match get_initial_state(&state.pool, user_id).await {
                                Ok(initial_state) => {
                                    let msg = json!({
                                        "type": "initial_state",
                                        "conversations": initial_state.conversations,
                                        "user_sequence": initial_state.user_sequence,
                                        "timestamp": chrono::Utc::now().timestamp()
                                    });

                                    if let Ok(txt) = serde_json::to_string(&msg) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                        info!(
                                            "Sent initial state to user {} with {} conversations",
                                            username,
                                            initial_state.conversations.len()
                                        );
                                    }

                                    if initial_state.pending_events.len() > 0 {
                                        let events_msg = json!({
                                            "type": "user_events_resume",
                                            "events": initial_state.pending_events,
                                            "count": initial_state.pending_events.len()
                                        });

                                        if let Ok(txt) = serde_json::to_string(&events_msg) {
                                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                            info!(
                                                "Sent {} pending events to user {}",
                                                initial_state.pending_events.len(),
                                                username
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to get initial state for user {}: {}",
                                        username, e
                                    );
                                    let error_msg = json!({
                                        "type": "error",
                                        "message": "Failed to load initial state",
                                        "error_code": "INITIAL_STATE_ERROR"
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_msg) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                }
                            }
                        }

                        "ping" => {
                            debug!("Ping received from user {}", user_id);

                            let client_user_seq =
                                value.get("user_sequence").and_then(|s| s.as_u64());

                            match state.handle_ping(user_id, client_user_seq, &out_tx).await {
                                Ok(_) => debug!("Handled ping for user {}", user_id),
                                Err(e) => error!("Failed to handle ping: {}", e),
                            }

                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "request_user_resume" => {
                            if let Err(e) =
                                handle_user_events_resume_request(&state, &value, user_id, &out_tx)
                                    .await
                            {
                                error!("Failed to handle user resume request: {}", e);
                            }
                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "request_messages_resume" => {
                            debug!("Messages resume request from user {}", user_id);

                            let conversation_id = value
                                .get("conversation_id")
                                .and_then(|s| s.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok());

                            let from_sequence = value
                                .get("from_sequence")
                                .and_then(|s| s.as_u64())
                                .unwrap_or(0);

                            let limit = value
                                .get("limit")
                                .and_then(|s| s.as_i64())
                                .unwrap_or(100)
                                .min(1000);

                            if let Some(conv_id) = conversation_id {
                                let user_id_str = user_id.to_string();
                                let conv_id_str = conv_id.to_string();

                                let is_participant: i64 = match sqlx::query_scalar(
                                    "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
                                )
                                    .bind(&conv_id_str)
                                    .bind(&user_id_str)
                                    .fetch_one(&state.pool)
                                    .await {
                                    Ok(count) => count,
                                    Err(e) => {
                                        error!("Failed to check participant status: {}", e);
                                        0
                                    }
                                };

                                if is_participant == 0 {
                                    let error_response = json!({
                                        "type": "error",
                                        "message": "Not authorized for this conversation",
                                        "error_code": "FORBIDDEN"
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                    continue;
                                }

                                match state
                                    .get_messages_since_sequence(conv_id, from_sequence, limit)
                                    .await
                                {
                                    Ok(messages) => {
                                        if !messages.is_empty() {
                                            info!(
                                                "Sending {} messages in resume for conversation {} to user {}",
                                                messages.len(),
                                                conv_id,
                                                user_id
                                            );
                                            if let Err(e) = state
                                                .send_messages_resume(
                                                    user_id, conv_id, messages, &out_tx,
                                                )
                                                .await
                                            {
                                                error!("Failed to send messages resume: {}", e);
                                            }
                                        } else {
                                            let current_seq = state
                                                .get_current_message_sequence(conv_id)
                                                .await
                                                .unwrap_or(0);
                                            let response = json!({
                                                "type": "messages_resume_complete",
                                                "conversation_id": conv_id,
                                                "from_sequence": from_sequence,
                                                "current_sequence": current_seq,
                                                "messages_count": 0,
                                                "message": "No messages to resume"
                                            });
                                            if let Ok(txt) = serde_json::to_string(&response) {
                                                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to get messages for resume: {}", e);
                                        let error_response = json!({
                                            "type": "error",
                                            "message": "Failed to retrieve messages",
                                            "error_code": "RESUME_ERROR",
                                            "details": e.to_string()
                                        });
                                        if let Ok(txt) = serde_json::to_string(&error_response) {
                                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                        }
                                    }
                                }
                            } else {
                                let error_response = json!({
                                    "type": "error",
                                    "message": "Missing or invalid conversation_id",
                                    "error_code": "INVALID_REQUEST"
                                });
                                if let Ok(txt) = serde_json::to_string(&error_response) {
                                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                }
                            }
                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "create_conversation" => {
                            debug!("Create conversation request from user {}", user_id);
                            last_heartbeat = Instant::now();

                            match handle_create_conversation(&state, &mut value, user_id, &username)
                                .await
                            {
                                Ok(()) => {
                                    debug!(
                                        "Successfully created conversation for user {}",
                                        user_id
                                    );
                                    total_messages_processed += 1;
                                }
                                Err(e) => {
                                    error!("Failed to create conversation: {}", e);
                                    let error_response = json!({
                                        "type": "error",
                                        "message": e.to_string(),
                                        "error_code": "CONVERSATION_CREATE_FAILED",
                                        "client_temp_id": value.get("client_temp_id")
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                }
                            }
                        }

                        "chat_message" => {
                            total_messages_processed += 1;
                            last_heartbeat = Instant::now();

                            // Usa il router per gestire messaggi e conversazioni
                            match handle_incoming_message(&state, &mut value, user_id, &username)
                                .await
                            {
                                Ok(()) => {
                                    debug!("Successfully processed message from user {}", user_id);

                                    // Gestione ack se presente client_msg_id
                                    if let Some(msg_id) = value.get("client_msg_id") {
                                        let ack = json!({
                                            "type": "message_ack",
                                            "client_msg_id": msg_id,
                                            "server_timestamp": chrono::Utc::now().timestamp()
                                        });
                                        if let Ok(txt) = serde_json::to_string(&ack) {
                                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to process message from user {}: {}",
                                        user_id, e
                                    );
                                    let error_response = json!({
                                        "type": "error",
                                        "message": e.to_string(),
                                        "error_code": "MESSAGE_PROCESSING_FAILED",
                                        "client_msg_id": value.get("client_msg_id"),
                                        "client_temp_id": value.get("client_temp_id")
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }

                                    // Gestione errori critici
                                    match &e {
                                        crate::error::AppError::Forbidden => {
                                            warn!(
                                                "User {} attempted unauthorized action, closing connection",
                                                user_id
                                            );
                                            let _ = stop_tx.send(true);
                                            break;
                                        }
                                        crate::error::AppError::Sqlx(_) => {
                                            error!(
                                                "Database error for user {}, closing connection",
                                                user_id
                                            );
                                            let _ = stop_tx.send(true);
                                            break;
                                        }
                                        crate::error::AppError::Internal(msg)
                                        if msg.contains("database") || msg.contains("sql") =>
                                            {
                                                error!(
                                                "Database-related internal error for user {}, closing connection",
                                                user_id
                                            );
                                                let _ = stop_tx.send(true);
                                                break;
                                            }
                                        crate::error::AppError::Unauthorized => {
                                            warn!(
                                                "Unauthorized action by user {}, closing connection",
                                                user_id
                                            );
                                            let _ = stop_tx.send(true);
                                            break;
                                        }
                                        _ => {
                                            warn!("Non-critical error for user {}: {}", user_id, e);
                                        }
                                    }
                                }
                            }
                        }
                        "delete_conversation" => {
                            last_heartbeat = Instant::now();

                            let cid_opt = value
                                .get("conversation_id")
                                .and_then(|v| v.as_str())
                                .and_then(|s| uuid::Uuid::parse_str(s).ok());

                            if cid_opt.is_none() {
                                let err = json!({
                                    "type":"error",
                                    "error_code":"INVALID_REQUEST",
                                    "message":"Missing or invalid conversation_id",
                                    "op":"delete_conversation"
                                });
                                if let Ok(txt) = serde_json::to_string(&err) {
                                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                }
                                continue;
                            }

                            let conversation_id = cid_opt.unwrap();

                            // Recupera i partecipanti per broadcast (non usato per autorizzazione)
                            let participants_res = crate::services::conversation_service::ConversationService::list_participant_ids(&state.pool, conversation_id).await;

                            // Esegui delete: l'autorizzazione viene validata nel service
                            let delete_res = crate::services::conversation_service::ConversationService::delete_conversation(
                                &state.pool,
                                conversation_id,
                                user_id
                            ).await;

                            if let Err(e) = delete_res {
                                // Mantieni semantica più specifica per gli errori comuni
                                let (code, message) = match &e {
                                    crate::error::AppError::Unauthorized => {
                                        ("FORBIDDEN", "User not authorized".to_string())
                                    }
                                    crate::error::AppError::NotFound => {
                                        ("NOT_FOUND", "Conversation not found".to_string())
                                    }
                                    _ => ("DELETE_FAILED", format!("Delete failed: {}", e)),
                                };
                                let err = json!({
                                    "type":"error",
                                    "error_code": code,
                                    "message": message,
                                    "op":"delete_conversation",
                                    "conversation_id": conversation_id
                                });
                                if let Ok(txt) = serde_json::to_string(&err) {
                                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                }
                                continue;
                            }

                            // Broadcast a tutti, incluso autore (se disponibile la lista partecipanti)
                            if let Ok(participants) = participants_res {
                                ConversationService::broadcast_conversation_deleted(
                                    &state,
                                    conversation_id,
                                    user_id,
                                    participants.clone(),
                                    true,
                                )
                                    .await;
                            }

                            // (Opzionale) ack esplicito
                            let ack = json!({
                                "type":"delete_conversation_ack",
                                "conversation_id": conversation_id,
                                "status":"ok"
                            });
                            if let Ok(txt) = serde_json::to_string(&ack) {
                                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                            }

                            continue;
                        }
                        "mark_read" => {
                            if let Err(e) = handle_mark_read(&state, user_id, &value).await {
                                error!("Failed to handle mark_read from user {}: {}", user_id, e);
                            }
                        }

                        "invite_user" => {
                            debug!("Invite user request from user {}", user_id);
                            last_heartbeat = Instant::now();

                            match handle_incoming_message(&state, &mut value, user_id, &username).await {
                                Ok(()) => {
                                    debug!("Successfully invited user to group by {}", user_id);
                                }
                                Err(e) => {
                                    error!("Failed to invite user: {}", e);
                                    let error_response = json!({
                                        "type": "error",
                                        "message": e.to_string(),
                                        "error_code": "INVITE_USER_FAILED"
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                }
                            }
                        }

                        "create_group_with_participants" => {
                            debug!("Create group with participants request from user {}", user_id);
                            last_heartbeat = Instant::now();

                            match handle_create_group_with_participants(&state, &mut value, user_id, &username, &out_tx).await {
                                Ok(()) => {
                                    debug!("Successfully created group with participants by {}", user_id);
                                }
                                Err(e) => {
                                    error!("Failed to create group with participants: {}", e);
                                    let error_response = json!({
                                        "type": "error",
                                        "message": e.to_string(),
                                        "error_code": "CREATE_GROUP_FAILED"
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                }
                            }
                        }

                        "leave_group" => {
                            last_heartbeat = Instant::now();

                            let cid_opt = value
                                .get("conversation_id")
                                .and_then(|v| v.as_str())
                                .and_then(|s| uuid::Uuid::parse_str(s).ok());

                            if cid_opt.is_none() {
                                let err = json!({
                                    "type":"error",
                                    "error_code":"INVALID_REQUEST",
                                    "message":"Missing or invalid conversation_id",
                                    "op":"leave_group"
                                });
                                if let Ok(txt) = serde_json::to_string(&err) {
                                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                }
                                continue;
                            }

                            let conversation_id = cid_opt.unwrap();

                            // Recupera i partecipanti prima di rimuovere l'utente
                            let participants_res = crate::services::conversation_service::ConversationService::list_participant_ids(&state.pool, conversation_id).await;

                            // Recupera username dell'utente che sta uscendo
                            let username_res = sqlx::query_scalar::<_, String>(
                                "SELECT username FROM users WHERE id = ?"
                            )
                            .bind(user_id.to_string())
                            .fetch_one(&state.pool)
                            .await;

                            // Esegui leave_group: l'autorizzazione viene validata nel service
                            let leave_res = crate::services::conversation_service::ConversationService::leave_group(
                                &state.pool,
                                conversation_id,
                                user_id
                            ).await;

                            if let Err(e) = leave_res {
                                let (code, message) = match &e {
                                    crate::error::AppError::Unauthorized => ("FORBIDDEN", "User not authorized".to_string()),
                                    crate::error::AppError::NotFound => ("NOT_FOUND", "Conversation not found".to_string()),
                                    crate::error::AppError::BadRequest(msg) => ("BAD_REQUEST", msg.clone()),
                                    _ => ("LEAVE_FAILED", format!("Leave failed: {}", e)),
                                };
                                let err = json!({
                                    "type":"error",
                                    "error_code": code,
                                    "message": message,
                                    "op":"leave_group",
                                    "conversation_id": conversation_id
                                });
                                if let Ok(txt) = serde_json::to_string(&err) {
                                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                }
                                continue;
                            }

                            // Broadcast a tutti i partecipanti rimanenti (escluso chi sta uscendo)
                            if let (Ok(participants), Ok(username)) = (participants_res, username_res) {
                                // Filtra l'utente che sta uscendo
                                let remaining_participants: Vec<Uuid> = participants
                                    .into_iter()
                                    .filter(|&pid| pid != user_id)
                                    .collect();
                                
                                if !remaining_participants.is_empty() {
                                    info!(
                                        "Broadcasting user_left_group to {} remaining participants",
                                        remaining_participants.len()
                                    );
                                    ConversationService::broadcast_user_left_group(
                                        &state,
                                        conversation_id,
                                        user_id,
                                        username.clone(),
                                        remaining_participants.clone(),
                                    ).await;

                                    // Crea messaggio di sistema persistente per l'uscita
                                    let system_message_content = format!("{} ha lasciato il gruppo", username);
                                    let timestamp = chrono::Utc::now().timestamp();
                                    
                                    match crate::web_socket::helpers::create_system_message(&state, conversation_id, system_message_content.clone()).await {
                                        Ok((msg_id, sequence)) => {
                                            info!("Created persistent leave system message (id={}, seq={})", msg_id, sequence);
                                            
                                            // Ottieni owner_id e username per il broadcast
                                            let owner_data: Option<(String, String)> = sqlx::query_as(
                                                "SELECT c.owner_id, u.username FROM conversations c 
                                                 JOIN users u ON c.owner_id = u.id 
                                                 WHERE c.id = ?"
                                            )
                                            .bind(conversation_id.to_string())
                                            .fetch_optional(&state.pool)
                                            .await
                                            .unwrap_or(None);
                                            
                                            if let Some((owner_id, owner_username)) = owner_data {
                                                // Broadcast il messaggio di sistema a tutti i partecipanti rimanenti
                                                let system_msg_broadcast = json!({
                                                    "type": "message",
                                                    "id": msg_id,
                                                    "conversation_id": conversation_id,
                                                    "author_id": owner_id,
                                                    "author_username": owner_username,
                                                    "content": system_message_content.clone(),
                                                    "created_at": timestamp,
                                                    "sequence_num": sequence
                                                });
                                                
                                                let _ = crate::web_socket::helpers::broadcast_to_conversation(&state, conversation_id, system_msg_broadcast.clone()).await;
                                                
                                                // Invia anche come evento sequenziato a tutti i partecipanti rimanenti
                                                for participant_id in &remaining_participants {
                                                    if let Err(e) = state.send_sequenced_event_to_user(
                                                        *participant_id,
                                                        "new_message",
                                                        system_msg_broadcast.clone(),
                                                        Some(conversation_id),
                                                    ).await {
                                                        warn!("Failed to send leave system message event to {}: {}", participant_id, e);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Failed to create leave system message (non-fatal): {}", e);
                                        }
                                    }

                                    // Invia anche member_list_updated con la lista aggiornata
                                    let members_data = match crate::services::conversation_service::ConversationService::get_members(
                                        &state.pool, 
                                        conversation_id, 
                                        remaining_participants[0] // Usa il primo partecipante rimanente
                                    ).await {
                                        Ok(data) => data,
                                        Err(e) => {
                                            warn!("Failed to get updated members list after user left: {}", e);
                                            Vec::new()
                                        }
                                    };

                                    let members: Vec<serde_json::Value> = members_data
                                        .into_iter()
                                        .map(|(user_id, username, role, joined_at)| json!({
                                            "user_id": user_id,
                                            "username": username,
                                            "role": role,
                                            "joined_at": joined_at
                                        }))
                                        .collect();

                                    let payload = json!({
                                        "conversation_id": conversation_id,
                                        "members": members,
                                        "timestamp": chrono::Utc::now().timestamp()
                                    });

                                    for participant_id in &remaining_participants {
                                        if let Err(e) = state.send_sequenced_event_to_user(
                                            *participant_id,
                                            "member_list_updated",
                                            payload.clone(),
                                            Some(conversation_id),
                                        ).await {
                                            tracing::error!("Failed to send member_list_updated event to {}: {}", participant_id, e);
                                        }
                                    }
                                    
                                    info!("Sent member_list_updated event to {} participants", remaining_participants.len());
                                } else {
                                    info!("No remaining participants to notify (group now empty)");
                                }
                            }

                            // Ack esplicito all'utente che è uscito
                            let ack = json!({
                                "type":"leave_group_ack",
                                "conversation_id": conversation_id,
                                "status":"ok"
                            });
                            if let Ok(txt) = serde_json::to_string(&ack) {
                                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                            }
                            
                            continue;
                        }

                        _ => {
                            warn!(
                                "Unknown message type '{}' from user {}: {:?}",
                                message_type, user_id, value
                            );
                            let error_response = json!({
                                "type": "error",
                                "message": format!("Unknown message type: {}", message_type),
                                "error_code": "UNKNOWN_MESSAGE_TYPE"
                            });
                            if let Ok(txt) = serde_json::to_string(&error_response) {
                                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                            }
                        }
                    }
                }
                Ok(Message::Close(frame)) => {
                    info!("Close frame received from user {}: {:?}", user_id, frame);
                    let close_frame = frame.map(|f| axum::extract::ws::CloseFrame {
                        code: f.code,
                        reason: f.reason.into_owned().into(),
                    });
                    let _ = out_tx.send(OutboundMsg::Close(close_frame)).await;
                    let _ = stop_tx.send(true);
                    break;
                }
                Ok(Message::Ping(data)) => {
                    debug!(
                        "Ping frame received from user {} ({} bytes)",
                        user_id,
                        data.len()
                    );
                    let _ = out_tx.send(OutboundMsg::Pong(data)).await;
                    last_heartbeat = Instant::now();
                }
                Ok(Message::Pong(_data)) => {
                    debug!("Pong frame received from user {}", user_id);
                    last_heartbeat = Instant::now();
                }
                Ok(Message::Binary(data)) => {
                    debug!(
                        "Binary message received from user {} ({} bytes)",
                        user_id,
                        data.len()
                    );
                    let _ = out_tx.send(OutboundMsg::Binary(data)).await;
                }
                Err(e) => {
                    error!("WebSocket read error for user {}: {}", user_id, e);

                    let error_str = e.to_string().to_lowercase();
                    if error_str.contains("connection")
                        && (error_str.contains("closed") || error_str.contains("reset"))
                    {
                        info!("Connection closed/reset by client for user {}", user_id);
                    } else if error_str.contains("protocol") {
                        warn!("WebSocket protocol error for user {}: {}", user_id, e);
                    } else {
                        warn!("WebSocket error for user {}: {}", user_id, e);
                    }

                    let _ = stop_tx.send(true);
                    break;
                }
            }

            if last_heartbeat.elapsed() > CLIENT_HEARTBEAT_TIMEOUT {
                warn!(
                    "Client heartbeat timeout for user {} (last seen: {:?} ago)",
                    user_id,
                    last_heartbeat.elapsed()
                );

                let timeout_warning = json!({
                    "type": "warning",
                    "message": "Client heartbeat timeout - connection will be closed",
                    "timeout_seconds": CLIENT_HEARTBEAT_TIMEOUT.as_secs()
                });
                if let Ok(txt) = serde_json::to_string(&timeout_warning) {
                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                }

                tokio::time::sleep(Duration::from_secs(5)).await;

                if last_heartbeat.elapsed() > CLIENT_HEARTBEAT_TIMEOUT + Duration::from_secs(5) {
                    error!(
                        "Client still not responding, closing connection for user {}",
                        user_id
                    );
                    let _ = stop_tx.send(true);
                    break;
                }
            }
        }

        info!(
            "Reader task ended for user {} (processed {} messages)",
            user_id, total_messages_processed
        );
    })
}

// Helper structures and functions
struct InitialState {
    conversations: Vec<Value>,
    user_sequence: u64,
    pending_events: Vec<Value>,
}

async fn get_initial_state(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<InitialState, Box<dyn std::error::Error + Send + Sync>> {
    let user_id_str = user_id.to_string();

    // Query ottimizzata che gestisce correttamente i titoli DM e include author_id
    let query = r#"
            SELECT
                c.id as conv_id,
                c.title as conv_title,
                c.kind as conv_kind,
                c.owner_id as conv_owner_id,
                c.created_at as conv_created_at,
                CASE
                    WHEN c.kind = 'dm' AND (c.title IS NULL OR c.title = '') THEN (
                        SELECT u.username
                        FROM participants p2
                        INNER JOIN users u ON p2.user_id = u.id
                        WHERE p2.conversation_id = c.id
                        AND p2.user_id != ?
                        LIMIT 1
                    )
                    WHEN c.title IS NULL OR c.title = '' THEN 'Untitled'
                    ELSE c.title
                END as display_title,
                (SELECT m.id
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_msg_id,
                (SELECT m.content
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_content,
                (SELECT m.author_id
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_author_id,
                (SELECT u.username
                 FROM messages m
                 INNER JOIN users u ON m.author_id = u.id
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_author,
                (SELECT m.created_at
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_msg_time,
                (SELECT m.sequence_num
                 FROM messages m
                 WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1) as last_sequence,
                p.last_read_sequence,
                (SELECT COUNT(*)
                 FROM messages m2
                 WHERE m2.conversation_id = c.id) as message_count
            FROM conversations c
            INNER JOIN participants p ON c.id = p.conversation_id
            WHERE p.user_id = ?
            ORDER BY COALESCE(
                (SELECT m.created_at FROM messages m WHERE m.conversation_id = c.id
                 ORDER BY m.created_at DESC LIMIT 1),
                c.created_at
            ) DESC
        "#;

    let rows = sqlx::query(query)
        .bind(&user_id_str) // Primo parametro per il CASE WHEN
        .bind(&user_id_str) // Secondo parametro per il WHERE principale
        .fetch_all(pool)
        .await?;

    let mut conversations: Vec<Value> = Vec::new();

    for row in rows {
        let id: String = row.try_get("conv_id").unwrap_or_default();
        let kind: String = row.try_get("conv_kind").unwrap_or_default();
        let owner_id: String = row.try_get("conv_owner_id").unwrap_or_default();
        let created_at: i64 = row.try_get("conv_created_at").unwrap_or(0);
        let message_count: i64 = row.try_get("message_count").unwrap_or(0);
        let last_read_seq: i64 = row.try_get("last_read_sequence").unwrap_or(0);

        let display_title: String = row.try_get("display_title").unwrap_or_else(|_| {
            if kind == "dm" {
                "Direct Message".to_string()
            } else {
                "Untitled Group".to_string()
            }
        });

        let mut conv = json!({
            "id": id,
            "kind": kind,
            "title": display_title,
            "owner_id": owner_id,
            "created_at": created_at,
            "message_count": message_count,
            "last_read_sequence": last_read_seq
        });

        // Aggiungi ultimo messaggio SOLO se esiste veramente
        if let Ok(Some(content)) = row.try_get::<Option<String>, _>("last_content") {
            if !content.is_empty() {
                if let Ok(Some(author)) = row.try_get::<Option<String>, _>("last_author") {
                    let mut last_message = json!({
                        "content": content,
                        "author_username": author
                    });

                    // ✅ MODIFICA 2: Includi l'UUID del messaggio per il controllo duplicati lato client
                    if let Ok(Some(msg_id)) = row.try_get::<Option<String>, _>("last_msg_id") {
                        last_message["id"] = json!(msg_id);
                    }

                    // Includi author_id nel last_message
                    if let Ok(Some(author_id)) = row.try_get::<Option<String>, _>("last_author_id")
                    {
                        last_message["author_id"] = json!(author_id);
                    }

                    if let Ok(Some(msg_time)) = row.try_get::<Option<i64>, _>("last_msg_time") {
                        last_message["created_at"] = json!(msg_time);
                    }

                    if let Ok(Some(seq)) = row.try_get::<Option<i64>, _>("last_sequence") {
                        last_message["sequence_num"] = json!(seq);
                        conv["last_message"] = last_message;
                    } else {
                        debug!(
                            "Message without sequence for conversation {}, not including in initial state",
                            id
                        );
                    }
                }
            }
        }

        conversations.push(conv);
    }

    // Recupera ultima user sequence
    let user_sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence_num), 0) FROM user_events WHERE user_id = ?",
    )
        .bind(&user_id_str)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    info!(
        "Loaded {} conversations for user {} (only including last_message where messages exist)",
        conversations.len(),
        user_id_str
    );

    Ok(InitialState {
        conversations,
        user_sequence: user_sequence as u64,
        pending_events: Vec::new(),
    })
}

async fn handle_create_group_with_participants(
    state: &AppState,
    value: &mut serde_json::Value,
    creator_id: Uuid,
    creator_username: &str,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> crate::error::Result<()> {
    use crate::services::conversation_service::ConversationService;

    // Estrai parametri
    let group_name = value
        .get("group_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("Missing group_name".into()))?;

    let participant_usernames = value
        .get("participant_usernames")
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::error::AppError::BadRequest("Missing participant_usernames".into()))?;

    // Estrai client_temp_id se presente
    let client_temp_id = value
        .get("client_temp_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    info!(
        "Creating group '{}' by {} with {} participants (client_temp_id: {:?})",
        group_name,
        creator_username,
        participant_usernames.len(),
        client_temp_id
    );

    // 1. Crea il gruppo
    let group_id = ConversationService::create_group(&state.pool, group_name, creator_id).await?;
    info!("Group created with ID: {}", group_id);

    // 2. Raccogli tutti i participant IDs (creatore + invitati)
    let mut all_participant_ids = vec![creator_id];

    for username_value in participant_usernames {
        if let Some(username) = username_value.as_str() {
            // Cerca l'utente per username
            match sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&state.pool)
                .await?
            {
                Some(user_id_str) => {
                    match Uuid::parse_str(&user_id_str) {
                        Ok(user_id) => {
                            if user_id != creator_id {
                                all_participant_ids.push(user_id);
                            }
                        }
                        Err(e) => {
                            warn!("Invalid UUID for user {}: {}", username, e);
                        }
                    }
                }
                None => {
                    warn!("User {} not found, skipping", username);
                }
            }
        }
    }

    info!(
        "Adding {} participants to group {}",
        all_participant_ids.len(),
        group_id
    );

    // 3. Aggiungi tutti i partecipanti al gruppo (usa add_member con requester_id = creator)
    for participant_id in &all_participant_ids {
        if *participant_id != creator_id {
            ConversationService::add_member(&state.pool, group_id, *participant_id, creator_id).await?;
        }
    }

    // 4. Carica la conversazione completa per broadcast (usa get_conversation)
    let conversation_opt = ConversationService::get_conversation(&state.pool, group_id, creator_id).await?;

    let (id, kind, title, owner_id, created_at, last_read_seq, last_activity, last_msg_seq) = match conversation_opt {
        Some(data) => data,
        None => {
            return Err(crate::error::AppError::Internal("Failed to load created group".into()));
        }
    };

    let mut conversation = json!({
        "id": id,
        "kind": kind,
        "title": title,
        "owner_id": owner_id,
        "created_at": created_at,
        "last_read_sequence": last_read_seq,
        "last_activity": last_activity,
        "last_msg_seq": last_msg_seq
    });

    // Aggiungi client_temp_id se presente
    if let Some(ref temp_id) = client_temp_id {
        conversation["client_temp_id"] = json!(temp_id);
    }

    // 5. Crea eventi user_events con sequenze per TUTTI i partecipanti (incluso il creatore)
    for participant_id in &all_participant_ids {
        // Genera sequenza per questo utente
        let user_sequence = state.get_next_user_sequence(*participant_id).await?;

        // Salva evento nella tabella user_events
        sqlx::query(
            "INSERT INTO user_events (user_id, sequence_num, event_type, event_data, conversation_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
            .bind(participant_id.to_string())
            .bind(user_sequence as i64)
            .bind("new_conversation")
            .bind(conversation.to_string())
            .bind(group_id.to_string())
            .bind(created_at)
            .execute(&state.pool)
            .await
            .map_err(|e| crate::error::AppError::from(e))?;

        info!(
            "Created user_event for participant {} with sequence {}",
            participant_id, user_sequence
        );

        // Invia notifica in tempo reale se l'utente è connesso
        let notification = json!({
            "type": "user_notification",
            "sequence": user_sequence,
            "event_type": "new_conversation",
            "event_data": {
                "conversation": conversation.clone()
            },
            "conversation_id": group_id
        });

        if let Some(user_tx) = state.user_notification_channels.read().await.get(participant_id) {
            match user_tx.send(notification) {
                Ok(_) => info!("Sent user_notification to participant {}", participant_id),
                Err(e) => warn!("Failed to send notification to participant {}: {}", participant_id, e),
            }
        } else {
            info!("Participant {} not connected, will receive event on reconnect", participant_id);
        }
    }

    info!(
        "Group '{}' created successfully with {} participants",
        group_name,
        all_participant_ids.len()
    );

    Ok(())
}