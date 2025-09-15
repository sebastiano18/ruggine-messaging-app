// events/websocket_handler.rs - Gestione WebSocket
use crate::models::{UiEvent, WsStatus, MessageDto, ConversationDto};
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct WebSocketHandler;

impl WebSocketHandler {
    pub fn handle(state: &mut crate::state::core::AppState, event: UiEvent) {
        match event {
            UiEvent::WsControlReady(ctrl) => {
                state.ws_ctrl = Some(ctrl);
                debug!("WebSocket control ready");
            }
            UiEvent::WsConnected => {
                state.ws_status = WsStatus::Connected;
                crate::app::events::helpers::add_system_message(state, "WebSocket connesso".into());
            }
            UiEvent::WsDisconnected => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;
                crate::app::events::helpers::add_system_message(state, "WebSocket disconnesso".into());
            }
            UiEvent::WsError(error) => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;
                crate::app::events::helpers::add_system_message(state, format!("WebSocket errore: {}", error));
            }
            UiEvent::WsIncoming(msg) => {
                Self::handle_incoming_message(state, msg);
            }
            _ => unreachable!("Invalid websocket event"),
        }
    }

    fn handle_incoming_message(state: &mut crate::state::core::AppState, msg: MessageDto) {
        let message_conversation_id = msg.conversation_id;

        if !crate::app::events::helpers::validate_incoming_message(&msg) {
            return;
        }

        debug!(
            "Processing incoming message: {} from {} in conversation {}",
            msg.content.chars().take(50).collect::<String>(),
            msg.author_username,
            message_conversation_id
        );

        // GESTIONE DM STUB: Solo conversione, mai creazione
        if state.dm_stubs.contains_key(&message_conversation_id) {
            info!("Converting DM stub {} to real conversation", message_conversation_id);

            let target_username = state.dm_stubs.remove(&message_conversation_id).unwrap();

            let real_conversation = ConversationDto {
                id: message_conversation_id,
                kind: "dm".to_string(),
                title: if msg.author_id == state.user_id.unwrap_or(Uuid::nil()) {
                    target_username
                } else {
                    msg.author_username.clone()
                },
                owner_id: state.user_id.unwrap_or(Uuid::nil()),
                created_at: msg.created_at,
            };

            if let Some(ref mut conversations) = state.conversations {
                conversations.push(real_conversation);
            } else {
                state.conversations = Some(vec![real_conversation]);
            }

            crate::app::events::helpers::add_system_message(state, format!("Chat con {} ora attiva!",
                                                                           if msg.author_id == state.user_id.unwrap_or(Uuid::nil()) {
                                                                               "te stesso"
                                                                           } else {
                                                                               &msg.author_username
                                                                           }
            ));
        }

        // Verifica esistenza conversazione
        let conversation_exists = state
            .conversations
            .as_ref()
            .map(|convs| convs.iter().any(|c| c.id == message_conversation_id))
            .unwrap_or(false);

        // CRITICO: MAI creare conversazioni lato client
        if !conversation_exists {
            info!("Message for unknown conversation {} - only caching message and triggering refresh", 
                  message_conversation_id);

            // Solo messaggio informativo - NO creazione conversazione
            crate::app::events::helpers::add_system_message(
                state,
                format!("Nuovo messaggio da {} - aggiornando lista conversazioni...", msg.author_username)
            );

            // Triggera refresh per ottenere conversazione dal server
            let _ = state.ui_tx.send(UiEvent::ConversationListUpdated);
        }

        // SEMPRE aggiorna cache messaggi (indipendentemente dall'esistenza della conversazione)
        if !Self::update_message_cache_improved(state, &msg) {
            debug!("Message already exists in cache, skipping: {}", msg.id);
            return;
        }

        // Aggiorna UI solo se è la conversazione corrente
        if Some(message_conversation_id) == state.cid {
            Self::update_ui_messages_improved(state, msg);
        } else {
            debug!(
                "Message cached for different conversation: {} -> {} (current: {:?})",
                msg.author_username, message_conversation_id, state.cid
            );
        }
    }

    fn update_message_cache_improved(state: &mut crate::state::core::AppState, msg: &MessageDto) -> bool {
        let conversation_cache = state
            .conversation_messages
            .entry(msg.conversation_id)
            .or_insert_with(Vec::new);

        if conversation_cache.iter().any(|existing| existing.id == msg.id) {
            return false;
        }

        let insert_pos = conversation_cache
            .binary_search_by(|existing| {
                existing
                    .created_at
                    .cmp(&msg.created_at)
                    .then_with(|| existing.id.cmp(&msg.id))
            })
            .unwrap_or_else(|pos| pos);

        conversation_cache.insert(insert_pos, msg.clone());

        debug!(
            "Message added to cache for conversation {}: {} chars from {}",
            msg.conversation_id,
            msg.content.len(),
            msg.author_username
        );

        true
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
}