use crate::models::{ConversationDto, GapInfo, MessageDto, ParticipantInfo, UiEvent, UserEventData};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Handler principale per messaggi WebSocket in arrivo
pub fn handle_websocket_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, msg: String) {
    debug!(
        "Received WebSocket message: {}",
        msg.chars().take(200).collect::<String>()
    );

    if msg.len() > 100_000 {
        error!(
            "WebSocket message too large ({} bytes), ignoring",
            msg.len()
        );
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
        "message" => handle_new_message_event(tx, &parsed_value), // Messaggi broadcast (inclusi quelli di sistema)
        "new_message" => handle_new_message_event(tx, &parsed_value),
        "initial_state" => handle_initial_state(tx, &parsed_value),
        "conversation_messages" => handle_conversation_messages(tx, &parsed_value),
        "conversation_created_complete" => handle_conversation_created_complete(tx, &parsed_value),
        "user_notification" => handle_user_notification(tx, &parsed_value),
        "user_event" => handle_user_notification(tx, &parsed_value),  // Gestito come user_notification
        "pong" => handle_pong(tx, &parsed_value),
        "server_heartbeat" => handle_server_heartbeat(tx, &parsed_value),
        "user_channel_ready" => handle_user_channel_ready(tx, &parsed_value),
        "fetch_conversation_messages" => handle_fetch_conversation_messages(tx, &parsed_value),
        "user_events_resume" => handle_user_events_resume(tx, &parsed_value),
        "messages_resume" => handle_messages_resume(tx, &parsed_value),
        "message_confirmation" => handle_message_confirmation(tx, &parsed_value),
        "user_resume_complete" => handle_resume_complete(tx, &parsed_value, "user"),
        "messages_resume_complete" => handle_resume_complete(tx, &parsed_value, "messages"),
        "conversation_created" => handle_conversation_created(tx, &parsed_value),
        "conversation_deleted" => handle_conversation_deleted(tx, &parsed_value),
        "member_added" => handle_member_added(tx, &parsed_value),
        "leave_group_ack" => handle_leave_group_ack(tx, &parsed_value),
        "error" => handle_server_error(tx, &parsed_value),
        "message_ack" => handle_message_ack(tx, &parsed_value),
        "warning" => handle_server_warning(tx, &parsed_value),
        "message_deleted" => handle_message_deleted(tx, &parsed_value),
        "check_user_response" => handle_check_user_response(tx, &parsed_value),
        "account_deleted_confirm" => handle_account_deleted_confirm(tx, &parsed_value),
        _ => {
            debug!("Unhandled message type: {}", msg_type);
        }
    }
}

fn handle_check_user_response(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let username = value
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let exists = value
        .get("exists")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let user_id = value
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let request_id = value
        .get("request_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    info!(
        "User check response: username='{}', exists={}, user_id={:?}, request_id={}",
        username, exists, user_id, request_id
    );

    let _ = tx.send(UiEvent::UserCheckResult {
        username,
        exists,
        user_id,
        request_id,
    });
}

