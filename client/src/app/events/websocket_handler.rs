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


                crate::app::events::helpers::add_system_message(state, "WebSocket connesso - sincronizzazione attiva".into());
                info!("WebSocket connected, dual sequence system active - user_seq: {}", state.user_sequence_confirmed);
            }
            UiEvent::WsDisconnected => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;


                crate::app::events::helpers::add_system_message(
                    state,
                    "WebSocket disconnesso - riconnessione automatica in corso...".into(),
                );

                info!("WebSocket disconnected, preserving sequences - user: {}, conversations: {}", 
                      state.user_sequence_confirmed, state.conversation_sequences.len());
            }
            UiEvent::WsError(error) => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;

                error!("WebSocket error: {}", error);
                crate::app::events::helpers::add_system_message(
                    state,
                    format!("Errore WebSocket: {}", error),
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
            warn!("Invalid incoming message rejected: {}", msg.id);
            return;
        }

        debug!(
        "Processing incoming message: {} from {} in conversation {} (seq: {:?})",
        msg.content.chars().take(50).collect::<String>(),
        msg.author_username,
        message_conversation_id,
        msg.sequence_num
    );

        // NUOVO: Gestione buffer di riordino
        if let Some(seq) = msg.sequence_num {
            let expected = state.conversation_sequences_confirmed
                .get(&message_conversation_id)
                .copied()
                .unwrap_or(0) + 1;

            if seq > expected {
                warn!("Message seq {} out of order (expected {}), buffering", seq, expected);
                state.buffer_message_for_reorder(msg);
                return;
            } else if seq < expected {
                debug!("Message seq {} already processed, skipping", seq);
                return;
            }

            state.update_conversation_sequence(message_conversation_id, seq);
            state.conversation_sequences_confirmed.insert(message_conversation_id, seq);

            debug!("Message seq {} matches expected, processing normally", seq);
        }

        state.sequence_stats.total_events_received += 1;

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

        let conversation_exists = state
            .conversations
            .as_ref()
            .map(|convs| convs.iter().any(|c| c.id == message_conversation_id))
            .unwrap_or(false);

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

            let _ = state
                .ui_tx
                .send(UiEvent::TriggerConversationFetch(
                    message_conversation_id,
                    "messaggio_conversazione_sconosciuta".to_string()
                ));
        }

        if !Self::update_message_cache_improved(state, &msg) {
            debug!("Message already exists in cache, skipping: {}", msg.id);
            return;
        }

        if Some(message_conversation_id) == state.cid {
            Self::update_ui_messages_improved(state, msg.clone());
        } else {
            debug!(
            "Message cached for different conversation: {} -> {} (current: {:?})",
            msg.author_username, message_conversation_id, state.cid
        );
        }

        // NUOVO: Controlla buffer dopo processing
        let buffered_messages = state.try_deliver_buffered_messages(message_conversation_id);
        if !buffered_messages.is_empty() {
            info!("Delivering {} buffered messages for conversation {}", 
            buffered_messages.len(), message_conversation_id);

            for buffered_msg in buffered_messages {
                if let Some(seq) = buffered_msg.sequence_num {
                    state.update_conversation_sequence(message_conversation_id, seq);
                    state.conversation_sequences_confirmed.insert(message_conversation_id, seq);
                }

                let _ = state.ui_tx.send(UiEvent::WsIncoming(buffered_msg));
            }
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

        // Controlla duplicati
        if conversation_cache
            .iter()
            .any(|existing| existing.id == msg.id)
        {
            return false;
        }

        // Se il messaggio ha una sequence, usa quella per l'ordinamento
        let insert_pos = if let Some(msg_seq) = msg.sequence_num {
            // Ordina prima per sequence, poi per timestamp come fallback
            conversation_cache
                .binary_search_by(|existing| {
                    match (existing.sequence_num, msg.sequence_num) {
                        (Some(e_seq), Some(m_seq)) => e_seq.cmp(&m_seq),
                        _ => existing.created_at.cmp(&msg.created_at)
                            .then_with(|| existing.id.cmp(&msg.id))
                    }
                })
                .unwrap_or_else(|pos| pos)
        } else {
            // Senza sequence, usa solo timestamp
            conversation_cache
                .binary_search_by(|existing| {
                    existing.created_at.cmp(&msg.created_at)
                        .then_with(|| existing.id.cmp(&msg.id))
                })
                .unwrap_or_else(|pos| pos)
        };

        conversation_cache.insert(insert_pos, msg.clone());

        debug!(
            "Message added to cache for conversation {}: {} chars from {} (seq: {:?}, pos: {})",
            msg.conversation_id,
            msg.content.len(),
            msg.author_username,
            msg.sequence_num,
            insert_pos
        );

        // Verifica integrità sequence nella cache
        if msg.sequence_num.is_some() {
            Self::verify_cache_sequence_integrity(state, msg.conversation_id);
        }

        true
    }

    fn update_ui_messages_improved(state: &mut crate::state::core::AppState, msg: MessageDto) {
        if state.messages.iter().any(|existing| existing.id == msg.id) {
            debug!("Message already exists in UI, skipping: {}", msg.id);
            return;
        }

        // Usa sequence per ordinamento se disponibile
        let ui_insert_pos = if let Some(msg_seq) = msg.sequence_num {
            state
                .messages
                .binary_search_by(|existing| {
                    match (existing.sequence_num, msg.sequence_num) {
                        (Some(e_seq), Some(m_seq)) => e_seq.cmp(&m_seq),
                        _ => existing.created_at.cmp(&msg.created_at)
                            .then_with(|| existing.id.cmp(&msg.id))
                    }
                })
                .unwrap_or_else(|pos| pos)
        } else {
            state
                .messages
                .binary_search_by(|existing| {
                    existing.created_at.cmp(&msg.created_at)
                        .then_with(|| existing.id.cmp(&msg.id))
                })
                .unwrap_or_else(|pos| pos)
        };

        state.messages.insert(ui_insert_pos, msg.clone());

        debug!(
            "Message added to current conversation UI: {} characters from {} (seq: {:?}, pos: {})",
            msg.content.len(),
            msg.author_username,
            msg.sequence_num,
            ui_insert_pos
        );
    }

    /// Verifica l'integrità delle sequence nella cache
    fn verify_cache_sequence_integrity(state: &crate::state::core::AppState, conversation_id: Uuid) {
        if let Some(messages) = state.conversation_messages.get(&conversation_id) {
            let sequenced_messages: Vec<u64> = messages
                .iter()
                .filter_map(|m| m.sequence_num)
                .collect();

            if sequenced_messages.len() > 1 {
                for window in sequenced_messages.windows(2) {
                    if window[1] != window[0] + 1 && window[1] > window[0] + 1 {
                        debug!(
                            "Sequence gap in cache for conversation {}: {} -> {} (gap: {})",
                            conversation_id,
                            window[0],
                            window[1],
                            window[1] - window[0] - 1
                        );
                    }
                }
            }
        }
    }
}