use crate::models::{ConversationDto, MessageDto, Outgoing, UiEvent, WsStatus};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use crate::app::events::buffer_handler::BufferHandler;
use crate::app::events::sequence_handler::SequenceHandler;

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
                SequenceHandler::reset_sequence_system(state);
                info!("WebSocket connected, sequence system active");
            }

            UiEvent::WsDisconnected => {
                state.ws_status = WsStatus::Disconnected;
                SequenceHandler::reset_sequence_on_disconnect(state);
                warn!("WebSocket disconnected, requesting reconnection");
            }

            UiEvent::WsError(error) => {
                error!("WebSocket error: {}", error);
            }

            UiEvent::WsIncoming(msg) => {
                Self::handle_incoming_message(state, msg);
            }

            _ => {}
        }
        
        (state.egui_waker)();
    }

    fn handle_incoming_message(state: &mut crate::state::core::AppState, msg: MessageDto) {
        let message_conversation_id = msg.conversation_id;

        // Validazione messaggio
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

        // GESTIONE BUFFER DI RIORDINO
        if let Some(seq) = msg.sequence_num {
            let expected = state
                .conversation_sequences_confirmed
                .get(&message_conversation_id)
                .copied()
                .unwrap_or(0)
                + 1;

            if seq > expected {
                warn!(
                    "Message seq {} out of order (expected {}), buffering",
                    seq, expected
                );

                // Update per gap detection (senza conferma esplicita)
                SequenceHandler::update_conversation_sequence(state, message_conversation_id, seq);

                // Bufferizza per consegna successiva
                BufferHandler::buffer_message_for_reorder(state, msg);
                return;
            } else if seq < expected {
                debug!("Message seq {} already processed, skipping", seq);
                return;
            }

            // Messaggio in ordine - update gestisce la conferma automaticamente
            SequenceHandler::update_conversation_sequence(state, message_conversation_id, seq);

            debug!("Message seq {} matches expected, processing normally", seq);
        }

        state.sequence_stats.total_events_received += 1;

        // VERIFICA E RIMOZIONE DUPLICATI CONVERSAZIONI
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
            }
        }

        // CONVERSIONE DM STUB A CONVERSAZIONE REALE
        let is_stub_conversion = state.dm_stubs.contains_key(&message_conversation_id);

        if is_stub_conversion {
            info!(
                "Converting DM stub {} to real conversation",
                message_conversation_id
            );

            // MODIFICATO: Estrai solo lo username dalla tupla (String, Instant)
            let target_username = state.dm_stubs.remove(&message_conversation_id)
                .map(|(username, _)| username)
                .unwrap();

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
                last_read_sequence: 0,
                last_activity: msg.created_at,
                last_msg_seq: msg.sequence_num.map(|s| s as i64).unwrap_or(0),
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

        // GESTIONE CONVERSAZIONE SCONOSCIUTA
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


            let _ = state.ui_tx.send(UiEvent::TriggerConversationFetch(
                message_conversation_id,
                "messaggio_conversazione_sconosciuta".to_string(),
            ));

            state.conversation_unread_counts.entry(message_conversation_id).or_insert(0);
        }

        // AGGIORNA CACHE MESSAGGI
        if !Self::update_message_cache(state, &msg) {
            debug!("Message already exists in cache: {}", msg.id);
            return;
        }

        // AGGIORNA UI SE CONVERSAZIONE ATTIVA
        if Some(message_conversation_id) == state.cid {
            Self::update_ui_messages(state, msg.clone());
        } else {
            debug!(
                "Message cached but not for current conversation: {}",
                msg.id
            );

            // Incrementa unread per messaggi in altre chat
            if Some(msg.author_id) != state.user_id {
                let current_unread = state.conversation_unread_counts
                    .entry(message_conversation_id)
                    .or_insert(0);

                *current_unread += 1;

                debug!(
                    "Incremented unread count for conversation {} to {} (from {})",
                    message_conversation_id,
                    *current_unread,
                    msg.author_username
                );
            }
        }

        // SVUOTA BUFFER DI RIORDINO
        if let Some(seq) = msg.sequence_num {
            while let Some(buffered_msg) = BufferHandler::get_next_buffered_message(state, message_conversation_id, seq + 1) {
                let buffered_seq = buffered_msg.sequence_num.unwrap_or(0);

                // Conferma sequenza per messaggio bufferizzato
                SequenceHandler::update_conversation_sequence(state, message_conversation_id, buffered_seq);

                debug!(
                    "Processing buffered message seq {} for conversation {}",
                    buffered_seq, message_conversation_id
                );

                // Aggiungi alla cache
                if !Self::update_message_cache(state, &buffered_msg) {
                    debug!(
                        "Buffered message already exists in cache: {}",
                        buffered_msg.id
                    );
                    continue;
                }

                // Aggiorna UI se conversazione corrente
                if Some(message_conversation_id) == state.cid {
                    Self::update_ui_messages(state, buffered_msg.clone());
                } else {
                    debug!(
                        "Buffered message cached but not for current conversation: {}",
                        buffered_msg.id
                    );

                    // Incrementa unread per messaggi bufferizzati in altre chat
                    if Some(buffered_msg.author_id) != state.user_id {
                        let current_unread = state.conversation_unread_counts
                            .entry(message_conversation_id)
                            .or_insert(0);

                        *current_unread += 1;

                        debug!(
                            "Incremented unread count for conversation {} to {} (buffered message from {})",
                            message_conversation_id,
                            *current_unread,
                            buffered_msg.author_username
                        );
                    }
                }
            }
        }
    }

    /// FUNZIONE HELPER PER AUTO-MARK_READ
    ///
    /// Controlla se inviare automaticamente mark_read quando un messaggio viene aggiunto alla cache.
    fn check_and_send_auto_mark_read(state: &mut crate::state::core::AppState, msg: &MessageDto) {
        // Controlla se il messaggio appartiene alla conversazione attualmente aperta
        if state.cid != Some(msg.conversation_id) {
            return;
        }

        // Controlla se il messaggio NON è dell'utente corrente
        if Some(msg.author_id) == state.user_id {
            return;
        }

        // Controlla se ha una sequence_num
        let Some(seq) = msg.sequence_num else {
            return;
        };

        // Invia il mark_read
        if let Err(e) = state.ui_to_net_tx.try_send(Outgoing::MarkRead {
            conversation_id: msg.conversation_id,
            sequence_num: seq,
        }) {
            warn!("Failed to auto-send mark_read: {}", e);
        } else {
            debug!(
                "✅ Auto-sent mark_read for conversation {} (seq: {}, author: {})",
                msg.conversation_id, seq, msg.author_username
            );

            // Aggiorna last_read_sequence locale nella conversazione
            if let Some(conversations) = &mut state.conversations {
                if let Some(conv) = conversations
                    .iter_mut()
                    .find(|c| c.id == msg.conversation_id)
                {
                    conv.last_read_sequence = seq as i64;
                    debug!(
                        "Updated local last_read_sequence to {} for conversation {}",
                        seq, msg.conversation_id
                    );
                }
            }

            // Azzera il contatore unread per questa conversazione
            state.conversation_unread_counts.insert(msg.conversation_id, 0);
        }
    }

    /// Aggiunge un messaggio alla cache della conversazione
    /// Ritorna false se il messaggio esiste già
    fn update_message_cache(
        state: &mut crate::state::core::AppState,
        msg: &MessageDto,
    ) -> bool {
        let conversation_cache = state
            .conversation_messages
            .entry(msg.conversation_id)
            .or_insert_with(Vec::new);

        // Controlla duplicati
        if conversation_cache.iter().any(|existing| existing.id == msg.id) {
            return false;
        }

        // Inserimento ordinato per sequence o timestamp
        let insert_pos = if let Some(_msg_seq) = msg.sequence_num {
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

        if msg.sequence_num.is_some() {
            Self::verify_cache_sequence_integrity(state, msg.conversation_id);
        }

        // Auto-mark_read per tutti i messaggi aggiunti alla cache
        Self::check_and_send_auto_mark_read(state, msg);

        true
    }

    /// Aggiunge un messaggio all'UI della conversazione corrente
    fn update_ui_messages(state: &mut crate::state::core::AppState, msg: MessageDto) {
        if state.messages.iter().any(|existing| existing.id == msg.id) {
            debug!("Message already exists in UI, skipping: {}", msg.id);
            return;
        }

        let ui_insert_pos = if let Some(_msg_seq) = msg.sequence_num {
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

    /// Verifica l'integrità delle sequenze nella cache
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