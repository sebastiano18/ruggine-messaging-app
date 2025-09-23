use crate::models::{MessageDto, UiEvent, GapInfo, UserEventData, ConversationDto};
use serde_json::Value;
use uuid::Uuid;
use tracing::{debug, error, info, warn};
use std::collections::HashMap;

/// Handler principale per messaggi WebSocket in arrivo
pub fn handle_websocket_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, msg: String) {
    debug!("Received WebSocket message: {}",
           msg.chars().take(200).collect::<String>());

    if msg.len() > 100_000 {
        error!("WebSocket message too large ({} bytes), ignoring", msg.len());
        return;
    }

    let parsed_value: Value = match serde_json::from_str(&msg) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse WebSocket JSON: {}", e);
            return;
        }
    };

    let msg_type = match parsed_value.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => {
            warn!("WebSocket message missing 'type' field");
            return;
        }
    };

    // Handle messages by type
    match msg_type {
        "chat_message" => handle_chat_message(tx, &parsed_value),
        "initial_state" => handle_initial_state(tx, &parsed_value),
        "conversation_messages" => handle_conversation_messages(tx, &parsed_value),
        "pong" => handle_simple_pong(tx, &parsed_value),
        "pong_with_sequences" => handle_enhanced_pong(tx, &parsed_value),
        "server_heartbeat" => handle_server_heartbeat(tx, &parsed_value),
        "user_channel_ready" => handle_user_channel_ready(tx, &parsed_value),
        "fetch_conversation_messages" => handle_fetch_conversation_messages(tx, &parsed_value),
        "user_events_resume" => handle_user_events_resume(tx, &parsed_value),
        "messages_resume" => handle_messages_resume(tx, &parsed_value),
        "user_resume_complete" => handle_resume_complete(tx, &parsed_value, "user"),
        "messages_resume_complete" => handle_resume_complete(tx, &parsed_value, "messages"),
        "conversation_created" => handle_conversation_created(tx, &parsed_value),
        "conversation_deleted" => handle_conversation_deleted(tx, &parsed_value),
        "error" => handle_server_error(tx, &parsed_value),
        "message_ack" => handle_message_ack(tx, &parsed_value),
        "warning" => handle_server_warning(tx, &parsed_value),
        _ => {
            debug!("Unhandled message type: {}", msg_type);
        }
    }
}

