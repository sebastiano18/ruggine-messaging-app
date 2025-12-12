use super::{
    auth_handler::AuthHandler, conversation_handler::ConversationHandler,
    message_handler::MessageHandler, sequence_handler::SequenceHandler,
    user_notification_handler::UserNotificationHandler, utils, websocket_handler::WebSocketHandler,
};
use std::collections::BTreeMap;

use crate::models::*;
use crate::state::core::AppState;
use tracing::{debug, error, info};

pub struct EventDispatcher;

impl EventDispatcher {
    pub fn handle_event(state: &mut AppState, event: UiEvent) {
        match event {
            UiEvent::LoginStarted => {
                AuthHandler::handle_login_started(state);
            }

            UiEvent::RegisterStarted => {
                AuthHandler::handle_register_started(state);
            }

            UiEvent::Logged(token, user_id, last_sequence) => {
                AuthHandler::handle_logged(state, token, user_id, last_sequence);
            }

            UiEvent::LoggedOut => {
                AuthHandler::handle_logged_out(state);
            }

            UiEvent::DeleteAccountStart => {
                AuthHandler::handle_delete_account_start(state);
            }

            UiEvent::DeleteAccountCancel => {
                AuthHandler::handle_delete_account_cancel(state);
            }

            UiEvent::DeleteAccountConfirm => {
                AuthHandler::handle_delete_account_confirm(state);
            }

            UiEvent::WsControlReady(_)
            | UiEvent::WsConnected
            | UiEvent::WsDisconnected
            | UiEvent::WsError(_)
            | UiEvent::WsIncoming(_) => {
                if let UiEvent::WsIncoming(ref msg) = event {
                    let conv_id = msg.conversation_id;

                    // Check if conversation exists in loaded conversations
                    let conversation_exists = state
                        .conversations
                        .as_ref()
                        .map(|convs| convs.iter().any(|c| c.id == conv_id))
                        .unwrap_or(false);

                    // Check if already fetching this conversation
                    let already_fetching = state.fetching_conversations.contains(&conv_id);

                    // ✅ CASO 1: Conversazione NON esiste e NON è in fetch → triggera fetch
                    if !conversation_exists && !already_fetching {
                        tracing::info!(
                            "Message received for unloaded conversation {}, fetching...",
                            conv_id
                        );

                        // Mark as fetching
                        state.fetching_conversations.insert(conv_id);

                        // ✅ FORCE-BUFFER (bypass gap check)
                        let buffer_entry = state
                            .message_reorder_buffer
                            .entry(conv_id)
                            .or_insert_with(BTreeMap::new);

                        if let Some(seq) = msg.sequence_num {
                            buffer_entry.insert(seq, msg.clone());
                            tracing::debug!(
                                "Force-buffered WS message seq {} for unloaded conversation {}",
                                seq,
                                conv_id
                            );
                        }

                        let base = state.base.clone();
                        let token = state.token.clone().unwrap_or_default();
                        let tx = state.ui_tx.clone();

                        state.rt.spawn(async move {
                            match crate::api::conversation::get_conversation(&base, &token, conv_id)
                                .await
                            {
                                Ok(summary) => {
                                    tracing::info!("Successfully fetched conversation {}", conv_id);
                                    let _ = tx.send(UiEvent::ConversationSummaryFetched(summary));
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to fetch conversation {}: {}",
                                        conv_id,
                                        e
                                    );
                                    let _ = tx.send(UiEvent::ConversationFetchFailed(conv_id));
                                }
                            }
                        });

                        return; // ✅ EXIT - non processare ora
                    }

                    // ✅ CASO 2: Conversazione NON esiste MA è già in fetch → buffera e aspetta
                    if !conversation_exists && already_fetching {
                        tracing::debug!(
                            "Message received for conversation {} being fetched, buffering",
                            conv_id
                        );

                        // ✅ FORCE-BUFFER anche qui
                        let buffer_entry = state
                            .message_reorder_buffer
                            .entry(conv_id)
                            .or_insert_with(BTreeMap::new);

                        if let Some(seq) = msg.sequence_num {
                            buffer_entry.insert(seq, msg.clone());
                            tracing::debug!(
                    "Buffered WS message seq {} for conversation {} (fetch in progress)",
                    seq, conv_id
                );
                        }

                        return; // ✅ EXIT - aspetta il fetch
                    }

                    // ✅ CASO 3: Conversazione ESISTE → processa normalmente
                    WebSocketHandler::handle(state, event);
                    utils::move_conversation_to_top(state, conv_id);
                } else {
                    WebSocketHandler::handle(state, event);
                }
            }

            UiEvent::PongReceived {
                current_user_sequence,
                gaps_detected,
                user_events_gap,
            } => {
                SequenceHandler::handle_pong(
                    state,
                    current_user_sequence,
                    gaps_detected,
                    user_events_gap,
                );
            }

            UiEvent::UserEventsResume { events } => {
                SequenceHandler::handle_user_events_resume(state, events);
            }

            UiEvent::MessagesResume {
                conversation_id,
                messages,
            } => {
                SequenceHandler::handle_messages_resume(state, conversation_id, messages);
            }

            UiEvent::InitialStateReceived {
                conversations,
                user_sequence,
                members_by_conversation,
            } => {
                ConversationHandler::handle_initial_state_received(
                    state,
                    conversations,
                    user_sequence,
                    members_by_conversation,
                );
            }

            UiEvent::LastMessageUpdate {
                conversation_id,
                message,
            } => {
                ConversationHandler::handle_last_message_update(state, conversation_id, message);
            }

            UiEvent::ConversationMessagesReceived {
                conversation_id,
                messages,
                has_more,
            } => {
                ConversationHandler::handle_conversation_messages_received(
                    state,
                    conversation_id,
                    messages,
                    has_more,
                );
            }

            UiEvent::ConversationConfirmed {
                conversation,
                messages,
                client_temp_id,
            } => {
                ConversationHandler::handle_conversation_confirmed(
                    state,
                    conversation,
                    messages,
                    client_temp_id,
                );
            }

            UiEvent::OlderMessagesLoaded(new_messages) => {
                ConversationHandler::handle_older_messages_loaded(state, new_messages);
            }

            UiEvent::LoadingError => {
                ConversationHandler::handle_loading_error(state);
            }

            UiEvent::Opened(cid) => {
                ConversationHandler::handle_opened(state, cid);
            }

            UiEvent::Closed(cid) => {
                ConversationHandler::handle_closed(state, cid);
            }

            UiEvent::ConversationDeleted(cid) => {
                ConversationHandler::handle_conversation_deleted(state, cid);
            }

            UiEvent::ConversationCreated(cid) => {
                ConversationHandler::handle_conversation_created(state, cid);
            }

            UiEvent::DmStubCreated(stub_id, target_username) => {
                ConversationHandler::handle_dm_stub_created(state, stub_id, target_username);
            }

            UiEvent::ConversationsLoaded(conversations) => {
                ConversationHandler::handle_conversations_loaded(state, conversations);
            }

            UiEvent::ConversationsAppended(response) => {
                tracing::info!(
        "Processing {} conversations from pagination",
        response.conversations.len()
    );

                // Estrai le conversazioni
                let conversations: Vec<_> = response
                    .conversations
                    .iter()
                    .map(|summary| summary.conversation.clone())
                    .collect();

                // Processa ogni conversazione per salvare dati e calcolare unread
                for summary in &response.conversations {
                    let conv_id = summary.conversation.id;

                    // ✅ Salva last_message se presente
                    if let Some(last_msg) = &summary.last_message {
                        state
                            .conversation_messages
                            .entry(conv_id)
                            .or_insert_with(Vec::new)
                            .push(last_msg.clone());
                    }

                    // ✅ Salva membri se presenti (non sovrascrive se già esistono)
                    if let Some(members) = &summary.members {
                        state
                            .members_list
                            .entry(conv_id)
                            .or_insert_with(|| members.clone());
                    }

                    // ✅ REGISTRA sequence della conversazione SENZA gap check
                    // (queste sono conversazioni storiche dalla REST API, non eventi real-time)
                    if summary.conversation.last_msg_seq > 0 {
                        state.conversation_sequences.insert(
                            conv_id,
                            summary.conversation.last_msg_seq as u64
                        );
                        tracing::debug!(
                "Conversation {} sequence set to {} (from pagination)",
                conv_id,
                summary.conversation.last_msg_seq
            );
                    }

                    // ✅ CALCOLA e aggiorna unread count per il badge
                    let last_msg_seq = summary.conversation.last_msg_seq;
                    let last_read_seq = summary.conversation.last_read_sequence;

                    if last_msg_seq > 0 {
                        let unread_count = if last_msg_seq > last_read_seq {
                            last_msg_seq - last_read_seq
                        } else {
                            0
                        };

                        if unread_count > 0 {
                            state.conversation_unread_counts.insert(conv_id, unread_count);
                            tracing::debug!(
                    "Conversation {} has {} unread messages (last_msg: {}, last_read: {})",
                    conv_id,
                    unread_count,
                    last_msg_seq,
                    last_read_seq
                );
                        } else {
                            // Nessun messaggio non letto, rimuovi dalla mappa se presente
                            state.conversation_unread_counts.remove(&conv_id);
                        }
                    }
                }

                // ✅ Appendi CON deduplicazione
                if let Some(existing) = &mut state.conversations {
                    // Filtra solo conversazioni NUOVE
                    let new_conversations: Vec<_> = conversations
                        .into_iter()
                        .filter(|new_conv| !existing.iter().any(|c| c.id == new_conv.id))
                        .collect();

                    let added_count = new_conversations.len();
                    let duplicates_count = response.conversations.len() - added_count;

                    existing.extend(new_conversations);

                    if duplicates_count > 0 {
                        tracing::debug!(
                "Appended {} new conversations (filtered {} duplicates). Total: {}",
                added_count,
                duplicates_count,
                existing.len()
            );
                    } else {
                        tracing::debug!(
                "Appended {} new conversations. Total: {}",
                added_count,
                existing.len()
            );
                    }
                } else {
                    state.conversations = Some(conversations);
                    tracing::debug!(
            "Created conversations list with {} items",
            response.conversations.len()
        );
                }

                // ✅ Aggiorna stato paginazione
                state.has_more_conversations = response.has_more;
                state.next_cursor = response.next_cursor;
                state.is_loading_more_conversations = false;

                tracing::info!(
        "Conversations total: {}, Has more: {}, Next cursor: {:?}",
        state.conversations.as_ref().map(|c| c.len()).unwrap_or(0),
        response.has_more,
        response.next_cursor
    );
            }

            UiEvent::ConversationSummaryFetched(summary) => {
                let conv_id = summary.conversation.id;

                tracing::info!("Processing fetched conversation {}", conv_id);

                // Aggiorna members
                if let Some(members) = &summary.members {
                    state.members_list.insert(conv_id, members.clone());
                }

                // ✅ Try to deliver buffered messages PRIMA di inizializzare sequence
                use crate::app::events::buffer_handler::BufferHandler;
                let delivered = BufferHandler::try_deliver_buffered_messages(state, conv_id);

                if !delivered.is_empty() {
                    tracing::info!(
                        "Delivered {} buffered messages for newly fetched conversation {}",
                        delivered.len(),
                        conv_id
                    );

                    // ✅ Processa ogni messaggio deliverizzato
                    for msg in delivered {
                        // Inserisci in cache
                        let cache = state
                            .conversation_messages
                            .entry(conv_id)
                            .or_insert_with(Vec::new);

                        // Check duplicati
                        if !cache.iter().any(|m| m.id == msg.id) {
                            let pos = cache
                                .binary_search_by(|existing| {
                                    match (existing.sequence_num, msg.sequence_num) {
                                        (Some(e_seq), Some(m_seq)) => e_seq.cmp(&m_seq),
                                        _ => existing
                                            .created_at
                                            .cmp(&msg.created_at)
                                            .then_with(|| existing.id.cmp(&msg.id)),
                                    }
                                })
                                .unwrap_or_else(|p| p);

                            cache.insert(pos, msg.clone());

                            tracing::debug!(
                                "Inserted buffered message {} into cache for conversation {}",
                                msg.id,
                                conv_id
                            );
                        }

                        // ✅ Aggiorna sequence (questo viene fatto automaticamente qui!)
                        if let Some(seq) = msg.sequence_num {
                            use crate::app::events::sequence_handler::SequenceHandler;
                            SequenceHandler::update_conversation_sequence(state, conv_id, seq);
                        }

                        // ✅ Incrementa unread se non è la conversazione corrente
                        if Some(conv_id) != state.cid {
                            if Some(msg.author_id) != state.user_id {
                                let current_unread = state
                                    .conversation_unread_counts
                                    .get(&conv_id)
                                    .copied()
                                    .unwrap_or(0);

                                state
                                    .conversation_unread_counts
                                    .insert(conv_id, current_unread + 1);

                                tracing::debug!(
                                    "Incremented unread count for conversation {} to {}",
                                    conv_id,
                                    current_unread + 1
                                );
                            }
                        } else {
                            // Se è la conversazione corrente, aggiungi anche alla UI
                            if !state.messages.iter().any(|m| m.id == msg.id) {
                                let ui_pos = state
                                    .messages
                                    .binary_search_by(|existing| {
                                        match (existing.sequence_num, msg.sequence_num) {
                                            (Some(e_seq), Some(m_seq)) => e_seq.cmp(&m_seq),
                                            _ => existing
                                                .created_at
                                                .cmp(&msg.created_at)
                                                .then_with(|| existing.id.cmp(&msg.id)),
                                        }
                                    })
                                    .unwrap_or_else(|p| p);

                                state.messages.insert(ui_pos, msg.clone());
                            }
                        }
                    }
                } else {
                    // ✅ SOLO se buffer vuoto, inizializza sequence da last_msg_seq
                    if summary.conversation.last_msg_seq > 0 {
                        use crate::app::events::sequence_handler::SequenceHandler;
                        SequenceHandler::update_conversation_sequence(
                            state,
                            conv_id,
                            summary.conversation.last_msg_seq as u64,
                        );
                        tracing::info!(
                            "Initialized conversation {} sequence to {} (no buffered messages)",
                            conv_id,
                            summary.conversation.last_msg_seq
                        );
                    }
                }

                // Rimuovi da fetching set
                state.fetching_conversations.remove(&conv_id);

                // Inserisci conversazione (deduplica)
                let already_exists = state
                    .conversations
                    .as_ref()
                    .map(|convs| convs.iter().any(|c| c.id == conv_id))
                    .unwrap_or(false);

                if already_exists {
                    tracing::debug!("Conversation {} already exists, skipping insert", conv_id);
                } else {
                    let conversation_dto = summary.conversation.clone();

                    if let Some(ref mut convs) = state.conversations {
                        convs.insert(0, conversation_dto);
                        tracing::debug!("Inserted new conversation at top. Total: {}", convs.len());
                    } else {
                        state.conversations = Some(vec![conversation_dto]);
                        tracing::debug!("Initialized conversations list with fetched conversation");
                    }
                }
            }

            UiEvent::ConversationFetchFailed(conv_id) => {
                tracing::warn!(
                    "Failed to fetch conversation {}, cleaning up state",
                    conv_id
                );
                // Remove from fetching set to allow retry
                state.fetching_conversations.remove(&conv_id);
            }

            UiEvent::AllMessagesLoaded(all_messages) => {
                ConversationHandler::handle_all_messages_loaded(state, all_messages);
            }

            UiEvent::RefreshedMsgs(messages) => {
                ConversationHandler::handle_refreshed_msgs(state, messages);
            }

            UiEvent::SingleConversationLoaded(conv) => {
                ConversationHandler::handle_single_conversation_loaded(state, conv);
            }

            UiEvent::InitialLoadComplete => {
                ConversationHandler::handle_initial_load_complete(state);
            }

            UiEvent::LoadingProgress(msg) => {
                ConversationHandler::handle_loading_progress(msg);
            }

            UiEvent::MessageSendFailed(msg_id) => {
                MessageHandler::handle_message_send_failed(state, msg_id);
            }

            UiEvent::MessageConfirmation {
                client_msg_id,
                server_msg_id,
                sequence,
                status,
            } => {
                MessageHandler::handle_message_confirmation(
                    state,
                    client_msg_id,
                    server_msg_id,
                    sequence,
                    status,
                );
            }

            UiEvent::MessageDeleted {
                message_id,
                conversation_id,
            } => {
                MessageHandler::handle_message_deleted(state, message_id, conversation_id);
            }

            UiEvent::UserCheckResult {
                username,
                exists,
                user_id,
                request_id,
            } => {
                state.handle_user_check_result(username, exists, user_id, request_id);
            }

            UiEvent::UserNotification {
                sequence,
                event_type,
                event_data,
                conversation_id,
                recovery,
            } => {
                UserNotificationHandler::handle_user_notification(
                    state,
                    sequence,
                    event_type,
                    event_data,
                    conversation_id,
                    recovery,
                );
            }

            UiEvent::Info(msg) => {
                info!("Info: {}", msg);
                state.set_message_info(msg);
            }

            UiEvent::Error(error_type) => {
                // Log dell'errore tecnico
                match &error_type {
                    ErrorType::Connection => error!("Connection error"),
                    ErrorType::MessageSend => error!("Message send error"),
                    ErrorType::MessageDelete => error!("Message delete error"),
                    ErrorType::ConversationDelete => error!("Conversation delete error"),
                    ErrorType::GroupLeave => error!("Group leave error"),
                    ErrorType::GroupCreate => error!("Group create error"),
                    ErrorType::Invite => error!("Invite error"),
                    ErrorType::Auth(details) => error!("Auth error: {}", details),
                    ErrorType::DataRecovery => error!("Data recovery error"),
                    ErrorType::Generic(msg) => error!("Generic error: {}", msg),
                    ErrorType::GroupRemoveMember => error!("Group remove member error"),
                }

                // Decidi se mostrare toast e quale messaggio
                let user_message = match error_type {
                    ErrorType::Connection => {
                        // Mostra toast per problemi di connessione durante operazioni
                        Some("Problema di connessione. Verifica la rete.".to_string())
                    }
                    ErrorType::MessageSend => {
                        Some("Impossibile inviare il messaggio. Riprova.".to_string())
                    }
                    ErrorType::MessageDelete => {
                        Some("Impossibile eliminare il messaggio. Riprova.".to_string())
                    }
                    ErrorType::ConversationDelete => {
                        Some("Impossibile eliminare la conversazione. Riprova.".to_string())
                    }
                    ErrorType::GroupLeave => {
                        Some("Impossibile uscire dal gruppo. Riprova.".to_string())
                    }
                    ErrorType::GroupCreate => {
                        Some("Impossibile creare il gruppo. Riprova.".to_string())
                    }
                    ErrorType::Invite => Some("Impossibile inviare l'invito. Riprova.".to_string()),
                    ErrorType::GroupRemoveMember => {
                        Some("Impossibile rimuovere il membro dal gruppo. Riprova.".to_string())
                    }
                    ErrorType::Auth(details) => Some(details.clone()),
                    ErrorType::DataRecovery => {
                        Some("Errore durante il recupero dati. Riprova.".to_string())
                    }
                    ErrorType::Generic(msg) => Some(msg.clone()),
                };

                // Mostra toast solo se c'è un messaggio
                if let Some(msg) = user_message {
                    state.set_message_error(msg);
                }
            }

            UiEvent::InviteCreated(token) => {
                info!("Invite created: {}", token);
                state.last_created_invite = Some(token);
            }

            UiEvent::MembersLoaded(conversation_id, members) => {
                ConversationHandler::handle_members_loaded(state, conversation_id, members);
            }

            UiEvent::TriggerConversationFetch(cid, reason) => {
                ConversationHandler::handle_trigger_conversation_fetch(state, cid, reason);
            }

            UiEvent::ConversationCompleteFetched(conv, messages) => {
                ConversationHandler::handle_conversation_complete_fetched(state, conv, messages);
            }

            UiEvent::SendPing => {
                debug!("Manual ping requested");
                SequenceHandler::send_ping(state);
            }
        }

        (state.egui_waker)();
    }
}
