// ws_manager.rs - Updated to remove new_conversation_available

use crate::state::AppState;
use serde_json::Value;
use uuid::Uuid;
use tracing::{warn, error, debug, info};
use crate::models::{MessageDto, Outgoing, UiEvent, WsStatus};

pub struct WebSocketManager;

impl WebSocketManager {
    pub fn new() -> Self {
        Self
    }

    pub fn ensure_ws_lifecycle(&mut self, state: &mut AppState) {
        self.process_outgoing_messages(state);

        if state.token.is_none() {
            if state.ws_status != WsStatus::Disconnected {
                state.ws_status = WsStatus::Disconnected;
                if let Some(ctrl) = state.ws_ctrl.take() {
                    let _ = ctrl.shutdown.send(());
                }
            }
            return;
        }

        if state.request_ws_reconnect && state.ws_status != WsStatus::Connecting {
            state.request_ws_reconnect = false;
            state.ws_status = WsStatus::Disconnected;
            if let Some(ctrl) = state.ws_ctrl.take() {
                let _ = ctrl.shutdown.send(());
            }
        }

        match state.ws_status {
            WsStatus::Disconnected => self.start_websocket_connection(state),
            WsStatus::Connecting | WsStatus::Connected => { /* no-op */ }
        }
    }

    fn process_outgoing_messages(&self, state: &mut AppState) {
        let mut processed_count = 0;

        while let Ok(outgoing) = state.ui_to_net_rx.try_recv() {
            processed_count += 1;
            if processed_count > 100 {
                warn!("Too many outgoing messages in queue, stopping processing");
                break;
            }

            if let Some(ref ws_ctrl) = state.ws_ctrl {
                let json_msg = match &outgoing {
                    Outgoing::ChatMessage { cid, content, target_username } => {
                        if content.trim().is_empty() {
                            warn!("Attempted to send empty message, skipping");
                            continue;
                        }
                        if content.len() > 10000 {
                            warn!("Message too long ({} chars), truncating", content.len());
                            let truncated = content.chars().take(10000).collect::<String>();
                            let mut json_obj = serde_json::json!({
                                "type": "chat_message",
                                "cid": cid,
                                "content": truncated
                            });
                            if let Some(ref username) = target_username {
                                json_obj["target_username"] = serde_json::Value::String(username.clone());
                            }
                            json_obj.to_string()
                        } else {
                            let mut json_obj = serde_json::json!({
                                "type": "chat_message",
                                "cid": cid,
                                "content": content
                            });
                            if let Some(ref username) = target_username {
                                json_obj["target_username"] = serde_json::Value::String(username.clone());
                            }
                            json_obj.to_string()
                        }
                    }
                    Outgoing::InviteUser { cid, username } => {
                        if username.trim().is_empty() {
                            warn!("Attempted to invite user with empty username, skipping");
                            continue;
                        }
                        serde_json::json!({
                            "type": "invite_user",
                            "cid": cid,
                            "username": username.trim()
                        }).to_string()
                    }
                    Outgoing::Typing { cid, is_typing } => {
                        serde_json::json!({
                            "type": "typing",
                            "cid": cid,
                            "is_typing": is_typing
                        }).to_string()
                    }
                };

                debug!("Sending WebSocket message: {}",
                       json_msg.chars().take(200).collect::<String>());

                if let Err(_) = ws_ctrl.outgoing_tx.send(json_msg) {
                    warn!("WebSocket channel closed, marking as disconnected");
                    let _ = state.ui_tx.send(UiEvent::WsDisconnected);
                    break;
                }
            } else {
                warn!("Attempted to send message but WebSocket not connected: {:?}", outgoing);
            }
        }

        if processed_count > 0 {
            debug!("Processed {} outgoing WebSocket messages", processed_count);
        }
    }

    fn start_websocket_connection(&mut self, state: &mut AppState) {
        let base = state.base.clone();
        let token = match state.token.clone() {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                error!("Cannot start WebSocket: invalid token");
                let _ = state.ui_tx.send(UiEvent::WsError("Token non valido".into()));
                return;
            }
        };
        let tx = state.ui_tx.clone();

        state.ws_status = WsStatus::Connecting;
        info!("Starting WebSocket connection to {}", base);

        state.rt.spawn(async move {
            match crate::api::ws::connect(&base, &token).await {
                Ok(mut ws) => {
                    debug!("WebSocket connected, sending subscribe message");

                    match crate::api::ws::subscribe(&mut ws).await {
                        Ok(_) => {
                            info!("WebSocket subscribed successfully");
                            let _ = tx.send(UiEvent::WsConnected);
                            let tx_reader = tx.clone();

                            let ctrl = crate::api::ws::spawn_bidirectional_handler(ws, move |msg| {
                                Self::handle_websocket_message(&tx_reader, msg);
                            });

                            let _ = tx.send(UiEvent::WsControlReady(ctrl));
                        }
                        Err(e) => {
                            error!("WebSocket subscribe failed: {}", e);
                            let _ = tx.send(UiEvent::WsError(format!("Sottoscrizione WebSocket fallita: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket connection failed: {}", e);
                    let _ = tx.send(UiEvent::WsError(format!("Connessione WebSocket fallita: {}", e)));
                }
            }
        });
    }

    fn handle_websocket_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, msg: String) {
        debug!("Received WebSocket message: {}",
               msg.chars().take(200).collect::<String>());

        if msg.len() > 100_000 {
            error!("WebSocket message too large ({} bytes), ignoring", msg.len());
            let _ = tx.send(UiEvent::Error("Messaggio WebSocket troppo grande".into()));
            return;
        }

        let parsed_value: Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse WebSocket message as JSON: {} - Message: {}", e,
                       msg.chars().take(100).collect::<String>());
                return;
            }
        };

