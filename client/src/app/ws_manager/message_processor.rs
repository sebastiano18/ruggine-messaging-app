use crate::models::{Outgoing, UiEvent};
use crate::state::AppState;
use tracing::{debug, error, warn};

use super::connection_manager::ConnectionManager;
use super::rate_limiter::RateLimiter;

pub struct MessageProcessor;

impl MessageProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Processa tutti i messaggi in uscita
    pub fn process_outgoing_messages(&self, state: &mut AppState, rate_limiter: &mut RateLimiter) {
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
                error!(
                    "Too many outgoing messages in queue ({}), stopping processing",
                    processed_count
                );
                break;
            }

            if let Some(ref ws_ctrl) = state.ws_ctrl {
                match self.format_outgoing_message(&outgoing) {
                    Ok(json_msg) => {
                        debug!(
                            "Sending WebSocket message: {}",
                            json_msg.chars().take(200).collect::<String>()
                        );

                        match ws_ctrl.outgoing_tx.send(json_msg) {
                            Ok(_) => {
                                rate_limiter.increment_sent();
                            }
                            Err(e) => {
                                error!("Failed to send WebSocket message: {}", e);
                                let _ = state
                                    .ui_tx
                                    .send(UiEvent::Error("Connessione WebSocket persa".into()));
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
            Outgoing::ChatMessage {
                cid,
                content,
                target_username,
                client_msg_id,
            } => {
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

                // Includi client_msg_id per tracking conferma
                if let Some(ref msg_id) = client_msg_id {
                    json_obj["client_msg_id"] = serde_json::Value::String(msg_id.clone());
                    debug!("Including client_msg_id: {}", msg_id);
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

            Outgoing::EnhancedPing {
                user_sequence,
                conversation_sequence,
                active_conversation_id,
            } => {
                // SEMPRE invia type: "ping"
                let mut json_obj = serde_json::json!({
                    "type": "ping",
                    "timestamp": chrono::Utc::now().timestamp()
                });

                // Aggiungi user_sequence SOLO se presente
                if let Some(user_seq) = user_sequence {
                    json_obj["user_sequence"] = serde_json::json!(user_seq);
                }

                // Aggiungi conversation_sequence SOLO se presente e c'è una conversazione attiva
                if let Some(conv_seq) = conversation_sequence {
                    json_obj["conversation_sequence"] = serde_json::json!(conv_seq);
                }

                // Aggiungi active_conversation_id SOLO se presente
                if let Some(conv_id) = active_conversation_id {
                    json_obj["active_conversation_id"] = serde_json::json!(conv_id.to_string());
                }

                debug!(
                    "Formatted enhanced ping: user_seq={:?}, conv_seq={:?}, active_conv={:?}",
                    user_sequence, conversation_sequence, active_conversation_id
                );

                json_obj
            }

            Outgoing::RequestUserResume {
                from_sequence,
                limit,
            } => {
                serde_json::json!({
                    "type": "request_user_resume",
                    "from_sequence": from_sequence,
                    "limit": limit
                })
            }

            Outgoing::RequestMessagesResume {
                conversation_id,
                from_sequence,
                limit,
            } => {
                serde_json::json!({
                    "type": "request_messages_resume",
                    "conversation_id": conversation_id,
                    "from_sequence": from_sequence,
                    "limit": limit
                })
            }

            Outgoing::SequenceAck {
                user_sequence,
                conversation_sequences,
            } => {
                let mut json_obj = serde_json::json!({
                    "type": "sequence_ack",
                    "timestamp": chrono::Utc::now().timestamp()
                });

                if let Some(user_seq) = user_sequence {
                    json_obj["user_sequence"] = serde_json::json!(user_seq);
                }

                if let Some(conv_seqs) = conversation_sequences {
                    json_obj["conversation_sequences"] = serde_json::json!(conv_seqs);
                }

                json_obj
            }
        };

        serde_json::to_string(&json_obj).map_err(|e| format!("JSON serialization error: {}", e))
    }
}
