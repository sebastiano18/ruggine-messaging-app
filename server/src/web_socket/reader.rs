use axum::extract::ws::{Message, WebSocket};
use futures::{StreamExt, stream::SplitStream};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{actor::OutboundMsg, handlers, initial_state::get_initial_state};
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
                                        "members_by_conversation": initial_state.members_by_conversation,
                                        "timestamp": chrono::Utc::now().timestamp()
                                    });

                                    if let Ok(txt) = serde_json::to_string(&msg) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                        info!(
                                            "Sent initial state to user {} with {} conversations and {} groups with members",
                                            username,
                                            initial_state.conversations.len(),
                                            initial_state.members_by_conversation.len()
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
                            if let Err(e) = handlers::handle_user_events_resume_request(
                                &state, &value, user_id, &out_tx,
                            )
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
                                        if let Err(e) = state
                                            .send_messages_resume(
                                                user_id, conv_id, messages, &out_tx,
                                            )
                                            .await
                                        {
                                            error!(
                                                "Failed to send messages resume to user {}: {}",
                                                user_id, e
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        error!(
                                            "Failed to get messages for resume for user {}: {}",
                                            user_id, e
                                        );
                                    }
                                }
                            }
                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "mark_read" => {
                            if let Err(e) =
                                handlers::handle_mark_read(&state, user_id, &value).await
                            {
                                error!("Failed to handle mark_read from user {}: {}", user_id, e);
                            }
                        }

                        "invite_user" => {
                            debug!("Invite user request from user {}", user_id);
                            last_heartbeat = Instant::now();

                            match handlers::handle_incoming_message(
                                &state, &mut value, user_id, &username,
                            )
                            .await
                            {
                                Ok(()) => {
                                    debug!("Successfully invited user to group by {}", user_id);
                                }
                                Err(e) => {
                                    error!("Failed to invite user: {}", e);
                                    let user_message =
                                        if e.to_string().contains("Nessun utente è stato aggiunto")
                                        {
                                            "Nessun utente è stato aggiunto al gruppo.".to_string()
                                        } else if e.to_string().contains("solo per i gruppi") {
                                            "Puoi invitare utenti solo nei gruppi.".to_string()
                                        } else {
                                            e.to_string()
                                        };
                                    let error_response = json!({
                                        "type": "error",
                                        "message": user_message,
                                        "error_code": "INVITE_USER_FAILED"
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                }
                            }
                        }

                        "create_group_with_participants" => {
                            debug!(
                                "Create group with participants request from user {}",
                                user_id
                            );
                            last_heartbeat = Instant::now();

                            match handlers::handle_create_group_with_participants(
                                &state, &mut value, user_id, &username, &out_tx,
                            )
                            .await
                            {
                                Ok(()) => {
                                    debug!(
                                        "Successfully created group with participants by {}",
                                        user_id
                                    );
                                }
                                Err(e) => {
                                    error!("Failed to create group with participants: {}", e);
                                    let user_message =
                                        if e.to_string().contains("username richiesti") {
                                            "Devi inserire username e password.".to_string()
                                        } else if e.to_string().contains("già partecipante") {
                                            "Sei già in questa conversazione.".to_string()
                                        } else {
                                            e.to_string()
                                        };
                                    let error_response = json!({
                                        "type": "error",
                                        "message": user_message,
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
                            if let Err(e) =
                                handlers::handle_leave_group(&state, &value, user_id, &out_tx).await
                            {
                                error!("Failed to handle leave_group: {}", e);
                            }
                        }

                        "remove_member" => {
                            last_heartbeat = Instant::now();
                            if let Err(e) =
                                handlers::handle_remove_member(&state, &value, user_id, &out_tx)
                                    .await
                            {
                                error!("Failed to handle remove_member: {}", e);
                            }
                        }

                        "delete_conversation" => {
                            last_heartbeat = Instant::now();
                            if let Err(e) = handlers::handle_delete_conversation(
                                &state, &value, user_id, &out_tx,
                            )
                            .await
                            {
                                error!("Failed to handle delete_conversation: {}", e);
                            }
                        }

                        "delete_message" => {
                            last_heartbeat = Instant::now();
                            if let Err(e) =
                                handlers::handle_delete_message(&state, &value, user_id, &out_tx)
                                    .await
                            {
                                error!(
                                    "Failed to handle delete_message from user {}: {}",
                                    user_id, e
                                );
                                let error_response = json!({
                                    "type": "error",
                                    "message": e.to_string(),
                                    "error_code": "DELETE_MESSAGE_FAILED"
                                });
                                if let Ok(txt) = serde_json::to_string(&error_response) {
                                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                }
                            }
                        }

                        "check_user" => {
                            last_heartbeat = Instant::now();
                            if let Err(e) =
                                handlers::handle_check_user(&state, &value, user_id, &out_tx).await
                            {
                                error!("Failed to handle check_user: {}", e);
                            }
                        }

                        "delete_user" => {
                            info!("User {} requested account deletion", user_id);
                            last_heartbeat = Instant::now();

                            match handlers::handle_delete_user(&state, user_id, &out_tx).await {
                                Ok(()) => {
                                    info!("Successfully deleted user {}", user_id);
                                    // ✅ NON chiudere - lascia che il client chiuda dopo aver ricevuto la conferma
                                    // Il messaggio account_deleted_confirm è già stato inviato da handle_delete_user
                                }
                                Err(e) => {
                                    error!("Failed to delete user {}: {}", user_id, e);
                                    let error_response = json!({
                                        "type": "error",
                                        "message": e.to_string(),
                                        "error_code": "DELETE_USER_FAILED"
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                }
                            }
                        }

                        "chat_message" | "message" | "create_conversation" => {
                            total_messages_processed += 1;
                            last_heartbeat = Instant::now();

                            match handlers::handle_incoming_message(
                                &state, &mut value, user_id, &username,
                            )
                            .await
                            {
                                Ok(()) => {
                                    debug!("Successfully processed message from user {}", user_id);

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
