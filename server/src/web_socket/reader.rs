use axum::extract::ws::{Message, WebSocket};
use futures::{StreamExt, stream::SplitStream};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use sqlx::Row;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{actor::OutboundMsg, helpers::handle_chat_message};
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
        // Rate limiting migliorato con sliding window
        let mut message_count = 0u32;
        let mut window_start = Instant::now();
        const MAX_MESSAGES_PER_MINUTE: u32 = 60;
        const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

        // Contatori per statistiche
        let mut total_messages_processed = 0u64;
        let mut last_heartbeat = Instant::now();
        const CLIENT_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(120); // 2 minuti

        info!("Reader task started for user {}", user_id);

        while let Some(msg) = ws_rx.next().await {
            // Check per stop signal
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

                    // Rate limiting con sliding window
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

                    // Parse del messaggio con gestione errori migliorata
                    let mut value = match serde_json::from_str::<Value>(&text) {
                        Ok(mut v) => {
                            // Aggiungi metadata dell'utente
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

                    // Gestione dei diversi tipi di messaggio
                    match message_type {
                        "ping" => {
                            debug!("Ping received from user {}", user_id);

                            // Estrai le sequenze dal ping
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

                            // Retrocompatibilità con vecchio formato
                            let legacy_sequence = value
                                .get("last_sequence")
                                .and_then(|s| s.as_u64());

                            if client_user_seq.is_some() || client_conv_seq.is_some() {
                                // Nuovo formato ping con sequenze separate
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
                                // Vecchio formato - usa solo user sequence per retrocompatibilità
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
                                // Ping semplice senza sequence
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
                            // Richiesta esplicita di resume per user events
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
                            // Richiesta esplicita di resume per messaggi di una conversazione
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
                                // Verifica che l'utente sia partecipante della conversazione
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

                        "get_conversation_sequences" => {
                            // Richiesta per ottenere tutte le sequenze delle conversazioni dell'utente
                            debug!("Conversation sequences request from user {}", user_id);

                            let user_id_str = user_id.to_string();

                            // Ottieni tutte le conversazioni dell'utente con le loro sequenze
                            let rows = match sqlx::query(
                                "SELECT p.conversation_id, COALESCE(ms.current_sequence, 0) as seq
                                 FROM participants p
                                 LEFT JOIN message_sequences ms ON p.conversation_id = ms.conversation_id
                                 WHERE p.user_id = ?"
                            )
                                .bind(&user_id_str)
                                .fetch_all(&state.pool)
                                .await {
                                Ok(rows) => rows,
                                Err(e) => {
                                    error!("Failed to get conversation sequences: {}", e);
                                    let error_response = json!({
                                        "type": "error",
                                        "message": "Failed to retrieve conversation sequences",
                                        "error_code": "DB_ERROR"
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }
                                    continue;
                                }
                            };

                            let mut sequences = serde_json::Map::new();
                            for row in rows {
                                if let (Ok(conv_id), Ok(seq)) = (
                                    row.try_get::<String, _>("conversation_id"),
                                    row.try_get::<i64, _>("seq")
                                ) {
                                    sequences.insert(conv_id, json!(seq));
                                }
                            }

                            let response = json!({
                                "type": "conversation_sequences",
                                "sequences": sequences,
                                "user_sequence": state.get_current_user_sequence(user_id).await.unwrap_or(0),
                                "timestamp": chrono::Utc::now().timestamp()
                            });

                            if let Ok(txt) = serde_json::to_string(&response) {
                                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                            }

                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "sequence_ack" => {
                            // Acknowledgment di eventi/messaggi ricevuti dal client
                            debug!("Sequence ack received from user {}", user_id);

                            let user_seq = value.get("user_sequence").and_then(|s| s.as_u64());
                            let conv_sequences = value.get("conversation_sequences").and_then(|s| s.as_object());

                            // Aggiorna tracking per user events
                            if let Some(seq) = user_seq {
                                let user_id_str = user_id.to_string();
                                let _ = sqlx::query(
                                    "UPDATE user_events SET delivered = TRUE
                                     WHERE user_id = ? AND sequence_num <= ? AND delivered = FALSE"
                                )
                                    .bind(&user_id_str)
                                    .bind(seq as i64)
                                    .execute(&state.pool)
                                    .await;

                                debug!("Marked user events as delivered up to sequence {} for user {}", seq, user_id);
                            }

                            // Log conversation sequences acknowledgments (per future analytics)
                            if let Some(conv_seqs) = conv_sequences {
                                for (conv_id_str, seq_val) in conv_seqs {
                                    if let Some(seq) = seq_val.as_u64() {
                                        debug!("User {} acknowledged messages up to sequence {} in conversation {}",
                                               user_id, seq, conv_id_str);
                                    }
                                }
                            }

                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "pong" => {
                            debug!("Pong received from user {}", user_id);
                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "heartbeat" => {
                            debug!("Heartbeat received from user {}", user_id);
                            let heartbeat_ack = json!({
                                "type": "heartbeat_ack",
                                "timestamp": chrono::Utc::now().timestamp(),
                                "server_time": chrono::Utc::now().to_rfc3339()
                            });
                            if let Ok(txt) = serde_json::to_string(&heartbeat_ack) {
                                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                            }
                            last_heartbeat = Instant::now();
                            continue;
                        }

                        "subscribe" | "unsubscribe" => {
                            // Questi messaggi potrebbero essere gestiti in futuro
                            debug!(
                                "Subscription message '{}' from user {} - not implemented",
                                message_type, user_id
                            );
                            continue;
                        }

                        "chat_message" => {
                            // Gestione dei messaggi di chat
                            total_messages_processed += 1;
                            last_heartbeat = Instant::now();

                            match handle_chat_message(&state, &mut value, user_id, &username).await
                            {
                                Ok(()) => {
                                    debug!(
                                        "Successfully processed chat message from user {}",
                                        user_id
                                    );
                                    // Invio conferma opzionale al client
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
                                        "Failed to save chat message from user {}: {}",
                                        user_id, e
                                    );
                                    let error_response = json!({
                                        "type": "error",
                                        "message": "Failed to save message",
                                        "error_code": "MESSAGE_SAVE_FAILED",
                                        "client_msg_id": value.get("client_msg_id")
                                    });
                                    if let Ok(txt) = serde_json::to_string(&error_response) {
                                        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                                    }

                                    // Per errori critici, chiudi la connessione
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
                    // Per ora, rimandiamo indietro i dati binari
                    let _ = out_tx.send(OutboundMsg::Binary(data)).await;
                }
                Err(e) => {
                    error!("WebSocket read error for user {}: {}", user_id, e);

                    // Gestione migliorata degli errori WebSocket
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

            // Controllo timeout heartbeat dal client
            if last_heartbeat.elapsed() > CLIENT_HEARTBEAT_TIMEOUT {
                warn!(
                    "Client heartbeat timeout for user {} (last seen: {:?} ago)",
                    user_id,
                    last_heartbeat.elapsed()
                );

                // Invia warning al client prima di chiudere
                let timeout_warning = json!({
                    "type": "warning",
                    "message": "Client heartbeat timeout - connection will be closed",
                    "timeout_seconds": CLIENT_HEARTBEAT_TIMEOUT.as_secs()
                });
                if let Ok(txt) = serde_json::to_string(&timeout_warning) {
                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                }

                // Aspetta un po' per permettere al client di rispondere
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