use axum::extract::ws::{Message, WebSocket};
use futures::{StreamExt, stream::SplitStream};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use sqlx::{Row, SqlitePool};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use crate::services::conversation_service::ConversationService;

use super::{
    actor::OutboundMsg,
    helpers::{
        handle_chat_message,
        handle_create_conversation,
        handle_incoming_message
    }
};
use crate::state::AppState;

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
                                        info!("Sent initial state to user {} with {} conversations",
                                              username, initial_state.conversations.len());
                                    }

                                    if initial_state.pending_events.len() > 0 {
                                        let events_msg = json!({
                                            "type": "user_events_resume",
                                            "events": initial_state.pending_events,
                                            "count": initial_state.pending_events.len()
                                        });

                                        if let Ok(txt) = serde_json::to_string(&events_msg) {
                                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                            info!("Sent {} pending events to user {}",
                                                  initial_state.pending_events.len(), username);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to get initial state for user {}: {}", username, e);
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

                            let client_user_seq = value
                                .get("user_sequence")
                                .and_then(|s| s.as_u64());

                            let client_conv_seq = value
                                .get("conversation_sequence")
                                .and_then(|s| s.as_u64());

                            let active_conversation = value
                                .get("active_conversation_id")
                                .and_then(|s| s.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok());

                            let legacy_sequence = value
                                .get("last_sequence")
                                .and_then(|s| s.as_u64());

                            if client_user_seq.is_some() || client_conv_seq.is_some() {
                                match state
                                    .handle_enhanced_ping(
                                        user_id,
                                        client_user_seq,
                                        client_conv_seq,
                                        active_conversation,
                                        &out_tx,
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        debug!(
                                            "Handled enhanced ping for user {} (user_seq={:?}, conv_seq={:?}, active_conv={:?})",
                                            user_id, client_user_seq, client_conv_seq, active_conversation
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to handle enhanced ping for user {}: {}",
                                            user_id, e
                                        );
                                        let error_response = json!({
                                            "type": "error",
                                            "message": "Sequence handling failed",
                                            "error_code": "SEQUENCE_ERROR",
                                            "details": e.to_string()
                                        });
                                        if let Ok(txt) = serde_json::to_string(&error_response) {
                                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                        }
                                    }
                                }
                            } else if let Some(seq) = legacy_sequence {
                                match state
                                    .handle_ping_with_sequence(
                                        user_id,
                                        seq,
                                        &out_tx,
                                    )
                                    .await
                                {
                                    Ok(_) => {
                                        debug!(
                                            "Handled legacy ping with sequence {} for user {}",
                                            seq, user_id
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to handle legacy ping for user {}: {}",
                                            user_id, e
                                        );
                                        let error_response = json!({
                                            "type": "error",
                                            "message": "Legacy sequence handling failed",
                                            "error_code": "SEQUENCE_ERROR",
                                            "details": e.to_string()
                                        });
                                        if let Ok(txt) = serde_json::to_string(&error_response) {
                                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                        }
                                    }
                                }
                            } else {
                                let current_user_seq = state.get_current_user_sequence(user_id).await.unwrap_or(0);
                                let pong_response = json!({
                                    "type": "pong",
                                    "timestamp": chrono::Utc::now().timestamp(),
                                    "current_user_sequence": current_user_seq,
                                    "message": "Use enhanced ping with sequences for gap detection"
                                });

                                if let Ok(txt) = serde_json::to_string(&pong_response) {
                                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                }
                            }

                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "request_user_resume" => {
                            debug!("User resume request from user {}", user_id);

                            let from_sequence = value
                                .get("from_sequence")
                                .and_then(|s| s.as_u64())
                                .unwrap_or(0);

                            let limit = value
                                .get("limit")
                                .and_then(|s| s.as_i64())
                                .unwrap_or(100)
                                .min(1000);

                            match state.get_user_events_since(user_id, from_sequence, limit).await {
                                Ok(events) => {
                                    if !events.is_empty() {
                                        info!("Sending {} user events in resume to user {}", events.len(), user_id);
                                        if let Err(e) = state.send_user_events_resume(user_id, events, &out_tx).await {
                                            error!("Failed to send user events resume: {}", e);
                                        }
                                    } else {
                                        let response = json!({
                                            "type": "user_resume_complete",
                                            "from_sequence": from_sequence,
                                            "current_sequence": state.get_current_user_sequence(user_id).await.unwrap_or(0),
                                            "events_count": 0,
                                            "message": "No events to resume"
                                        });
                                        if let Ok(txt) = serde_json::to_string(&response) {
                                            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to get user events for resume: {}", e);
                                    let error_response = json!({
                                        "type": "error",
                                        "message": "Failed to retrieve user events",
                                        "error_code": "RESUME_ERROR",
                                        "details": e.to_string()
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                }
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

                                match state.get_messages_since_sequence(conv_id, from_sequence, limit).await {
                                    Ok(messages) => {
                                        if !messages.is_empty() {
                                            info!("Sending {} messages in resume for conversation {} to user {}",
                                                  messages.len(), conv_id, user_id);
                                            if let Err(e) = state.send_messages_resume(user_id, conv_id, messages, &out_tx).await {
                                                error!("Failed to send messages resume: {}", e);
                                            }
                                        } else {
                                            let current_seq = state.get_current_message_sequence(conv_id).await.unwrap_or(0);
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

                            match handle_create_conversation(&state, &mut value, user_id, &username).await {
                                Ok(()) => {
                                    debug!("Successfully created conversation for user {}", user_id);
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
                            match handle_incoming_message(&state, &mut value, user_id, &username).await {
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
                                    error!("Failed to process message from user {}: {}", user_id, e);
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
                                            warn!("User {} attempted unauthorized action, closing connection", user_id);
                                            let _ = stop_tx.send(true);
                                            break;
                                        }
                                        crate::error::AppError::Sqlx(_) => {
                                            error!("Database error for user {}, closing connection", user_id);
                                            let _ = stop_tx.send(true);
                                            break;
                                        }
                                        crate::error::AppError::Internal(msg)
                                        if msg.contains("database") || msg.contains("sql") => {
                                            error!("Database-related internal error for user {}, closing connection", user_id);
                                            let _ = stop_tx.send(true);
                                            break;
                                        }
                                        crate::error::AppError::Unauthorized => {
                                            warn!("Unauthorized action by user {}, closing connection", user_id);
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
                                    crate::error::AppError::Unauthorized => ("FORBIDDEN", "User not authorized".to_string()),
                                    crate::error::AppError::NotFound => ("NOT_FOUND", "Conversation not found".to_string()),
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
                                    true
                                ).await;
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
            -- Per DM, ottieni il nome dell'altro partecipante
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
        .bind(&user_id_str)  // Primo parametro per il CASE WHEN
        .bind(&user_id_str)  // Secondo parametro per il WHERE principale
        .fetch_all(pool)
        .await?;

    let mut conversations: Vec<Value> = Vec::new();

    for row in rows {
        let id: String = row.try_get("conv_id").unwrap_or_default();
        let kind: String = row.try_get("conv_kind").unwrap_or_default();
        let owner_id: String = row.try_get("conv_owner_id").unwrap_or_default();
        let created_at: i64 = row.try_get("conv_created_at").unwrap_or(0);
        let message_count: i64 = row.try_get("message_count").unwrap_or(0);

        // Usa display_title che è già stato calcolato dalla query
        let display_title: String = row.try_get("display_title")
            .unwrap_or_else(|_| {
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
            "message_count": message_count
        });

        // Aggiungi ultimo messaggio SOLO se esiste veramente
        if let Ok(Some(content)) = row.try_get::<Option<String>, _>("last_content") {
            if !content.is_empty() {
                if let Ok(Some(author)) = row.try_get::<Option<String>, _>("last_author") {
                    let mut last_message = json!({
                        "content": content,
                        "author_username": author
                    });

                    // Includi author_id nel last_message
                    if let Ok(Some(author_id)) = row.try_get::<Option<String>, _>("last_author_id") {
                        last_message["author_id"] = json!(author_id);
                    }

                    if let Ok(Some(msg_time)) = row.try_get::<Option<i64>, _>("last_msg_time") {
                        last_message["created_at"] = json!(msg_time);
                    }

                    if let Ok(Some(seq)) = row.try_get::<Option<i64>, _>("last_sequence") {
                        last_message["sequence_num"] = json!(seq);
                        conv["last_message"] = last_message;
                    } else {
                        debug!("Message without sequence for conversation {}, not including in initial state", id);
                    }
                }
            }
        }

        conversations.push(conv);
    }

    // Recupera ultima user sequence
    let user_sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence_num), 0) FROM user_events WHERE user_id = ?"
    )
        .bind(&user_id_str)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    info!("Loaded {} conversations for user {} (only including last_message where messages exist)",
          conversations.len(), user_id_str);

    Ok(InitialState {
        conversations,
        user_sequence: user_sequence as u64,
        pending_events: Vec::new(),
    })
}