        let msg_type = match parsed_value.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => {
                warn!("WebSocket message missing 'type' field: {}",
                      parsed_value.to_string().chars().take(200).collect::<String>());
                return;
            }
        };

        match msg_type {
            "chat_message" => {
                Self::handle_chat_message(tx, &parsed_value);
            }
            "fetch_conversation_messages" => {
                Self::handle_fetch_request(tx, &parsed_value);
            }
            "conversation_added" => {
                Self::handle_conversation_added(tx, &parsed_value);
            }
            "typing" => {
                Self::handle_typing_indicator(tx, &parsed_value);
            }
            "error" => {
                Self::handle_server_error(tx, &parsed_value);
            }
            "heartbeat_ack" | "server_heartbeat" | "pong" | "user_channel_ready" => {
                debug!("Received heartbeat/control message from server");
            }
            "system" => {
                Self::handle_system_message(tx, &parsed_value);
            }
            "user_joined" | "user_left" => {
                Self::handle_user_status(tx, &parsed_value, msg_type);
            }
            "conversation_updated" => {
                Self::handle_conversation_update(tx, &parsed_value);
            }
            // REMOVED: new_conversation_available handler
            unknown => {
                warn!("Unknown WebSocket message type '{}', ignoring", unknown);
                debug!("Unknown message content: {}",
                       parsed_value.to_string().chars().take(500).collect::<String>());
            }
        }
    }

    fn handle_conversation_added(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
        let conversation_id = match Self::parse_conversation_id(value) {
            Some(id) => id,
            None => {
                warn!("Invalid conversation_id in conversation_added event");
                return;
            }
        };

        let reason = value.get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown");

        info!("Received conversation_added event for conversation {} (reason: {})", conversation_id, reason);

        // Use fetch_conversation_messages for consistency
        let _ = tx.send(UiEvent::FetchConversationMessages(conversation_id, reason.to_string()));
        let _ = tx.send(UiEvent::ConversationAdded(conversation_id, reason.to_string()));
    }

    fn handle_fetch_request(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
        let conversation_id = match Self::parse_conversation_id(value) {
            Some(id) => id,
            None => {
                warn!("Invalid conversation_id in fetch request");
                return;
            }
        };

        let reason = value.get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown");

        info!("Received fetch request for conversation {} (reason: {})", conversation_id, reason);
        let _ = tx.send(UiEvent::FetchConversationMessages(conversation_id, reason.to_string()));
    }

    fn handle_chat_message(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
        let id = match Self::parse_uuid_field(value, "id") {
            Some(id) => id,
            None => {
                warn!("Invalid or missing message ID in WebSocket payload");
                return;
            }
        };

        let author_id = match Self::parse_uuid_field(value, "author_id") {
            Some(id) => id,
            None => {
                warn!("Invalid or missing author_id in WebSocket payload");
                return;
            }
        };

        let conversation_id = match Self::parse_conversation_id(value) {
            Some(id) => id,
            None => {
                warn!("Invalid or missing conversation_id in WebSocket payload");
                return;
            }
        };

        let content = match value.get("content").and_then(|v| v.as_str()) {
            Some(c) if !c.trim().is_empty() => c.to_string(),
            _ => {
                warn!("Empty or missing content in WebSocket message");
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

        let content = if content.len() > 10000 {
            warn!("Message content too long ({} chars), truncating", content.len());
            content.chars().take(10000).collect()
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

        debug!("Parsed message DTO: {} chars from {} in {}",
               dto.content.len(), dto.author_username, dto.conversation_id);
        let _ = tx.send(UiEvent::WsIncoming(dto));
    }

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

    fn handle_typing_indicator(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
        debug!("Received typing indicator: {}",
               value.to_string().chars().take(100).collect::<String>());
    }

    fn handle_server_error(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
        let error_msg = value.get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Errore WebSocket sconosciuto");

        let error_code = value.get("code")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");

        error!("Server error received: {} (code: {})", error_msg, error_code);
        let _ = tx.send(UiEvent::Error(format!("Server ({}): {}", error_code, error_msg)));
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

        let conversation_id = value.get("conversation_id")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown");

        let status_msg = match status_type {
            "user_joined" => format!("{} si è unito alla conversazione", username),
            "user_left" => format!("{} ha lasciato la conversazione", username),
            _ => format!("Stato utente cambiato: {}", username)
        };

        debug!("User status change in {}: {}", conversation_id, status_msg);
        let _ = tx.send(UiEvent::Info(status_msg));
    }

    fn handle_conversation_update(tx: &tokio::sync::mpsc::UnboundedSender<UiEvent>, value: &Value) {
        debug!("Conversation update received: {}",
               value.to_string().chars().take(200).collect::<String>());
        let _ = tx.send(UiEvent::Info("Conversazione aggiornata".into()));
    }
}