/// Handle initial state from server
fn handle_initial_state(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    // Estrai conversazioni
    let conversations: Vec<ConversationDto> = value.get("conversations")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|conv| {
                    let id = parse_uuid_field(conv, "id")?;
                    let kind = conv.get("kind")?.as_str()?.to_string();
                    let owner_id = parse_uuid_field(conv, "owner_id")?;
                    let created_at = conv.get("created_at")?.as_i64()?;

                    // title può essere null per DM
                    let title = conv.get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();

                    Some(ConversationDto {
                        id,
                        kind,
                        title,
                        owner_id,
                        created_at,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let user_sequence = value.get("user_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    info!("Received initial state with {} conversations, user_seq: {}",
          conversations.len(), user_sequence);

    // Invia le conversazioni al sistema eventi
    let _ = tx.send(UiEvent::InitialStateReceived {
        conversations,
        user_sequence,
    });

    // Processa l'ultimo messaggio per ogni conversazione
    if let Some(convs) = value.get("conversations").and_then(|c| c.as_array()) {
        for conv in convs {
            if let Some(last_msg) = conv.get("last_message") {
                if let Some(conv_id) = parse_uuid_field(conv, "id") {
                    process_last_message(tx, conv_id, last_msg);
                }
            }
        }
    }
}

/// Process last message from conversation
/// Process last message from conversation
fn process_last_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, conversation_id: Uuid, msg: &Value) {
    let content = msg.get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let author_username = msg.get("author_username")
        .and_then(|a| a.as_str())
        .unwrap_or("unknown")
        .to_string();

    // IMPORTANTE: Estrai anche l'author_id!
    let author_id = msg.get("author_id")
        .and_then(|id| id.as_str())
        .and_then(|id_str| Uuid::parse_str(id_str).ok())
        .unwrap_or(Uuid::nil());

    let created_at = msg.get("created_at")
        .and_then(|t| t.as_i64())
        .unwrap_or(0);

    let sequence = msg.get("sequence")
        .and_then(|s| s.as_i64())
        .map(|s| s as u64);

    debug!("Last message for {}: \"{}\" by {} (author_id: {})",
           conversation_id,
           content.chars().take(50).collect::<String>(),
           author_username,
           author_id);

    // Crea un MessageDto con l'author_id corretto
    let last_msg_dto = MessageDto {
        id: Uuid::new_v4(),
        author_id, // USA L'AUTHOR_ID REALE!
        author_username,
        conversation_id,
        content,
        created_at,
        sequence_num: sequence,
    };

    // Invia come evento LastMessageUpdate
    let _ = tx.send(UiEvent::LastMessageUpdate {
        conversation_id,
        message: last_msg_dto,
    });
}

/// Handle conversation messages when opening a conversation
fn handle_conversation_messages(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let conversation_id = match parse_uuid_field(value, "conversation_id") {
        Some(id) => id,
        None => {
            warn!("Invalid conversation_id in conversation_messages");
            return;
        }
    };

    let messages: Vec<MessageDto> = value.get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| {
                    let id = parse_uuid_field(msg, "id")?;
                    let author_id = parse_uuid_field(msg, "author_id")?;
                    let author_username = msg.get("author_username")?.as_str()?.to_string();
                    let content = msg.get("content")?.as_str()?.to_string();
                    let created_at = msg.get("created_at")?.as_i64()?;
                    let sequence_num = msg.get("sequence_num").and_then(|s| s.as_i64()).map(|s| s as u64);

                    Some(MessageDto {
                        id,
                        author_id,
                        author_username,
                        conversation_id,
                        content,
                        created_at,
                        sequence_num,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let has_more = value.get("has_more")
        .and_then(|h| h.as_bool())
        .unwrap_or(false);

    info!("Received {} messages for conversation {} (has_more: {})",
          messages.len(), conversation_id, has_more);

    // Invia i messaggi al sistema eventi
    let _ = tx.send(UiEvent::ConversationMessagesReceived {
        conversation_id,
        messages,
        has_more,
    });
}

/// Handle simple pong (retrocompatibilità)
fn handle_simple_pong(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let current_user_sequence = value.get("current_user_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    let message = value.get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("");

    debug!("Simple pong received - user_seq: {}, message: {}", current_user_sequence, message);

    // Converti in formato enhanced per uniformità
    let _ = tx.send(UiEvent::EnhancedPongReceived {
        current_user_sequence,
        conversation_sequences: None,
        gaps_detected: false,
        user_events_gap: None,
        message_gap: None,
    });
}

/// Handle server heartbeat
fn handle_server_heartbeat(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let user_id = value.get("user_id")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown");

    let timestamp = value.get("timestamp")
        .and_then(|t| t.as_i64())
        .unwrap_or(0);

    debug!("Server heartbeat received - user: {}, timestamp: {}", user_id, timestamp);
}

/// Handle user channel ready notification
fn handle_user_channel_ready(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let user_id = value.get("user_id")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown");

    info!("User channel ready notification received for user {}", user_id);
}

/// Handle fetch conversation messages event
fn handle_fetch_conversation_messages(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let conversation_id = match parse_conversation_id(value) {
        Some(id) => id,
        None => {
            warn!("Invalid conversation_id in fetch_conversation_messages event");
            return;
        }
    };

    let message_count = value.get("message_count")
        .and_then(|c| c.as_i64())
        .unwrap_or(0);

    let current_sequence = value.get("current_sequence")
        .and_then(|s| s.as_i64())
        .unwrap_or(0);

    let reason = value.get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("unknown");

    info!("Fetch event for conversation {} ({} messages, seq: {}, reason: {})",
          conversation_id, message_count, current_sequence, reason);

    let _ = tx.send(UiEvent::TriggerConversationFetch(
        conversation_id,
        format!("fetch_event_{}", reason)
    ));
}

/// Handle enhanced pong with sequences
fn handle_enhanced_pong(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let current_user_sequence = value.get("current_user_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    let conversation_sequences = value.get("conversation_sequences")
        .and_then(|s| s.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_u64().map(|seq| (k.clone(), seq)))
                .collect::<HashMap<String, u64>>()
        });

    let gaps_detected = value.get("gaps_detected")
        .and_then(|g| g.as_bool())
        .unwrap_or(false);

    let user_events_gap = value.get("user_events_gap")
        .and_then(|gap| parse_gap_info(gap));

    let message_gap = value.get("message_gap")
        .and_then(|gap| parse_gap_info(gap));

    debug!("Enhanced pong - user_seq: {}, gaps: {}", current_user_sequence, gaps_detected);

    let _ = tx.send(UiEvent::EnhancedPongReceived {
        current_user_sequence,
        conversation_sequences,
        gaps_detected,
        user_events_gap,
        message_gap,
    });
}

fn parse_gap_info(value: &Value) -> Option<GapInfo> {
    let detected = value.get("detected")?.as_bool()?;
    let client_seq = value.get("client_seq")?.as_u64()?;
    let server_seq = value.get("server_seq")?.as_u64()?;
    let gap_size = value.get("gap_size")?.as_u64()?;

    Some(GapInfo {
        detected,
        client_seq,
        server_seq,
        gap_size,
    })
}

/// Handle user events resume
fn handle_user_events_resume(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let events = value.get("events")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|event| {
                    let sequence = event.get("sequence")?.as_u64()?;
                    let event_type = event.get("event_type")?.as_str()?.to_string();
                    let created_at = event.get("created_at")?.as_i64()?;
                    let conversation_id = event.get("conversation_id")
                        .and_then(|c| c.as_str())
                        .and_then(|s| Uuid::parse_str(s).ok());

                    Some(UserEventData {
                        sequence,
                        event_type,
                        event_data: event.get("event_data").unwrap_or(&Value::Null).clone(),
                        conversation_id,
                        created_at,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let count = value.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    info!("Received user events resume with {} events", count);

    let _ = tx.send(UiEvent::UserEventsResume { events });
}

/// Handle messages resume
fn handle_messages_resume(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let conversation_id = match value.get("conversation_id")
        .and_then(|c| c.as_str())
        .and_then(|s| Uuid::parse_str(s).ok()) {
        Some(id) => id,
        None => {
            warn!("Messages resume missing valid conversation_id");
            return;
        }
    };

    let messages: Vec<MessageDto> = value.get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| {
                    let id = parse_uuid_field(msg, "id")?;
                    let author_id = parse_uuid_field(msg, "author_id")?;
                    let author_username = msg.get("author_username")?.as_str()?.to_string();
                    let content = msg.get("content")?.as_str()?.to_string();
                    let created_at = msg.get("created_at")?.as_i64()?;
                    let sequence_num = msg.get("sequence_num").and_then(|s| s.as_u64());

                    Some(MessageDto {
                        id,
                        author_id,
                        author_username,
                        conversation_id,
                        content,
                        created_at,
                        sequence_num,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let count = value.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    info!("Received messages resume for {} with {} messages", conversation_id, count);

    let _ = tx.send(UiEvent::MessagesResume { conversation_id, messages });
}

/// Handle resume complete notifications
fn handle_resume_complete(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value, resume_type: &str) {
    let from_sequence = value.get("from_sequence").and_then(|s| s.as_u64()).unwrap_or(0);
    let current_sequence = value.get("current_sequence").and_then(|s| s.as_u64()).unwrap_or(0);
    let count = value.get("events_count")
        .or_else(|| value.get("messages_count"))
        .and_then(|c| c.as_u64())
        .unwrap_or(0);

    debug!("{} resume complete - from: {}, current: {}, count: {}",
           resume_type, from_sequence, current_sequence, count);

    if count == 0 {
        let _ = tx.send(UiEvent::Info(format!("No {} to resume", resume_type)));
    }
}

/// Handle chat messages with sequence
fn handle_chat_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let id = match parse_uuid_field(value, "id") {
        Some(id) => id,
        None => {
            warn!("Invalid or missing message ID");
            return;
        }
    };

    let author_id = match parse_uuid_field(value, "author_id") {
        Some(id) => id,
        None => {
            warn!("Invalid or missing author_id");
            return;
        }
    };

    let conversation_id = match parse_conversation_id(value) {
        Some(id) => id,
        None => {
            warn!("Invalid or missing conversation_id");
            return;
        }
    };

    let content = match value.get("content").and_then(|v| v.as_str()) {
        Some(c) if !c.trim().is_empty() => c.to_string(),
        _ => {
            warn!("Empty or missing content in message");
            return;
        }
    };

    let author_username = value.get("author_username")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let created_at = value.get("created_at")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    let sequence_num = value.get("sequence").and_then(|s| s.as_u64());

    let dto = MessageDto {
        id,
        author_id,
        author_username,
        conversation_id,
        content,
        created_at,
        sequence_num,
    };

    debug!("Received chat message with sequence {:?}", sequence_num);

    let _ = tx.send(UiEvent::WsIncoming(dto));
}

/// Handle conversation created events
fn handle_conversation_created(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let conversation_id = match parse_conversation_id(value) {
        Some(id) => id,
        None => {
            warn!("Invalid conversation_id in conversation_created event");
            return;
        }
    };

    let sequence = value.get("sequence").and_then(|s| s.as_u64());

    if let Some(seq) = sequence {
        let _ = tx.send(UiEvent::UserNotification {
            sequence: seq,
            event_type: "conversation_created".to_string(),
            event_data: value.clone(),
            conversation_id: Some(conversation_id),
            recovery: false,
        });
    } else {
        let _ = tx.send(UiEvent::ConversationCreated(conversation_id));
    }
}

fn handle_server_error(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let error_msg = value.get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Errore sconosciuto");

    let error_code = value.get("error_code")
        .and_then(|c| c.as_str())
        .unwrap_or("unknown");

    error!("Server error: {} (code: {})", error_msg, error_code);

    let _ = tx.send(UiEvent::Error(format!("Errore server: {}", error_msg)));
}

fn handle_message_ack(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    if let Some(client_msg_id) = value.get("client_msg_id") {
        debug!("Message acknowledged by server: {:?}", client_msg_id);
    }
}

fn handle_server_warning(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let warning_msg = value.get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Warning dal server");

    warn!("Server warning: {}", warning_msg);
    let _ = tx.send(UiEvent::Info(format!("Attenzione: {}", warning_msg)));
}

// Utility functions
fn parse_uuid_field(value: &Value, field_name: &str) -> Option<Uuid> {
    value.get(field_name)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn parse_conversation_id(value: &Value) -> Option<Uuid> {
    value.get("conversation_id")
        .or_else(|| value.get("cid"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn handle_conversation_deleted(
    tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>,
    value: &Value,
) {
    let cid_opt = value
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    match cid_opt {
        Some(conversation_id) => {
            debug!("Handling conversation_deleted for {}", conversation_id);

            let _ = tx.send(UiEvent::ConversationDeleted(conversation_id));
        }
        None => {
            warn!("conversation_deleted without valid conversation_id: {:?}", value);
        }
    }
}