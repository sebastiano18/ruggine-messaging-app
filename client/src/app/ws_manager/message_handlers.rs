use crate::models::{MessageDto, UiEvent};
use serde_json::Value;
use uuid::Uuid;
use tracing::{debug, error, info, warn};

/// Handler principale per messaggi WebSocket in arrivo
pub fn handle_websocket_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, msg: String) {
    debug!("Received WebSocket message: {}",
           msg.chars().take(200).collect::<String>());

    // Validazione dimensione messaggio
    if msg.len() > 100_000 {
        error!("WebSocket message too large ({} bytes), ignoring", msg.len());
        let _ = tx.send(UiEvent::Error("Messaggio WebSocket troppo grande".into()));
        return;
    }

    // Parsing JSON
    let parsed_value: Value = match serde_json::from_str(&msg) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse WebSocket JSON: {} - Message: {}", e,
                   msg.chars().take(100).collect::<String>());
            let _ = tx.send(UiEvent::Error(format!("Errore parsing JSON: {}", e)));
            return;
        }
    };

    // Estrai tipo messaggio
    let msg_type = match parsed_value.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => {
            warn!("WebSocket message missing 'type' field: {}",
                  parsed_value.to_string().chars().take(200).collect::<String>());
            return;
        }
    };

    // Gestione eventi sequenziati universale
    if let Some(sequence) = parsed_value.get("sequence").and_then(|s| s.as_u64()) {
        debug!("Received sequenced event: seq={}, type={}", sequence, msg_type);

        // Invia sequence received event
        let _ = tx.send(UiEvent::SequenceReceived(sequence));

        // Check se è un evento recovery
        let is_recovery = parsed_value.get("recovery").and_then(|r| r.as_bool()).unwrap_or(false);

        // Per eventi sequenziati, usa il nuovo UserNotification event unificato
        if msg_type != "pong" && msg_type != "heartbeat_ack" {
            let conversation_id = parse_conversation_id(&parsed_value);

            let _ = tx.send(UiEvent::UserNotification {
                sequence,
                event_type: msg_type.to_string(),
                event_data: parsed_value.clone(),
                conversation_id,
                recovery: is_recovery,
            });

            // Per eventi recovery o user notifications, non processare oltre
            if is_recovery || msg_type.starts_with("conversation_") {
                return;
            }
        }
    }

    // Routing per tipo messaggio specifico
    match msg_type {
        "chat_message" => handle_chat_message(tx, &parsed_value),
        "pong" => handle_pong_message(tx, &parsed_value),
        "user_channel_ready" | "server_heartbeat" | "heartbeat_ack" => {
            debug!("Received control message: {}", msg_type);
        }
        "typing" => handle_typing_indicator(tx, &parsed_value),
        "error" => handle_server_error(tx, &parsed_value),
        "system" => handle_system_message(tx, &parsed_value),
        "user_joined" | "user_left" => handle_user_status(tx, &parsed_value, msg_type),
        "message_ack" => handle_message_ack(tx, &parsed_value),
        "sequence_reset" => handle_sequence_reset(tx, &parsed_value),
        "warning" => handle_server_warning(tx, &parsed_value),
        "conversation_created" | "fetch_conversation_messages" | "debug" => {
            handle_legacy_server_event(tx, &parsed_value, msg_type);
        }
        unknown => {
            warn!("Unknown WebSocket message type '{}', ignoring", unknown);
            debug!("Unknown message content: {}",
                   parsed_value.to_string().chars().take(500).collect::<String>());
        }
    }
}

/// Handler per messaggi chat
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
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("unknown")
        .to_string();

    let created_at = value.get("created_at")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    let content = if content.len() > 50000 {
        warn!("Message content too long ({} chars), truncating", content.len());
        content.chars().take(50000).collect()
    } else {
        content
    };

    let dto = MessageDto {
        id,
        author_id,
        author_username,
        conversation_id,
        content,
        created_at,
    };

    debug!("Parsed message: {} chars from {} in {}",
           dto.content.len(), dto.author_username, dto.conversation_id);

    let _ = tx.send(UiEvent::WsIncoming(dto));
}

