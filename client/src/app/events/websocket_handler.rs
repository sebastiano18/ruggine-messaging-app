use crate::models::{ConversationDto, MessageDto, UiEvent, WsStatus};
use tracing::{debug, error, info, warn};
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
                crate::app::events::helpers::add_system_message(
                    state,
                    "WebSocket disconnesso".into(),
                );
            }
            UiEvent::WsError(error) => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;
                crate::app::events::helpers::add_system_message(
                    state,
                    format!("WebSocket errore: {}", error),
                );
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

        // 1. CONTROLLO DUPLICATI PREVENTIVO
        if let Some(ref conversations) = state.conversations {
            let count = conversations
                .iter()
                .filter(|c| c.id == message_conversation_id)
                .count();
            if count > 1 {
                error!(
                    "DUPLICATE CONVERSATIONS DETECTED for ID {}: {} instances",
                    message_conversation_id, count
                );

                let mut deduped_conversations = Vec::new();
                let mut seen_ids = std::collections::HashSet::new();

                for conv in conversations {
                    if seen_ids.insert(conv.id) {
                        deduped_conversations.push(conv.clone());
                    } else {
                        warn!(
                            "Removing duplicate conversation: {} ({})",
                            conv.id, conv.title
                        );
                    }
                }

                state.conversations = Some(deduped_conversations);
                crate::app::events::helpers::add_system_message(
                    state,
                    "Rimossi duplicati conversazioni".into(),
                );
            }
        }

        // 2. GESTIONE DM STUB: Solo conversione, mai creazione
        let is_stub_conversion = state.dm_stubs.contains_key(&message_conversation_id);

        if is_stub_conversion {
            info!(
                "Converting DM stub {} to real conversation",
                message_conversation_id
            );

            let target_username = state.dm_stubs.remove(&message_conversation_id).unwrap();

            let conversation_title = if msg.author_id == state.user_id.unwrap_or(Uuid::nil()) {
                target_username.clone()
            } else {
                msg.author_username.clone()
            };

            let real_conversation = ConversationDto {
                id: message_conversation_id,
                kind: "dm".to_string(),
                title: conversation_title,
                owner_id: state.user_id.unwrap_or(Uuid::nil()),
                created_at: msg.created_at,
            };

            if let Some(ref mut conversations) = state.conversations {
                let old_len = conversations.len();
                conversations.retain(|c| c.id != message_conversation_id);
                let removed = old_len - conversations.len();

                if removed > 0 {
                    info!(
                        "Removed {} stub instances for conversation {}",
                        removed, message_conversation_id
                    );
                }

                conversations.push(real_conversation);
                info!(
                    "Converted DM stub to real conversation: {}",
                    message_conversation_id
                );
            } else {
                state.conversations = Some(vec![real_conversation]);
            }

            crate::app::events::helpers::add_system_message(
                state,
                format!("Chat con {} ora attiva!", target_username),
            );
        }

        // 3. VERIFICA ESISTENZA CONVERSAZIONE (solo se non è una conversione stub)
        let conversation_exists = state
            .conversations
            .as_ref()
            .map(|convs| convs.iter().any(|c| c.id == message_conversation_id))
            .unwrap_or(false);

        // 4. GESTIONE CONVERSAZIONE SCONOSCIUTA - UNIFICATA
        if !is_stub_conversion && !conversation_exists {
            info!(
                "Message for unknown conversation {} - triggering unified fetch",
                message_conversation_id
            );

            crate::app::events::helpers::add_system_message(
                state,
                format!(
                    "Nuovo messaggio da {} - caricando conversazione...",
                    msg.author_username
                ),
            );

            // UNIFICATO: Usa TriggerConversationFetch per conversazioni sconosciute
            let _ = state
                .ui_tx
                .send(UiEvent::TriggerConversationFetch(
                    message_conversation_id,
                    "messaggio_conversazione_sconosciuta".to_string()
                ));
        }

        // 5. AGGIORNA CACHE MESSAGGI (sempre)
        if !Self::update_message_cache_improved(state, &msg) {
            debug!("Message already exists in cache, skipping: {}", msg.id);
            return;
        }

        // 6. AGGIORNA UI SOLO SE È LA CONVERSAZIONE CORRENTE
        if Some(message_conversation_id) == state.cid {
            Self::update_ui_messages_improved(state, msg);
        } else {
            debug!(
                "Message cached for different conversation: {} -> {} (current: {:?})",
                msg.author_username, message_conversation_id, state.cid
            );
        }
    }

    fn update_message_cache_improved(
        state: &mut crate::state::core::AppState,
        msg: &MessageDto,
    ) -> bool {
        let conversation_cache = state
            .conversation_messages
            .entry(msg.conversation_id)
            .or_insert_with(Vec::new);

        if conversation_cache
            .iter()
            .any(|existing| existing.id == msg.id)
        {
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