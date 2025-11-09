use crate::app::events::helpers;
use crate::app::events::sequence_handler::SequenceHandler;
use crate::models::*;
use crate::state::core::AppState;
use tracing::{debug, info, warn};
use uuid::Uuid;

pub struct ConversationHandler;

impl ConversationHandler {
    pub fn handle_initial_state_received(
        state: &mut AppState,
        conversations: Vec<ConversationDto>,
        user_sequence: u64,
    ) {
        info!(
            "Processing initial state: {} conversations, user_seq: {}",
            conversations.len(),
            user_sequence
        );

        state.user_sequence_confirmed = user_sequence;
        state.user_sequence_received = user_sequence;

        state.conversations = Some(conversations.clone());

        state.conversation_unread_counts.clear();

        // Inizializza le sequenze per TUTTE le conversazioni
        // Usa la sequenza dell'ultimo messaggio, non last_read_sequence!
        // Questo evita gap permanenti quando ci sono messaggi non letti
        for conv in &conversations {
            // Prova a usare la sequenza dell'ultimo messaggio se disponibile
            let last_seq = conv.last_msg_seq;

            if last_seq > 0 {
                state
                    .conversation_sequences
                    .insert(conv.id, last_seq as u64);
                state
                    .conversation_sequences_confirmed
                    .insert(conv.id, last_seq as u64);
                debug!(
                    "Initialized conversation {} ({}) sequences to {} (from last_msg_seq)",
                    conv.title, conv.id, last_seq
                );
            }
        }

        for conv in &conversations {
            let last_cached_seq = state
                .conversation_messages
                .get(&conv.id)
                .and_then(|messages| messages.iter().filter_map(|m| m.sequence_num).max());

            let unread = if let Some(last_seq) = last_cached_seq {
                let unread_count = last_seq as i64 - conv.last_read_sequence;
                std::cmp::max(0, unread_count)
            } else {
                0
            };

            state.conversation_unread_counts.insert(conv.id, unread);

            if unread > 0 {
                debug!(
                    "Conversation '{}' (id: {}) - last_cached_seq: {:?}, last_read: {}, unread: {}",
                    conv.title, conv.id, last_cached_seq, conv.last_read_sequence, unread
                );
            }
        }

        let total_unread: i64 = state.conversation_unread_counts.values().sum();
        info!(
            "Loaded {} conversations with {} total unread messages",
            conversations.len(),
            total_unread
        );

        let mut stubs_to_remove = Vec::new();
        for (stub_id, _) in &state.dm_stubs {
            if conversations.iter().any(|c| c.id == *stub_id) {
                stubs_to_remove.push(*stub_id);
            }
        }

        for stub_id in stubs_to_remove {
            state.remove_dm_stub(stub_id);
            debug!("Removed DM stub {} (now real conversation)", stub_id);
        }

        helpers::add_system_message(
            state,
            format!("Sincronizzate {} conversazioni", conversations.len()),
        );

        if state.cid.is_some() && state.messages.is_empty() {
            state.request_conversations_refresh = true;
        }
    }

    pub fn handle_last_message_update(
        state: &mut AppState,
        conversation_id: Uuid,
        message: MessageDto,
    ) {
        if let Some(ref mut conversations) = state.conversations {
            if let Some(conv) = conversations.iter_mut().find(|c| c.id == conversation_id) {
                conv.last_activity = std::cmp::max(conv.last_activity, message.created_at);

                if let Some(last_msg_seq) = message.sequence_num {
                    let unread = std::cmp::max(0, last_msg_seq as i64 - conv.last_read_sequence);
                    state
                        .conversation_unread_counts
                        .insert(conversation_id, unread);

                    if unread > 0 {
                        debug!("Updated unread for '{}': {} messages", conv.title, unread);
                    }
                }
            }
        }

        state
            .conversation_messages
            .entry(conversation_id)
            .or_insert_with(Vec::new)
            .push(message);
    }

    pub fn handle_conversation_messages_received(
        state: &mut AppState,
        conversation_id: Uuid,
        messages: Vec<MessageDto>,
        has_more: bool,
    ) {
        info!(
            "Received {} messages for conversation {} (has_more: {})",
            messages.len(),
            conversation_id,
            has_more
        );

        let sequences: Vec<u64> = messages.iter().filter_map(|m| m.sequence_num).collect();
        info!("Message sequences received: {:?}", sequences);

        let mut sorted_messages = messages.clone();
        sorted_messages.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
            (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
            _ => a.created_at.cmp(&b.created_at),
        });

        state
            .conversation_messages
            .insert(conversation_id, sorted_messages.clone());