fn handle_message_confirmation(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let client_msg_id = value
        .get("client_msg_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let server_msg_id = value
        .get("server_msg_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    let sequence = value.get("sequence").and_then(|v| v.as_u64());

    let status = value
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    if let (Some(client_id), Some(server_id)) = (client_msg_id, server_msg_id) {
        info!(
            "Message confirmation: client_id={}, server_id={}, seq={:?}, status={}",
            client_id, server_id, sequence, status
        );

        let _ = tx.send(UiEvent::MessageConfirmation {
            client_msg_id: client_id,
            server_msg_id: server_id,
            sequence,
            status: status.to_string(),
        });
    } else {
        warn!("Invalid message_confirmation: missing required fields");
    }
}

/// Handle conversation_created_complete con tutti i dati
fn handle_conversation_created_complete(
    tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>,
    value: &Value,
) {
    // Estrai la sequenza dell'evento se presente
    let sequence = value.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0);

    // Estrai i dati della conversazione
    let conversation_opt = value.get("conversation").and_then(|conv_obj| {
        let id = parse_uuid_field(conv_obj, "id")?;
        let kind = conv_obj.get("kind")?.as_str()?.to_string();
        let owner_id = parse_uuid_field(conv_obj, "owner_id")?;
        let created_at = conv_obj.get("created_at")?.as_i64()?;
        let last_read_sequence = conv_obj
            .get("last_read_sequence")
            .and_then(|s| s.as_i64())
            .unwrap_or(0);


        // Usa display_title per DM, altrimenti usa title normale
        let title = conv_obj
            .get("display_title")
            .and_then(|t| t.as_str())
            .or_else(|| conv_obj.get("title").and_then(|t| t.as_str()))
            .unwrap_or("")
            .to_string();

        // Calcola last_activity
        let last_message_time = conv_obj
            .get("last_message")
            .and_then(|msg| msg.get("created_at"))
            .and_then(|t| t.as_i64())
            .unwrap_or(0);

        let last_activity = std::cmp::max(created_at, last_message_time);


        let last_msg_seq = conv_obj
            .get("last_message")
            .and_then(|msg| msg.get("sequence_num"))
            .and_then(|t| t.as_i64())
            .unwrap_or(0);

        Some(ConversationDto {
            id,
            kind,
            title,
            owner_id,
            created_at,
            last_read_sequence,
            last_activity,
            last_msg_seq,
        })
    });

    let conversation = match conversation_opt {
        Some(c) => c,
        None => {
            warn!("conversation_created_complete missing valid conversation object");
            return;
        }
    };

    info!(
        "Received complete conversation: {} ({}) - seq: {}",
        conversation.id, conversation.title, sequence
    );

    // Se ha una sequenza, usa il sistema di notifiche per aggiornare le sequenze
    if sequence > 0 {
        let _ = tx.send(UiEvent::UserNotification {
            sequence,
            event_type: "conversation_created_complete".to_string(),
            event_data: value.clone(),
            conversation_id: Some(conversation.id),
            recovery: false,
        });
    } else {
        // Altrimenti gestisci direttamente
        // Estrai messaggi se presenti
        let mut messages = Vec::new();
        if let Some(last_msg) = value
            .get("conversation")
            .and_then(|c| c.get("last_message"))
        {
            if let Some(msg) = parse_message_from_json(last_msg, conversation.id) {
                messages.push(msg);
            }
        }

        let _ = tx.send(UiEvent::ConversationCompleteFetched(conversation, messages));
    }
}

/// Helper per parsare un messaggio da JSON
fn parse_message_from_json(value: &Value, conversation_id: Uuid) -> Option<MessageDto> {
    let id = parse_uuid_field(value, "id")?;
    let author_id = parse_uuid_field(value, "author_id")?;
    let author_username = value.get("author_username")?.as_str()?.to_string();
    let content = value.get("content")?.as_str()?.to_string();
    let created_at = value.get("created_at")?.as_i64()?;
    let sequence_num = value
        .get("sequence_num")
        .or_else(|| value.get("sequence")) // Supporta entrambi i nomi
        .and_then(|s| s.as_u64());

    // IMPORTANTE: Estrai client_msg_id se presente
    let client_msg_id = value
        .get("client_msg_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // LOG per debug
    if client_msg_id.is_some() {
        info!(
            "Parsed message {} with client_msg_id: {:?}",
            id, client_msg_id
        );
    }

    Some(MessageDto {
        id,
        author_id,
        author_username,
        conversation_id,
        content,
        created_at,
        sequence_num,
        client_msg_id,
        is_confirmed: Some(true),
    })
}


fn handle_initial_state(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {

    let conversations: Vec<ConversationDto> = value
        .get("conversations")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|conv| {
                    let id = parse_uuid_field(conv, "id")?;
                    let kind = conv.get("kind")?.as_str()?.to_string();
                    let owner_id = parse_uuid_field(conv, "owner_id")?;
                    let created_at = conv.get("created_at")?.as_i64()?;

                    let title = conv
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();

                    let last_read_sequence = conv
                        .get("last_read_sequence")
                        .and_then(|s| s.as_i64())
                        .unwrap_or(0);

                    let last_message_time = conv
                        .get("last_message")
                        .and_then(|msg| msg.get("created_at"))
                        .and_then(|t| t.as_i64())
                        .unwrap_or(0);

                    let last_activity = std::cmp::max(created_at, last_message_time);

                    let last_msg_seq = conv
                        .get("last_message")
                        .and_then(|msg| msg.get("sequence_num"))
                        .and_then(|t| t.as_i64())
                        .unwrap_or(0);

                    Some(ConversationDto {
                        id,
                        kind,
                        title,
                        owner_id,
                        created_at,
                        last_read_sequence,
                        last_activity,
                        last_msg_seq,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut conversations = conversations;
    conversations.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

    let user_sequence = value
        .get("user_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    // Parsing dei membri per conversazione
    let members_by_conversation: Option<std::collections::HashMap<Uuid, Vec<ParticipantInfo>>> = value
        .get("members_by_conversation")
        .and_then(|m| m.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(conv_id_str, members_value)| {
                    // Parse conversation_id
                    let conv_id = Uuid::parse_str(conv_id_str).ok()?;

                    // Parse array di membri
                    let members = members_value
                        .as_array()?
                        .iter()
                        .filter_map(|member| {
                            let user_id = parse_uuid_field(member, "user_id")?;
                            let username = member.get("username")?.as_str()?.to_string();
                            let role = member.get("role")?.as_str()?.to_string();

                            Some(ParticipantInfo {
                                user_id,
                                username,
                                role,
                            })
                        })
                        .collect::<Vec<_>>();

                    Some((conv_id, members))
                })
                .collect()
        });

    info!(
        "Received initial state with {} conversations, user_seq: {}, members_map: {}",
        conversations.len(),
        user_sequence,
        if members_by_conversation.is_some() { "present" } else { "absent" }
    );

    let _ = tx.send(UiEvent::InitialStateReceived {
        conversations,
        user_sequence,
        members_by_conversation,
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


fn process_last_message(
    tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>,
    conversation_id: Uuid,
    msg: &Value,
) {
    let content = msg
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let author_username = msg
        .get("author_username")
        .and_then(|a| a.as_str())
        .unwrap_or("unknown")
        .to_string();

    let author_id = msg
        .get("author_id")
        .and_then(|id| id.as_str())
        .and_then(|id_str| Uuid::parse_str(id_str).ok())
        .unwrap_or(Uuid::nil());

    let created_at = msg.get("created_at").and_then(|t| t.as_i64()).unwrap_or(0);

    let sequence_num = msg
        .get("sequence_num")
        .and_then(|s| s.as_i64())
        .map(|s| s as u64);

    // ✅ AGGIUNTO: Estrae l'UUID dal server
    let message_id = msg
        .get("id")
        .and_then(|id| id.as_str())
        .and_then(|id_str| Uuid::parse_str(id_str).ok())
        .unwrap_or_else(|| {
            warn!("last_message without id, generating new UUID");
            Uuid::new_v4()
        });

    debug!(
        "Last message parsed with id: {} for conversation {}",
        message_id, conversation_id
    );

    let last_msg_dto = MessageDto {
        id: message_id, // ✅ MODIFICATO: Usa l'UUID dal server
        author_id,
        author_username,
        conversation_id,
        content,
        created_at,
        sequence_num,
        client_msg_id: None,
        is_confirmed: Some(true),
    };

    let _ = tx.send(UiEvent::LastMessageUpdate {
        conversation_id,
        message: last_msg_dto,
    });
}

// Tutte le altre funzioni esistenti rimangono invariate...
fn handle_conversation_messages(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let conversation_id = match parse_uuid_field(value, "conversation_id") {
        Some(id) => id,
        None => {
            warn!("Invalid conversation_id in conversation_messages");
            return;
        }
    };

    let messages: Vec<MessageDto> = value
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| parse_message_from_json(msg, conversation_id))
                .collect()
        })
        .unwrap_or_default();

    let has_more = value
        .get("has_more")
        .and_then(|h| h.as_bool())
        .unwrap_or(false);

    info!(
        "Received {} messages for conversation {} (has_more: {})",
        messages.len(),
        conversation_id,
        has_more
    );

    let _ = tx.send(UiEvent::ConversationMessagesReceived {
        conversation_id,
        messages,
        has_more,
    });
}

fn handle_pong(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let current_user_sequence = value
        .get("current_user_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);


    let gaps_detected = value
        .get("gaps_detected")
        .and_then(|g| g.as_bool())
        .unwrap_or(false);

    let user_events_gap = value
        .get("user_events_gap")
        .and_then(|gap| parse_gap_info(gap));


    debug!(
        "Pong - user_seq: {}, gaps: {}",
        current_user_sequence, gaps_detected
    );

    let _ = tx.send(UiEvent::PongReceived {
        current_user_sequence,
        gaps_detected,
        user_events_gap,
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

fn handle_server_heartbeat(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let user_id = value
        .get("user_id")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown");

    let timestamp = value.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0);

    debug!(
        "Server heartbeat received - user: {}, timestamp: {}",
        user_id, timestamp
    );
}

fn handle_user_channel_ready(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let user_id = value
        .get("user_id")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown");

    info!(
        "User channel ready notification received for user {}",
        user_id
    );
}

fn handle_fetch_conversation_messages(
    tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>,
    value: &Value,
) {
    let conversation_id = match parse_conversation_id(value) {
        Some(id) => id,
        None => {
            warn!("Invalid conversation_id in fetch_conversation_messages event");
            return;
        }
    };

    let message_count = value
        .get("message_count")
        .and_then(|c| c.as_i64())
        .unwrap_or(0);

    let current_sequence = value
        .get("current_sequence")
        .and_then(|s| s.as_i64())
        .unwrap_or(0);

    let reason = value
        .get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("unknown");

    info!(
        "Fetch event for conversation {} ({} messages, seq: {}, reason: {})",
        conversation_id, message_count, current_sequence, reason
    );

    let _ = tx.send(UiEvent::TriggerConversationFetch(
        conversation_id,
        format!("fetch_event_{}", reason),
    ));
}

fn handle_user_events_resume(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let events = value
        .get("events")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|event| {
                    let sequence = event.get("sequence")?.as_u64()?;
                    let event_type = event.get("event_type")?.as_str()?.to_string();
                    let created_at = event.get("created_at")?.as_i64()?;
                    let conversation_id = event
                        .get("conversation_id")
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

fn handle_messages_resume(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let conversation_id = match value
        .get("conversation_id")
        .and_then(|c| c.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        Some(id) => id,
        None => {
            warn!("Messages resume missing valid conversation_id");
            return;
        }
    };

    let messages: Vec<MessageDto> = value
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| parse_message_from_json(msg, conversation_id))
                .collect()
        })
        .unwrap_or_default();

    let count = value.get("count").and_then(|c| c.as_u64()).unwrap_or(0);
    info!(
        "Received messages resume for {} with {} messages",
        conversation_id, count
    );

    let _ = tx.send(UiEvent::MessagesResume {
        conversation_id,
        messages,
    });
}

fn handle_resume_complete(
    tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>,
    value: &Value,
    resume_type: &str,
) {
    let from_sequence = value
        .get("from_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);
    let current_sequence = value
        .get("current_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);
    let count = value
        .get("events_count")
        .or_else(|| value.get("messages_count"))
        .and_then(|c| c.as_u64())
        .unwrap_or(0);

    debug!(
        "{} resume complete - from: {}, current: {}, count: {}",
        resume_type, from_sequence, current_sequence, count
    );

    if count == 0 {
        let _ = tx.send(UiEvent::Info(format!("No {} to resume", resume_type)));
    }
}

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

    let author_username = value
        .get("author_username")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let created_at = value
        .get("created_at")
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
        client_msg_id: None,
        is_confirmed: Some(true),
    };

    debug!("Received chat message with sequence {:?}", sequence_num);

    let _ = tx.send(UiEvent::WsIncoming(dto));
}

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
    let error_msg = value
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Errore sconosciuto");

    let error_code = value
        .get("error_code")
        .and_then(|c| c.as_str())
        .unwrap_or("unknown");

    error!("Server error: {} (code: {})", error_msg, error_code);

    // Mappa gli errori del server ai tipi appropriati
    let error_type = match error_code {
        "auth_failed" | "invalid_token" | "token_expired" => {
            crate::models::ErrorType::Auth(error_msg.to_string())
        }
        "message_send_failed" => {
            crate::models::ErrorType::MessageSend
        }
        "message_delete_failed" => {
            crate::models::ErrorType::MessageDelete
        }
        "conversation_delete_failed" => {
            crate::models::ErrorType::ConversationDelete
        }
        "group_leave_failed" => {
            crate::models::ErrorType::GroupLeave
        }
        "group_create_failed" => {
            crate::models::ErrorType::GroupCreate
        }
        "invite_failed" => {
            crate::models::ErrorType::Invite
        }
        "connection_error" | "websocket_error" => {
            crate::models::ErrorType::Connection
        }
        _ => {
            // Errore generico con il messaggio del server
            crate::models::ErrorType::Generic(format!("Errore server: {}", error_msg))
        }
    };

    let _ = tx.send(UiEvent::Error(error_type));
}

fn handle_message_ack(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    if let Some(client_msg_id) = value.get("client_msg_id") {
        debug!("Message acknowledged by server: {:?}", client_msg_id);
    }
}

fn handle_server_warning(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let warning_msg = value
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Warning dal server");

    warn!("Server warning: {}", warning_msg);
    let _ = tx.send(UiEvent::Info(format!("Attenzione: {}", warning_msg)));
}

fn handle_conversation_deleted(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let cid_opt = value
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    match cid_opt {
        Some(conversation_id) => {
            debug!("Handling conversation_deleted for {}", conversation_id);

            // Se presente, inoltra anche come UserNotification per aggiornare la user_sequence
            if let Some(seq) = value.get("sequence").and_then(|s| s.as_u64()) {
                let _ = tx.send(UiEvent::UserNotification {
                    sequence: seq,
                    event_type: "conversation_deleted".to_string(),
                    event_data: value.clone(),
                    conversation_id: Some(conversation_id),
                    recovery: false,
                });
            } else {
                debug!("conversation_deleted arrived without sequence; not updating user_sequence");
            }

            let _ = tx.send(UiEvent::ConversationDeleted(conversation_id));
        }
        None => {
            warn!(
                "conversation_deleted without valid conversation_id: {:?}",
                value
            );
        }
    }
}

/// Gestisce l'evento di eliminazione di un messaggio
fn handle_message_deleted(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let message_id = match parse_uuid_field(value, "message_id") {
        Some(id) => id,
        None => {
            warn!("Invalid or missing message_id in message_deleted event");
            return;
        }
    };

    let conversation_id = match parse_uuid_field(value, "conversation_id") {
        Some(id) => id,
        None => {
            warn!("Invalid or missing conversation_id in message_deleted event");
            return;
        }
    };

    info!(
        "Handling message_deleted event for message {} in conversation {}",
        message_id, conversation_id
    );

    let _ = tx.send(UiEvent::MessageDeleted {
        message_id,
        conversation_id,
    });
}

/// Handler per eventi member_added (quando un utente viene aggiunto a un gruppo)
fn handle_member_added(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    info!("Received member_added event: {:?}", value);

    // Estrai conversation_id
    let conversation_id = match parse_conversation_id(value) {
        Some(id) => id,
        None => {
            warn!("member_added without valid conversation_id: {:?}", value);
            return;
        }
    };

    // Estrai informazioni sul membro aggiunto
    let username = value
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    let user_id = value
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    // Estrai chi ha aggiunto (opzionale)
    let added_by = value
        .get("added_by")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    info!(
        "User '{}' (id: {:?}) added to conversation {} by '{}'",
        username, user_id, conversation_id, added_by
    );

    // Gestisci SEMPRE tramite UserNotification (con o senza sequence)
    let sequence = value.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0);

    let _ = tx.send(UiEvent::UserNotification {
        sequence,
        event_type: "member_added".to_string(),
        event_data: value.clone(),
        conversation_id: Some(conversation_id),
        recovery: false,
    });

    // Mostra notifica all'utente
    let message = format!("{} è stato aggiunto al gruppo", username);
    let _ = tx.send(UiEvent::Info(message));
    
}

fn handle_leave_group_ack(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    if let Some(conversation_id) = parse_conversation_id(value) {
        debug!(
            "Received leave_group_ack for conversation {}",
            conversation_id
        );

        // Rimuovi la conversazione localmente
        let _ = tx.send(UiEvent::ConversationDeleted(conversation_id));
    } else {
        warn!("leave_group_ack without valid conversation_id: {:?}", value);
    }
}

// Utility functions
fn parse_uuid_field(value: &Value, field_name: &str) -> Option<Uuid> {
    value
        .get(field_name)
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn parse_conversation_id(value: &Value) -> Option<Uuid> {
    value
        .get("conversation_id")
        .or_else(|| value.get("cid"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Handler per messaggi user_notification dal server
fn handle_user_notification(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let sequence = value.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0);

    let event_type = value
        .get("event_type")
        .and_then(|t| t.as_str())
        .unwrap_or("unknown")
        .to_string();

    let event_data = value.get("event_data").cloned().unwrap_or(value.clone());

    let conversation_id = value
        .get("conversation_id")
        .and_then(|id| id.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    info!(
        "📬 Received user_notification: type={}, seq={}, conv={:?}",
        event_type, sequence, conversation_id
    );

    let _ = tx.send(UiEvent::UserNotification {
        sequence,
        event_type,
        event_data,
        conversation_id,
        recovery: false,
    });
}

/// 🆕 Handler per eventi new_message (real-time con user_sequence)
/// Riusa completamente la logica esistente di UserNotification
fn handle_new_message_event(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    // 1. Estrai user_sequence
    let sequence = value.get("sequence").and_then(|s| s.as_u64()).unwrap_or(0);

    // 2. Estrai conversation_id
    let conversation_id = value
        .get("conversation_id")
        .and_then(|id| id.as_str())
        .and_then(|id_str| Uuid::parse_str(id_str).ok());

    // 3. Estrai il messaggio dall'evento
    let message_data = match value.get("message") {
        Some(msg) => msg.clone(),
        None => {
            error!("new_message event without message data");
            return;
        }
    };

    info!(
        "Received new_message event: seq={}, conv={:?}",
        sequence, conversation_id
    );

    let _ = tx.send(UiEvent::UserNotification {
        sequence,
        event_type: "new_message".to_string(),
        event_data: message_data,
        conversation_id,
        recovery: false, // È un evento real-time, non recovery
    });

}

fn handle_account_deleted_confirm(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    info!("Account deletion confirmed by server - triggering immediate logout");

    if let Some(message) = value.get("message").and_then(|v| v.as_str()) {
        debug!("Server message: {}", message);
    }

    let _ = tx.send(UiEvent::LoggedOut);
}