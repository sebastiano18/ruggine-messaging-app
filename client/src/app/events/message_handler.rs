// events/message_handler.rs - Gestione messaggi
use super::sequence_handler::SequenceHandler;
use crate::models::*;
use crate::state::core::AppState;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct MessageHandler;

impl MessageHandler {
    pub fn handle_message_send_failed(state: &mut AppState, msg_id: Uuid) {
        warn!("Message send failed: {}", msg_id);

        // Trova il client_msg_id, conversazione E CONTENUTO del messaggio fallito
        let (client_msg_id, conversation_id, failed_content) = state.messages
            .iter()
            .find(|m| m.id == msg_id)
            .map(|m| (m.client_msg_id.clone(), m.conversation_id, m.content.clone()))
            .unwrap_or((None, uuid::Uuid::nil(), String::new()));

        // Verifica che abbiamo una conversazione valida
        if conversation_id.is_nil() {
            warn!("Cannot handle failed message: conversation_id is nil");
            return;
        }

        // Marca il messaggio come fallito nella UI
        let mut marked_in_ui = false;
        for msg in &mut state.messages {
            if msg.id == msg_id {
                msg.is_confirmed = Some(false); // false = fallito
                marked_in_ui = true;
                info!("Marked message {} as failed in UI", msg_id);
                break;
            }
        }

        // Marca anche nella cache
        let mut marked_in_cache = false;
        if let Some(messages) = state.conversation_messages.get_mut(&conversation_id) {
            for msg in messages.iter_mut() {
                if msg.id == msg_id {
                    msg.is_confirmed = Some(false); // false = fallito
                    marked_in_cache = true;
                    info!("Marked message {} as failed in cache", msg_id);
                    break;
                }
            }
        }

        // IMPORTANTE: Rimuovi da pending_confirmations per evitare memory leak
        if let Some(client_id) = client_msg_id {
            if let Some(_failed_msg) = state.pending_confirmations.remove(&client_id) {
                info!("Removed failed message {} from pending_confirmations", client_id);
            }
        }

        // Crea messaggio di errore con ANTEPRIMA del contenuto fallito
        let error_text = if !failed_content.is_empty() {
            // Mostra i primi 50 caratteri del messaggio fallito
            let preview = if failed_content.len() > 50 {
                format!("{}...", &failed_content[..50])
            } else {
                failed_content.clone()
            };
            format!("⚠️ Messaggio non inviato: \"{}\"", preview)
        } else {
            "⚠️ Messaggio non inviato - errore di connessione".to_string()
        };

        // Crea il messaggio di sistema manualmente con la conversation_id corretta
        let mut system_msg = MessageDto::system_message(error_text.clone());
        system_msg.conversation_id = conversation_id; // IMPORTANTE: Imposta la conversation_id corretta

        // Aggiungi alla UI se siamo nella conversazione giusta
        if state.cid == Some(conversation_id) {
            state.messages.push(system_msg.clone());
            info!("Added system error message to UI for conversation {}", conversation_id);
        }

        // Aggiungi sempre alla cache della conversazione
        if let Some(messages) = state.conversation_messages.get_mut(&conversation_id) {
            messages.push(system_msg);
            info!("Added system error message to cache for conversation {}", conversation_id);
        }

        info!(
            "Handled message send failure: msg_id={}, marked_ui={}, marked_cache={}, cid={}",
            msg_id, marked_in_ui, marked_in_cache, conversation_id
        );
    }

    pub fn handle_message_confirmation(
        state: &mut AppState,
        client_msg_id: String,
        server_msg_id: Uuid,
        sequence: Option<u64>,
        status: String,
    ) {
        info!("Processing confirmation for msg {}", client_msg_id);

        let already_exists = state.messages.iter().any(|m| m.id == server_msg_id);

        if already_exists {
            state
                .messages
                .retain(|m| m.client_msg_id.as_ref() != Some(&client_msg_id));
            state.pending_confirmations.remove(&client_msg_id);

            if let Some(cid) = state.cid {
                if let Some(cache) = state.conversation_messages.get_mut(&cid) {
                    cache.retain(|m| m.client_msg_id.as_ref() != Some(&client_msg_id));
                }
            }

            info!(
                "Message {} already received via resume, cleaned optimistic",
                server_msg_id
            );
            return;
        }

        if let Some(pending_msg) = state.pending_confirmations.remove(&client_msg_id) {
            let old_id = pending_msg.id;
            let conversation_id = pending_msg.conversation_id;

            let mut updated = false;
            for msg in &mut state.messages {
                if msg.client_msg_id.as_ref() == Some(&client_msg_id) {
                    msg.id = server_msg_id;
                    msg.sequence_num = sequence;
                    msg.is_confirmed = Some(true);
                    updated = true;
                    debug!("Updated UI msg {} -> {}", old_id, server_msg_id);
                    break;
                }
            }

            if !updated {
                debug!(
                    "Message {} not found in UI, likely processed via resume",
                    client_msg_id
                );
                return;
            }

            if let Some(cache) = state.conversation_messages.get_mut(&conversation_id) {
                for msg in cache.iter_mut() {
                    if msg.client_msg_id.as_ref() == Some(&client_msg_id) {
                        msg.id = server_msg_id;
                        msg.sequence_num = sequence;
                        msg.is_confirmed = Some(true);
                        break;
                    }
                }

                if sequence.is_some() {
                    cache.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                        (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                        _ => a.created_at.cmp(&b.created_at),
                    });
                }
            }

            if let Some(seq) = sequence {
                SequenceHandler::update_conversation_sequence(state, conversation_id, seq);
            }

            info!(
                "Message confirmed: {} -> {} (seq: {:?})",
                client_msg_id, server_msg_id, sequence
            );
        } else {
            warn!(
                "Received confirmation for unknown message: {}",
                client_msg_id
            );
        }
    }

    pub fn request_conversation_messages(state: &mut AppState, conversation_id: Uuid) {
        debug!(
            "Requesting messages for conversation {} via WebSocket",
            conversation_id
        );

        let request = serde_json::json!({
            "type": "open_conversation",
            "conversation_id": conversation_id.to_string()
        });

        if let Some(ref ws_ctrl) = state.ws_ctrl {
            if let Ok(json_str) = serde_json::to_string(&request) {
                let _ = ws_ctrl.outgoing_tx.send(json_str);

                crate::app::events::helpers::add_system_message(state, "Caricamento messaggi...".into());
            }
        } else {
            warn!("Cannot request messages: WebSocket not connected");
        }
    }
}