/// Handler per messaggi pong
fn handle_pong_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let server_sequence = value.get("server_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    let gap_detected = value.get("gap_detected")
        .and_then(|g| g.as_bool())
        .unwrap_or(false);

    let events_recovered = if gap_detected {
        value.get("events_recovered").and_then(|e| e.as_u64()).map(|e| e as usize)
    } else {
        None
    };

    debug!("Pong received: server_seq={}, gap={}, recovered={:?}",
          server_sequence, gap_detected, events_recovered);

    let _ = tx.send(UiEvent::PongReceived {
        server_sequence,
        gap_detected,
        events_recovered,
    });
}

/// Handler per eventi legacy del server senza sequence
fn handle_legacy_server_event(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value, event_type: &str) {
    match event_type {
        "fetch_conversation_messages" => {
            if let Some(conversation_id) = parse_conversation_id(value) {
                let reason = value.get("reason")
                    .and_then(|r| r.as_str())
                    .unwrap_or("server_request");

                info!("Received fetch request for conversation {} (reason: {})", conversation_id, reason);
                let _ = tx.send(UiEvent::TriggerConversationFetch(conversation_id, reason.to_string()));
            }
        }
        "conversation_created" => {
            if let Some(conversation_id) = parse_conversation_id(value) {
                info!("Received legacy conversation_created for {}", conversation_id);
                let _ = tx.send(UiEvent::ConversationCreated(conversation_id));
            }
        }
        "debug" => {
            let debug_msg = value.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Debug message from server");
            debug!("Server debug message: {}", debug_msg);
        }
        _ => {
            warn!("Unknown legacy server event: {}", event_type);
        }
    }
}

fn handle_typing_indicator(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    debug!("Received typing indicator: {}",
           value.to_string().chars().take(100).collect::<String>());
}

fn handle_server_error(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let error_msg = value.get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Errore sconosciuto");

    let error_code = value.get("error_code")
        .and_then(|c| c.as_str())
        .unwrap_or("unknown");

    error!("Server error: {} (code: {})", error_msg, error_code);

    let user_msg = match error_code {
        "RATE_LIMIT" => "Troppi messaggi inviati, rallenta",
        "FORBIDDEN" => "Operazione non autorizzata",
        "NOT_FOUND" => "Conversazione non trovata",
        "SEQUENCE_ERROR" => "Errore di sincronizzazione",
        _ => error_msg
    };

    let _ = tx.send(UiEvent::Error(format!("Errore: {}", user_msg)));
}

fn handle_system_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let system_msg = value.get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Messaggio di sistema");

    info!("System message: {}", system_msg);
    let _ = tx.send(UiEvent::Info(format!("Sistema: {}", system_msg)));
}

fn handle_user_status(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value, status_type: &str) {
    let username = value.get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("unknown");

    let status_msg = match status_type {
        "user_joined" => format!("{} si è unito", username),
        "user_left" => format!("{} ha lasciato la chat", username),
        _ => format!("Stato utente cambiato: {}", username)
    };

    debug!("User status change: {}", status_msg);
    let _ = tx.send(UiEvent::Info(status_msg));
}

fn handle_message_ack(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    if let Some(client_msg_id) = value.get("client_msg_id") {
        debug!("Message acknowledged by server: {:?}", client_msg_id);
    }
}

fn handle_sequence_reset(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let reason = value.get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("unknown");

    let new_sequence = value.get("new_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    warn!("Server requested sequence reset: reason={}, new_seq={}", reason, new_sequence);

    let _ = tx.send(UiEvent::Error(format!(
        "Sincronizzazione resettata dal server: {}", reason
    )));

    if new_sequence > 0 {
        let _ = tx.send(UiEvent::SequenceReceived(new_sequence));
    }
}

fn handle_server_warning(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
    let warning_msg = value.get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("Warning dal server");

    warn!("Server warning: {}", warning_msg);
    let _ = tx.send(UiEvent::Info(format!("Warning: {}", warning_msg)));
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