        if let Some(max_seq) = sorted_messages.iter().filter_map(|m| m.sequence_num).max() {
            state
                .conversation_sequences
                .insert(conversation_id, max_seq);
            state
                .conversation_sequences_confirmed
                .insert(conversation_id, max_seq);
            debug!(
                "Updated conversation {} sequence to {} from messages",
                conversation_id, max_seq
            );
        }

        if state.cid == Some(conversation_id) {
            state.messages = sorted_messages;

            if let Some(ref conversations) = state.conversations {
                if let Some(conv) = conversations.iter().find(|c| c.id == conversation_id) {
                    state.conv_title = conv.title.clone();
                    debug!("Updated conversation title to: {}", state.conv_title);
                }
            }

            if has_more {
                helpers::add_system_message(
                    state,
                    "Caricati messaggi recenti (altri disponibili)".into(),
                );
            }
        }
    }

    pub fn handle_conversation_confirmed(
        state: &mut AppState,
        conversation: ConversationDto,
        messages: Vec<MessageDto>,
        client_temp_id: Option<String>,
    ) {
        info!(
            "Processing conversation confirmation for {} (temp_id: {:?})",
            conversation.id, client_temp_id
        );

        let mut was_active_stub = false;
        let mut stub_to_remove = None;

        if let Some(ref temp_id) = client_temp_id {
            if let Ok(stub_uuid) = Uuid::parse_str(temp_id) {
                if state.cid == Some(stub_uuid) {
                    was_active_stub = true;
                    info!(
                        "Active stub {} will be replaced with real conversation {}",
                        stub_uuid, conversation.id
                    );
                }

                if state.dm_stubs.contains_key(&stub_uuid) {
                    stub_to_remove = Some(stub_uuid);
                }
            }
        }

        if let Some(stub_id) = stub_to_remove {
            if let Some(target) = state.dm_stubs.remove(&stub_id) {
                info!(
                    "Removed DM stub {} (target: {}) after confirmation",
                    stub_id, target
                );
            }

            state.conversation_sequences.remove(&stub_id);
            state.conversation_sequences_confirmed.remove(&stub_id);

            // IMPORTANTE: Rimuovi lo stub anche da conversations se presente
            if let Some(ref mut conversations) = state.conversations {
                conversations.retain(|c| c.id != stub_id);
                info!("Removed stub {} from conversations list", stub_id);
            }
        }

        // Aggiungi o aggiorna la conversazione reale
        if let Some(ref mut conversations) = state.conversations {
            // Rimuovi eventuali conversazioni con lo stesso ID reale (non dovrebbe succedere)
            conversations.retain(|c| c.id != conversation.id);
            conversations.push(conversation.clone());
            conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            info!(
                "Added conversation {} to list ({} total)",
                conversation.id,
                conversations.len()
            );
        } else {
            state.conversations = Some(vec![conversation.clone()]);
            info!("Initialized conversations list with {}", conversation.id);
        }

        if !messages.is_empty() {
            state
                .conversation_messages
                .insert(conversation.id, messages.clone());

            if let Some(last_msg) = messages.last() {
                if let Some(seq) = last_msg.sequence_num {
                    state.conversation_sequences.insert(conversation.id, seq);
                    state
                        .conversation_sequences_confirmed
                        .insert(conversation.id, seq);
                    info!(
                        "Set conversation {} sequence to {} from messages",
                        conversation.id, seq
                    );
                }
            }
        }

        if was_active_stub {
            state.cid = Some(conversation.id);
            state.conv_title = conversation.title.clone();
            state.messages = messages;
            info!(
                "Updated active conversation from stub {} to real {}",
                stub_to_remove.unwrap_or(Uuid::nil()),
                conversation.id
            );
        } else if state.cid == Some(conversation.id) {
            state.messages = messages;
        }

        helpers::add_system_message(
            state,
            format!("Conversazione '{}' confermata", conversation.title),
        );
    }

    pub fn handle_older_messages_loaded(state: &mut AppState, new_messages: Vec<MessageDto>) {
        state.is_loading_more = false;

        let Some(cid) = state.cid else { return };

        if new_messages.is_empty() {
            state.has_more_messages.insert(cid, false);
            return;
        }

        if new_messages.len() < 30 {
            state.has_more_messages.insert(cid, false);
        }

        let mut all_messages = Vec::new();
        all_messages.extend(new_messages);
        all_messages.extend(state.messages.clone());

        let mut seen_ids = std::collections::HashSet::new();
        all_messages.retain(|msg| seen_ids.insert(msg.id));

        all_messages.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
            (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
            _ => a.created_at.cmp(&b.created_at),
        });

        state.messages = all_messages.clone();
        state.conversation_messages.insert(cid, all_messages);
    }

    pub fn handle_loading_error(state: &mut AppState) {
        state.is_loading_more = false;
    }

    pub fn handle_opened(state: &mut AppState, cid: Uuid) {
        info!("Opening conversation: {}", cid);

        state.cid = Some(cid);
        state.page = Page::Chat;

        state.messages.clear();
        state.is_loading_more = false;

        state.has_more_messages.insert(cid, true);

        state.conversation_sequences.entry(cid).or_insert(0);
        state
            .conversation_sequences_confirmed
            .entry(cid)
            .or_insert(0);

        if state.conversation_unread_counts.contains_key(&cid) {
            let old_count = state
                .conversation_unread_counts
                .get(&cid)
                .copied()
                .unwrap_or(0);
            state.conversation_unread_counts.insert(cid, 0);

            if old_count > 0 {
                debug!(
                    "Reset unread count for conversation {} from {} to 0 (conversation opened)",
                    cid, old_count
                );
            }
        }

        if let Some(cached) = state.conversation_messages.get(&cid) {
            state.messages = cached.clone();

            if let Some(max_seq) = cached.iter().filter_map(|m| m.sequence_num).max() {
                state.conversation_sequences.insert(cid, max_seq);
                state.conversation_sequences_confirmed.insert(cid, max_seq);

                if let Err(e) = state.ui_to_net_tx.try_send(Outgoing::MarkRead {
                    conversation_id: cid,
                    sequence_num: max_seq,
                }) {
                    warn!("Failed to send mark_read: {}", e);
                } else {
                    info!(
                        "✅ Sent mark_read for conversation {} up to sequence {}",
                        cid, max_seq
                    );

                    if let Some(conversations) = &mut state.conversations {
                        if let Some(conv) = conversations.iter_mut().find(|c| c.id == cid) {
                            let old_last_read = conv.last_read_sequence;
                            conv.last_read_sequence = max_seq as i64;
                            debug!(
                                "Updated local last_read_sequence from {} to {}",
                                old_last_read, max_seq
                            );
                        }
                    }
                }
            }

            info!(
                "Loaded {} messages from cache for conversation {}",
                cached.len(),
                cid
            );
        }

        // ✅ RIMOSSO: Non viene più fatto fetch esplicito quando si apre un gruppo
        // I messaggi vengono caricati dalla cache o tramite LoadConversationMessages come per i DM
    }

    pub fn handle_conversation_deleted(state: &mut AppState, cid: Uuid) {
        info!("Conversation deleted: {}", cid);

        if let Some(ref mut conversations) = state.conversations {
            conversations.retain(|c| c.id != cid);
        }

        state.conversation_messages.remove(&cid);

        state.conversation_sequences.remove(&cid);
        state.conversation_sequences_confirmed.remove(&cid);
        state.is_recovering_messages.remove(&cid);

        if state.cid == Some(cid) {
            state.cid = None;
            state.messages.clear();
            state.page = Page::Conversations;
        }

        if state.is_dm_stub(cid) {
            state.remove_dm_stub(cid);
        }

        state.request_conversations_refresh = true;
    }

    pub fn handle_conversation_created(state: &mut AppState, cid: Uuid) {
        info!("Conversation created: {}", cid);
        state.cid = Some(cid);
        state.page = Page::Chat;
        state.messages.clear();
        state.request_conversations_refresh = true;
    }

    pub fn handle_dm_stub_created(state: &mut AppState, stub_id: Uuid, target_username: String) {
        info!("Creating DM stub: {} -> {}", stub_id, target_username);

        state.add_dm_stub(stub_id, target_username.clone());
        state.cid = Some(stub_id);
        state.page = Page::Chat;
        state.messages.clear();
        state.conversation_messages.insert(stub_id, Vec::new());
        state.conv_title = target_username.clone();

        let system_msg = MessageDto::system_message(format!(
            "Chat con {} pronta. Invia il primo messaggio per iniziare!",
            target_username
        ));
        state.messages.push(system_msg.clone());
        state
            .conversation_messages
            .get_mut(&stub_id)
            .map(|msgs| msgs.push(system_msg));
    }

    pub fn handle_conversations_loaded(state: &mut AppState, conversations: Vec<ConversationDto>) {
        info!("Loaded {} conversations", conversations.len());

        let mut stubs_to_remove = Vec::new();
        for (stub_id, target_username) in &state.dm_stubs {
            if conversations
                .iter()
                .any(|c| c.kind == "dm" && c.title == *target_username)
            {
                stubs_to_remove.push(*stub_id);
            }
        }

        for stub_id in stubs_to_remove {
            state.remove_dm_stub(stub_id);
        }

        state.conversations = Some(conversations);
    }

    pub fn handle_all_messages_loaded(
        state: &mut AppState,
        all_messages: std::collections::HashMap<Uuid, Vec<MessageDto>>,
    ) {
        let total_messages: usize = all_messages.values().map(|v| v.len()).sum();
        info!(
            "Loaded all messages: {} conversations, {} total messages",
            all_messages.len(),
            total_messages
        );

        state.conversation_messages = all_messages;

        for (conv_id, messages) in &state.conversation_messages {
            if let Some(max_seq) = messages.iter().filter_map(|m| m.sequence_num).max() {
                state.conversation_sequences.insert(*conv_id, max_seq);
                state
                    .conversation_sequences_confirmed
                    .insert(*conv_id, max_seq);
                debug!(
                    "Updated sequence for conversation {}: {} (from {} messages)",
                    conv_id,
                    max_seq,
                    messages.len()
                );
            }
        }

        if let Some(cid) = state.cid {
            if let Some(messages) = state.conversation_messages.get(&cid) {
                state.messages = messages.clone();
            }
        }

        debug!(
            "Sequence tracking initialized for {} conversations",
            state.conversation_sequences.len()
        );
    }

    pub fn handle_refreshed_msgs(state: &mut AppState, messages: Vec<MessageDto>) {
        debug!("Refreshed {} messages", messages.len());
        state.messages = messages.clone();

        if let Some(cid) = state.cid {
            state.conversation_messages.insert(cid, messages.clone());

            if let Some(max_seq) = messages.iter().filter_map(|m| m.sequence_num).max() {
                SequenceHandler::update_conversation_sequence(state, cid, max_seq);
            }
        }
    }

    pub fn handle_single_conversation_loaded(state: &mut AppState, conv: ConversationDto) {
        debug!("Single conversation loaded: {}", conv.id);

        if let Some(ref mut conversations) = state.conversations {
            if let Some(pos) = conversations.iter().position(|c| c.id == conv.id) {
                conversations[pos] = conv;
            } else {
                conversations.push(conv);
            }
        } else {
            state.conversations = Some(vec![conv]);
        }
    }

    pub fn handle_initial_load_complete(state: &mut AppState) {
        state.is_initial_load_complete = true;
        state.is_loading = false;
        info!("Initial data load complete");
    }

    pub fn handle_loading_progress(msg: String) {
        debug!("Loading progress: {}", msg);
    }

    pub fn handle_trigger_conversation_fetch(state: &mut AppState, cid: Uuid, reason: String) {
        debug!("Triggering conversation fetch for {}: {}", cid, reason);

        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversation_with_messages(&base, &token, cid)
                    .await
                {
                    Ok(conv_with_msgs) => {
                        // Invia i membri se presenti
                        if !conv_with_msgs.members.is_empty() {
                            let _ = tx.send(UiEvent::MembersLoaded(conv_with_msgs.members));
                        }
                        // Invia la conversazione e i messaggi
                        let _ = tx.send(UiEvent::ConversationCompleteFetched(
                            conv_with_msgs.conversation,
                            conv_with_msgs.messages,
                        ));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!(
                            "Failed to fetch conversation: {}",
                            e
                        )));
                    }
                }
            });
        }
    }

    pub fn handle_conversation_complete_fetched(
        state: &mut AppState,
        conv: ConversationDto,
        messages: Vec<MessageDto>,
    ) {
        debug!(
            "Fetched conversation {} with {} messages",
            conv.id,
            messages.len()
        );

        if let Some(ref mut conversations) = state.conversations {
            if let Some(pos) = conversations.iter().position(|c| c.id == conv.id) {
                conversations[pos] = conv.clone();
            } else {
                conversations.push(conv.clone());
            }
        } else {
            state.conversations = Some(vec![conv.clone()]);
        }

        let max_seq = messages
            .iter()
            .filter_map(|m| m.sequence_num)
            .max()
            .unwrap_or(0);

        if max_seq > 0 {
            state.conversation_sequences.insert(conv.id, max_seq);
            state
                .conversation_sequences_confirmed
                .insert(conv.id, max_seq);
            debug!(
                "Updated conversation {} sequence to {} from fetched messages",
                conv.id, max_seq
            );
        }

        state
            .conversation_messages
            .insert(conv.id, messages.clone());

        if state.cid == Some(conv.id) {
            state.messages = messages;
        }

        state.remove_dm_stub(conv.id);

        // Rimosso move_conversation_to_top per evitare che la chat venga spostata in alto al click
        // super::utils::move_conversation_to_top(state, conv.id);
    }

    pub fn handle_conversation_list_updated(state: &mut AppState) {
        debug!("Conversation list update requested");
        state.request_conversations_refresh = true;
    }
}
