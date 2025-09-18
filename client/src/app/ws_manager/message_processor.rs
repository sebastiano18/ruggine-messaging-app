use crate::state::AppState;
use crate::models::{Outgoing, UiEvent};
use tracing::{debug, error, warn};

use super::rate_limiter::RateLimiter;
use super::connection_manager::ConnectionManager;

pub struct MessageProcessor;

impl MessageProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Processa tutti i messaggi in uscita
    pub fn process_outgoing_messages(
        &self,
        state: &mut AppState,
        rate_limiter: &mut RateLimiter
    ) {
        let mut processed_count = 0;

        // Rate limiting check
        if !rate_limiter.check_rate_limit() {
            debug!("Rate limit exceeded, skipping outgoing message processing");
            return;
        }

        while let Ok(outgoing) = state.ui_to_net_rx.try_recv() {
            processed_count += 1;

            // Protezione anti-flooding
            if processed_count > 200 {
                error!("Too many outgoing messages in queue ({}), stopping processing", processed_count);
                break;
            }

            if let Some(ref ws_ctrl) = state.ws_ctrl {
                match self.format_outgoing_message(&outgoing) {
                    Ok(json_msg) => {
                        debug!("Sending WebSocket message: {}",
                               json_msg.chars().take(200).collect::<String>());

                        match ws_ctrl.outgoing_tx.send(json_msg) {
                            Ok(_) => {
                                // Aggiorna statistiche tramite reference counting
                                // Note: in un refactor più completo, potresti passare connection_manager qui
                                rate_limiter.increment_sent();
                            }
                            Err(e) => {
                                error!("Failed to send WebSocket message: {}", e);
                                let _ = state.ui_tx.send(UiEvent::WsDisconnected);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to format outgoing message: {}", e);
                        continue;
                    }
                }
            } else {
                warn!("Attempted to send message but WebSocket not connected");
                // Re-queue il messaggio per quando ci riconnettiamo
                let _ = state.ui_to_net_tx.try_send(outgoing);
                break;
            }
        }

        if processed_count > 50 {
            debug!("Processed {} outgoing WebSocket messages", processed_count);
        }
    }

    /// Formatta i messaggi in uscita
    fn format_outgoing_message(&self, outgoing: &Outgoing) -> Result<String, String> {
        let json_obj = match outgoing {
            Outgoing::ChatMessage { cid, content, target_username } => {
                if content.trim().is_empty() {
                    return Err("Empty message content".into());
                }

                let content = if content.len() > 10000 {
                    warn!("Message too long ({} chars), truncating", content.len());
                    content.chars().take(10000).collect::<String>()
                } else {
                    content.clone()
                };

                let mut json_obj = serde_json::json!({
                    "type": "chat_message",
                    "cid": cid,
                    "content": content,
                    "client_timestamp": chrono::Utc::now().timestamp()
                });

                if let Some(ref username) = target_username {
                    json_obj["target_username"] = serde_json::Value::String(username.clone());
                }
                json_obj
            }

            Outgoing::InviteUser { cid, username } => {
                if username.trim().is_empty() {
                    return Err("Empty username".into());
                }
                serde_json::json!({
                    "type": "invite_user",
                    "cid": cid,
                    "username": username.trim()
                })
            }

            Outgoing::Typing { cid, is_typing } => {
                serde_json::json!({
                    "type": "typing",
                    "cid": cid,
                    "is_typing": is_typing
                })
            }

            Outgoing::Ping { last_sequence } => {
                serde_json::json!({
                    "type": "ping",
                    "last_sequence": last_sequence,
                    "timestamp": chrono::Utc::now().timestamp(),
                    "client_id": "ruggine_client"
                })
            }

            Outgoing::SequenceAck { sequence } => {
                serde_json::json!({
                    "type": "sequence_ack",
                    "sequence": sequence,
                    "timestamp": chrono::Utc::now().timestamp()
                })
            }
        };

        serde_json::to_string(&json_obj).map_err(|e| format!("JSON serialization error: {}", e))
    }
}