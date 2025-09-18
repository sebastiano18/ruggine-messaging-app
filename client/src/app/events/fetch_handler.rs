// events/fetch_handler.rs - Gestione fetch messaggi
use crate::models::{UiEvent, MessageDto};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

pub struct FetchHandler;

impl FetchHandler {
    /*pub fn handle(state: &mut crate::state::core::AppState, event: UiEvent) {
        match event {
            UiEvent::FetchConversationMessages(conversation_id, reason) => {
                Self::handle_fetch_request(state, conversation_id, reason);
            }
            UiEvent::FetchedMessages(conversation_id, messages) => {
                Self::handle_fetched_messages(state, conversation_id, messages);
            }
            _ => unreachable!("Invalid fetch event"),
        }
    }

    fn handle_fetch_request(state: &mut crate::state::core::AppState, conversation_id: Uuid, reason: String) {
        info!("Processing fetch request for conversation {} (reason: {})", conversation_id, reason);

        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::chat::fetch_conversation_messages(&base, &token, conversation_id, Some(50)).await {
                    Ok(messages) => {
                        info!("Successfully fetched {} messages for conversation {}", messages.len(), conversation_id);
                        let _ = tx.send(UiEvent::FetchedMessages(conversation_id, messages));
                    }
                    Err(e) => {
                        error!("Failed to fetch messages for conversation {}: {}", conversation_id, e);
                        let _ = tx.send(UiEvent::Error(format!("Errore nel caricamento messaggi: {}", e)));
                    }
                }
            });

            crate::app::events::helpers::add_system_message(state, format!("Sincronizzando messaggi... ({})", reason));
        } else {
            warn!("Cannot fetch messages: no authentication token");
        }
    }

    fn handle_fetched_messages(state: &mut crate::state::core::AppState, conversation_id: Uuid, mut messages: Vec<MessageDto>) {
        info!("Processing {} fetched messages for conversation {}", messages.len(), conversation_id);

        messages.retain(|msg| crate::app::events::helpers::validate_incoming_message(msg));
        crate::app::events::helpers::deduplicate_messages(&mut messages);
        messages.sort_by_key(|m| m.created_at);

        if messages.is_empty() {
            debug!("No valid messages to process for conversation {}", conversation_id);
            return;
        }

        let mut new_messages = Vec::new();
        {
            let conversation_cache = state
                .conversation_messages
                .entry(conversation_id)
                .or_insert_with(Vec::new);

            for msg in messages {
                if !conversation_cache.iter().any(|existing| existing.id == msg.id) {
                    let insert_pos = conversation_cache
                        .binary_search_by(|existing| {
                            existing
                                .created_at
                                .cmp(&msg.created_at)
                                .then_with(|| existing.id.cmp(&msg.id))
                        })
                        .unwrap_or_else(|pos| pos);

                    conversation_cache.insert(insert_pos, msg.clone());
                    new_messages.push(msg);
                }
            }
        }

        let new_messages_count = new_messages.len();

        if Some(conversation_id) == state.cid && !new_messages.is_empty() {
            for msg in new_messages {
                Self::update_ui_messages_improved(state, msg);
            }
        }

        if new_messages_count > 0 {
            info!("Added {} new messages to cache for conversation {}", new_messages_count, conversation_id);

            if Some(conversation_id) == state.cid {
                let notification = MessageDto::fetch_notification(
                    conversation_id,
                    new_messages_count,
                    "fetch automatico"
                );
                state.messages.push(notification);
            }
        } else {
            debug!("All fetched messages were already in cache for conversation {}", conversation_id);
        }
    }

    fn update_ui_messages_improved(state: &mut crate::state::core::AppState, msg: MessageDto) {
        if state.messages.iter().any(|existing| existing.id == msg.id) {
            debug!("Message already exists in UI, skipping: {}", msg.id);
            return;
        }

        let ui_insert_pos = state
            .messages
            .binary_search_by(|existing| {
                existing
                    .created_at
                    .cmp(&msg.created_at)
                    .then_with(|| existing.id.cmp(&msg.id))
            })
            .unwrap_or_else(|pos| pos);

        state.messages.insert(ui_insert_pos, msg.clone());

        debug!(
            "Message added to current conversation UI: {} characters from {}",
            msg.content.len(),
            msg.author_username
        );
    }
    
     */
}