use crate::app::events::helpers;
use crate::models::*;
use crate::state::core::AppState;
use tracing::{debug, info, warn};
use uuid::Uuid;
use crate::app::events::buffer_handler::BufferHandler;
use crate::app::events::sequence_handler::SequenceHandler;

pub struct UserNotificationHandler;

impl UserNotificationHandler {
    pub fn handle_user_notification(
        state: &mut AppState,
        sequence: u64,
        event_type: String,
        event_data: serde_json::Value,
        conversation_id: Option<Uuid>,
        recovery: bool,
    ) {
        let expected = state.user_sequence_confirmed + 1;

        if sequence > expected {
            warn!(
                "User event seq {} out of order (expected {}), buffering",
                sequence, expected
            );
            BufferHandler::buffer_user_event_for_reorder(state, sequence, event_data);
            return;
        } else if sequence < expected && sequence > 0 {
            debug!(
                "User event seq {} already processed (expected {}), skipping",
                sequence, expected
            );
            return;
        }

        if sequence > 0 {
            SequenceHandler::update_user_sequence(state, sequence);
        }

        Self::process_user_notification(state, sequence, event_type, event_data, conversation_id, recovery);

        debug!("User event seq {} processed normally", sequence);

        let buffered_events = BufferHandler::try_deliver_buffered_user_events(state);
        if !buffered_events.is_empty() {
            info!(
                "Delivering {} buffered user events after processing seq {}",
                buffered_events.len(),
                sequence
            );

            for buffered_event in buffered_events {
                if let Some(event_seq) = buffered_event.get("sequence").and_then(|s| s.as_u64()) {
                    SequenceHandler::update_user_sequence(state, event_seq);

                    let evt_type = buffered_event
                        .get("event_type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let conv_id = buffered_event
                        .get("conversation_id")
                        .and_then(|id| id.as_str())
                        .and_then(|s| Uuid::parse_str(s).ok());

                    Self::process_user_notification(state, event_seq, evt_type, buffered_event.clone(), conv_id, false);
                }
            }
        }
    }

    fn process_user_notification(
        state: &mut AppState,
        sequence: u64,
        event_type: String,
        event_data: serde_json::Value,
        conversation_id: Option<Uuid>,
        recovery: bool,
    ) {
        debug!(
            "User notification - seq: {}, type: {}, recovery: {}",
            sequence, event_type, recovery
        );

        if recovery {
            state.sequence_stats.events_recovered += 1;
            info!("Processing recovery event: seq {} type {}", sequence, event_type);
        }

        match event_type.as_str() {
            "new_message" => {
                Self::handle_new_message(state, event_data);
            }
            "conversation_deleted" => {
                if let Some(cid) = conversation_id {
                    info!(
                        "Applying conversation_deleted from UserNotification for {}",
                        cid
                    );
                    let _ = state.ui_tx.send(UiEvent::ConversationDeleted(cid));
                } else {
                    warn!("conversation_deleted notification without conversation_id");
                }
            }
            "member_kicked" => {
                if let Some(cid) = conversation_id {
                    info!(
                        "User was kicked from conversation {}",
                        cid
                    );
                    // Rimuovi la conversazione dalla lista
                    let _ = state.ui_tx.send(UiEvent::ConversationDeleted(cid));
                } else {
                    warn!("member_kicked notification without conversation_id");
                }
            }
            "member_added" => {
                // Gestisce quando un utente viene aggiunto al gruppo
                let username = event_data
                    .get("username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                info!("User {} added to group (conversation: {:?})", username, conversation_id);

                // Il messaggio di sistema viene ora salvato dal server e arriverà come messaggio normale
                // Non serve più creare un messaggio locale
                
            }
            "member_removed" => {
                // Gestisce quando un utente viene espulso dal gruppo
                let username = event_data
                    .get("username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                info!("User {} removed from group (conversation: {:?})", username, conversation_id);

                // Il messaggio di sistema viene ora salvato dal server e arriverà come messaggio normale
                // Non serve più creare un messaggio locale
                
            }
            "member_list_updated" => {
                // Aggiorna la lista dei membri in real-time
                if let Ok(members) = serde_json::from_value::<Vec<crate::models::ParticipantInfo>>(
                    event_data.get("members").cloned().unwrap_or(serde_json::Value::Array(vec![]))
                ) {
                    info!("Received member list update with {} members for conversation {:?}", members.len(), conversation_id);
                    for member in &members {
                        info!("  - {} ({})", member.username, member.role);
                    }
                    let _ = state.ui_tx.send(UiEvent::MembersLoaded(members));
                } else {
                    warn!("Failed to parse members from member_list_updated event");
                }
            }
            "user_left_group" => {
                // Gestisce quando un altro utente lascia il gruppo
                let username = event_data
                    .get("username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                info!("User {} left the group (conversation: {:?})", username, conversation_id);

                // Il messaggio di sistema viene ora salvato dal server e arriverà come messaggio normale
                // Non serve più creare un messaggio locale

                // Richiedi aggiornamento della lista conversazioni
                let _ = state.ui_tx.send(UiEvent::ConversationListUpdated);
            }
            "conversation_created_complete" => {
                Self::handle_conversation_created_complete(state, event_data);
            }
            "new_conversation" => {
                Self::handle_new_conversation(state, event_data);
            }
            "conversation_confirmation" => {
                Self::handle_conversation_confirmation_event(state, event_data);
            }
            _ => {
                debug!("Unhandled notification type: {}", event_type);
            }
        }

        if let Some(conv_id) = conversation_id {
            if let Some(current_cid) = state.cid {
                if current_cid == conv_id {
                    let seq = state
                        .conversation_sequences_confirmed
                        .get(&conv_id)
                        .copied()
                        .or_else(|| state.conversation_sequences.get(&conv_id).copied());

                    if let Some(seq) = seq {
                        let outgoing = Outgoing::MarkRead {
                            conversation_id: conv_id,
                            sequence_num: seq,
                        };
                        let _ = state.ui_to_net_tx.try_send(outgoing);
                        debug!("Auto mark_read for conversation {} up to seq {}", conv_id, seq);
                    }
                }
            }
        }
    }

    fn handle_new_message(state: &mut AppState, event_data: serde_json::Value) {
        let conversation_sequence = event_data
            .get("conversation_sequence")
            .and_then(|v| v.as_u64())
            .map(|seq| seq as u64);

        let message_data = event_data.get("message").unwrap_or(&event_data);

        if let Ok(mut msg) = serde_json::from_value::<MessageDto>(message_data.clone()) {
            if let Some(conv_seq) = conversation_sequence {
                msg.sequence_num = Some(conv_seq);
            }

            info!(
                "Processing new_message from UserEvent (msg_id: {}, conv_seq: {:?})",
                msg.id, msg.sequence_num
            );

            let already_in_cache = state
                .conversation_messages
                .get(&msg.conversation_id)
                .map(|cache| cache.iter().any(|m| m.id == msg.id))
                .unwrap_or(false);

            if already_in_cache {
                debug!(
                    "UserEvent message {} already in cache (from WebSocket), skipping",
                    msg.id
                );

                return;
            }

            if let Some(conv_seq) = msg.sequence_num {
                use super::sequence_handler::SequenceHandler;
                SequenceHandler::update_conversation_sequence(state, msg.conversation_id, conv_seq);
                debug!(
                    "Updated conversation {} sequence to {} via UserEvent",
                    msg.conversation_id, conv_seq
                );
            }

            let cache = state
                .conversation_messages
                .entry(msg.conversation_id)
                .or_insert_with(Vec::new);

            let cache_insert_pos = if let Some(_msg_seq) = msg.sequence_num {
                cache
                    .binary_search_by(|existing| {
                        match (existing.sequence_num, msg.sequence_num) {
                            (Some(e_seq), Some(m_seq)) => e_seq.cmp(&m_seq),
                            _ => existing.created_at.cmp(&msg.created_at)
                                .then_with(|| existing.id.cmp(&msg.id)),
                        }
                    })
                    .unwrap_or_else(|pos| pos)
            } else {
                cache
                    .binary_search_by(|existing| {
                        existing.created_at.cmp(&msg.created_at)
                            .then_with(|| existing.id.cmp(&msg.id))
                    })
                    .unwrap_or_else(|pos| pos)
            };

            cache.insert(cache_insert_pos, msg.clone());
            debug!(
                "UserEvent message {} cached at position {} for conversation {}",
                msg.id, cache_insert_pos, msg.conversation_id
            );

            if Some(msg.conversation_id) == state.cid {
                let already_in_ui = state.messages.iter().any(|m| m.id == msg.id);

                if !already_in_ui {
                    let ui_insert_pos = if let Some(_msg_seq) = msg.sequence_num {
                        state
                            .messages
                            .binary_search_by(|existing| {
                                match (existing.sequence_num, msg.sequence_num) {
                                    (Some(e_seq), Some(m_seq)) => e_seq.cmp(&m_seq),
                                    _ => existing.created_at.cmp(&msg.created_at)
                                        .then_with(|| existing.id.cmp(&msg.id)),
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
                        "Added UserEvent message {} to UI at position {} (seq: {:?})",
                        msg.id, ui_insert_pos, msg.sequence_num
                    );
                } else {
                    debug!("UserEvent message {} already in UI, skipping", msg.id);
                }
            } else {
                debug!(
                    "UserEvent message {} cached for conversation {} (not current: {:?})",
                    msg.id, msg.conversation_id, state.cid
                );

                if Some(msg.author_id) != state.user_id {
                    let current_unread = state
                        .conversation_unread_counts
                        .get(&msg.conversation_id)
                        .copied()
                        .unwrap_or(0);

                    state
                        .conversation_unread_counts
                        .insert(msg.conversation_id, current_unread + 1);

                    debug!(
                        "Incremented unread count for conversation {} from {} to {} (new message from {})",
                        msg.conversation_id,
                        current_unread,
                        current_unread + 1,
                        msg.author_username
                    );
                }
            }

            super::utils::move_conversation_to_top(state, msg.conversation_id);
        }
    }

    fn handle_conversation_created_complete(state: &mut AppState, event_data: serde_json::Value) {
        if let Some(conv_obj) = event_data.get("conversation") {
            let id = conv_obj
                .get("id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_else(Uuid::nil);

            let kind_str = conv_obj
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("direct_message");

            let kind = kind_str.to_string();

            let owner_id = conv_obj
                .get("owner_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_else(Uuid::nil);

            let created_at = conv_obj
                .get("created_at")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let title = conv_obj
                .get("display_title")
                .and_then(|t| t.as_str())
                .or_else(|| conv_obj.get("title").and_then(|t| t.as_str()))
                .unwrap_or("")
                .to_string();

            let last_read_sequence = conv_obj
                .get("last_read_sequence")
                .and_then(|s| s.as_i64())
                .unwrap_or(0);

            let last_message_time = conv_obj
                .get("last_message")
                .and_then(|msg| msg.get("created_at"))
                .and_then(|t| t.as_i64())
                .unwrap_or(created_at);

            let last_activity = std::cmp::max(created_at, last_message_time);

            let last_msg_seq = conv_obj
                .get("last_message")
                .and_then(|msg| msg.get("sequence_num"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let conversation = ConversationDto {
                id,
                kind,
                title: title.clone(),
                owner_id,
                created_at,
                last_read_sequence,
                last_activity,
                last_msg_seq,
            };

            let mut messages = Vec::new();
            if let Some(last_msg) = conv_obj.get("last_message") {
                let msg_id = last_msg
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(Uuid::new_v4);

                let msg_author_id = last_msg
                    .get("author_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(Uuid::nil);

                let msg_author_username = last_msg
                    .get("author_username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let msg_content = last_msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let msg_created_at = last_msg
                    .get("created_at")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                let msg_sequence = last_msg.get("sequence_num").and_then(|v| v.as_u64());

                messages.push(MessageDto {
                    id: msg_id,
                    author_id: msg_author_id,
                    author_username: msg_author_username.clone(),
                    conversation_id: id,
                    content: msg_content,
                    created_at: msg_created_at,
                    sequence_num: msg_sequence,
                    client_msg_id: None,
                    is_confirmed: Some(true),
                });
            }

            if state.dm_stubs.contains_key(&id) {
                state.remove_dm_stub(id);
                info!("Removed DM stub {} after receiving complete conversation", id);
            }

            if let Some(ref mut conversations) = state.conversations {
                conversations.retain(|c| c.id != id);
                conversations.push(conversation.clone());
            } else {
                state.conversations = Some(vec![conversation.clone()]);
            }

            if !messages.is_empty() {
                state.conversation_messages.insert(id, messages.clone());

                if state.cid == Some(id) {
                    state.messages = messages.clone();
                }
            }

            if let Some(first_msg) = messages.first() {
                if let Some(msg_seq) = first_msg.sequence_num {
                    let unread = std::cmp::max(0, msg_seq as i64 - conversation.last_read_sequence);

                    if first_msg.author_id != state.user_id.unwrap_or(Uuid::nil()) && unread > 0 {
                        state.conversation_unread_counts.insert(id, unread);

                        info!(
                            "New conversation '{}' has {} unread messages (author: {})",
                            conversation.title, unread, first_msg.author_username
                        );
                    } else {
                        state.conversation_unread_counts.insert(id, 0);
                    }
                } else {
                    state.conversation_unread_counts.insert(id, 0);
                }
            } else {
                state.conversation_unread_counts.insert(id, 0);
            }

            if state.page == Page::Chat && (state.cid == Some(id) || state.cid.is_none()) {
                state.cid = Some(id);
                state.conv_title = conversation.title.clone();

                state.conversation_unread_counts.insert(id, 0);

                if let Some(cached) = state.conversation_messages.get(&id) {
                    state.messages = cached.clone();
                }
            }

            helpers::add_system_message(
                state,
                format!("Nuova conversazione '{}' creata e sincronizzata", title),
            );

            info!("Successfully processed conversation_created_complete for {}", id);

            super::utils::move_conversation_to_top(state, id);
        } else {
            warn!("conversation_created_complete missing conversation object");
        }
    }

    fn handle_new_conversation(state: &mut AppState, event_data: serde_json::Value) {
        if let Some(conv_obj) = event_data.get("conversation") {
            let id = conv_obj
                .get("id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_else(Uuid::nil);

            let kind = conv_obj
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("group")
                .to_string();

            let owner_id = conv_obj
                .get("owner_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_else(Uuid::nil);

            let created_at = conv_obj
                .get("created_at")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            let title = conv_obj
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("Gruppo")
                .to_string();

            let last_read_sequence = conv_obj
                .get("last_read_sequence")
                .and_then(|s| s.as_i64())
                .unwrap_or(0);

            let last_activity = conv_obj
                .get("last_activity")
                .and_then(|t| t.as_i64())
                .unwrap_or(created_at);

            let last_msg_seq = conv_obj
                .get("last_msg_seq")
                .and_then(|s| s.as_i64())
                .unwrap_or(0);

            let conversation = ConversationDto {
                id,
                kind: kind.clone(),
                title: title.clone(),
                owner_id,
                created_at,
                last_read_sequence,
                last_activity,
                last_msg_seq,
            };

            // ✅ NUOVO: Gestione stub per gruppi
            let stub_to_replace = if kind == "group" {
                // Cerca stub usando client_temp_id
                conv_obj
                    .get("client_temp_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .filter(|stub_id| state.group_stubs.contains_key(stub_id))
            } else {
                None
            };

            // ✅ Sostituisci stub se trovato
            if let Some(stub_id) = stub_to_replace {
                info!("🔄 Replacing group stub {} with real group {}", stub_id, id);

                // Rimuovi stub dalla lista conversazioni
                if let Some(ref mut convs) = state.conversations {
                    convs.retain(|c| c.id != stub_id);
                }

                // Rimuovi stub dal tracking
                state.group_stubs.remove(&stub_id);

                // Sposta messaggi dallo stub al gruppo reale (escludi messaggi di sistema)
                if let Some(stub_messages) = state.conversation_messages.remove(&stub_id) {
                    let real_messages: Vec<_> = stub_messages
                        .into_iter()
                        .filter(|m| !m.is_system_message())
                        .collect();

                    if !real_messages.is_empty() {
                        info!("📦 Moving {} messages from stub to real group", real_messages.len());
                        state
                            .conversation_messages
                            .entry(id)
                            .or_insert_with(Vec::new)
                            .extend(real_messages);
                    }
                }

                // Rimuovi altre entry dello stub
                state.conversation_unread_counts.remove(&stub_id);
                state.conversation_sequences.remove(&stub_id);
                state.conversation_sequences_confirmed.remove(&stub_id);

                // ✅ Aggiorna cid se stavi visualizzando lo stub
                if state.cid == Some(stub_id) {
                    state.cid = Some(id);
                    state.conv_title = title.clone();
                    state.page = Page::Chat;

                    // Aggiorna anche la vista corrente
                    state.messages = state
                        .conversation_messages
                        .get(&id)
                        .cloned()
                        .unwrap_or_default();

                    info!("👁️ Switched view from stub {} to real group {}", stub_id, id);
                }

                info!("✅ Stub {} replaced with real group {}", stub_id, id);
            }

            info!("📩 Aggiunta alla conversazione '{}' ({})", conversation.title, id);

            // Aggiungi la conversazione reale alla lista
            if let Some(ref mut convs) = state.conversations {
                // Rimuovi duplicati per sicurezza
                convs.retain(|c| c.id != id);

                if stub_to_replace.is_some() {
                    // Se sostituisci uno stub, metti all'inizio
                    convs.insert(0, conversation.clone());
                } else {
                    // Altrimenti aggiungi normalmente
                    convs.push(conversation.clone());
                }

                // Inizializza entry se non esistono
                state.conversation_unread_counts.entry(id).or_insert(0);
                state.conversation_sequences.entry(id).or_insert(0);
                state.conversation_sequences_confirmed.entry(id).or_insert(0);
                state.conversation_messages.entry(id).or_insert_with(Vec::new);

                if stub_to_replace.is_none() {
                    // Solo se NON è uno stub, mostra il messaggio di sistema
                    helpers::add_system_message(
                        state,
                        format!("✅ Sei stato aggiunto al gruppo '{}'", conversation.title),
                    );
                }

                super::utils::move_conversation_to_top(state, id);

                info!("New conversation '{}' added to list", conversation.title);
            }
        } else {
            warn!("new_conversation event missing conversation object");
        }
    }

    /// Gestisce conversation_confirmation da UserNotification
    fn handle_conversation_confirmation_event(
        state: &mut AppState,
        event_data: serde_json::Value,
    ) {
        use crate::models::{ConversationDto, MessageDto};
        use uuid::Uuid;

        debug!("Parsing conversation_confirmation from event_data...");

        // Parsa i dati della conversazione
        let conversation_data = event_data.get("conversation").and_then(|conv_obj| {
            debug!("Found conversation object in event_data");

            // Parse UUID fields
            let id_str = conv_obj.get("id")?.as_str()?;
            let id = Uuid::parse_str(id_str).ok()?;
            debug!("Parsed conversation id: {}", id);

            let owner_id_str = conv_obj.get("owner_id")?.as_str()?;
            let owner_id = Uuid::parse_str(owner_id_str).ok()?;
            debug!("Parsed owner_id: {}", owner_id);

            let client_temp_id = conv_obj
                .get("client_temp_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(ref temp_id) = client_temp_id {
                info!("Received conversation_confirmation for temp_id: {}", temp_id);
            } else {
                warn!("No client_temp_id found in conversation_confirmation");
            }

            let kind = conv_obj.get("kind")?.as_str()?.to_string();
            debug!("Parsed kind: {}", kind);

            let created_at = conv_obj.get("created_at")?.as_i64()?;
            debug!("Parsed created_at: {}", created_at);

            let title = conv_obj
                .get("display_title")
                .and_then(|t| t.as_str())
                .or_else(|| conv_obj.get("title").and_then(|t| t.as_str()))
                .unwrap_or("")
                .to_string();
            debug!("Parsed conversation title: {}", title);

            let last_read_sequence = conv_obj
                .get("last_read_sequence")
                .and_then(|s| s.as_i64())
                .unwrap_or(0);

            // Calcola last_activity
            let last_message_time = conv_obj
                .get("last_message")
                .and_then(|msg| msg.get("created_at"))
                .and_then(|t| t.as_i64())
                .unwrap_or(0);

            let last_activity = std::cmp::max(created_at, last_message_time);

            let last_msg_seq = conv_obj
                .get("last_message")
                .and_then(|msg| msg.get("sequence_num"))
                .and_then(|t| t.as_i64())
                .unwrap_or(0);

            let conversation = ConversationDto {
                id,
                kind,
                title,
                owner_id,
                created_at,
                last_read_sequence,
                last_activity,
                last_msg_seq,
            };

            // Parse dell'ultimo messaggio se presente
            let messages = if let Some(last_msg) = conv_obj.get("last_message") {
                debug!("Parsing last_message from conversation_confirmation");
                Self::parse_message_from_event(last_msg, id)
                    .map(|m| vec![m])
                    .unwrap_or_default()
            } else {
                debug!("No last_message in conversation_confirmation");
                Vec::new()
            };

            debug!("Successfully parsed conversation_confirmation data");
            Some((conversation, messages, client_temp_id))
        });

        if let Some((conv, messages, client_temp_id)) = conversation_data {
            info!(
                "Conversation confirmed: {} (temp_id: {:?})",
                conv.id, client_temp_id
            );

            // Chiama il conversation_handler per gestire lo stub e aggiungere la conversazione
            super::conversation_handler::ConversationHandler::handle_conversation_confirmed(
                state,
                conv,
                messages,
                client_temp_id,
            );
        } else {
            warn!("Invalid conversation_confirmation structure - parsing failed");
            warn!("Raw event_data: {:?}", event_data);
        }
    }

    /// Helper per parsare un messaggio da JSON
    fn parse_message_from_event(msg_obj: &serde_json::Value, conversation_id: uuid::Uuid) -> Option<MessageDto> {
        let id_str = msg_obj.get("id")?.as_str()?;
        let id = uuid::Uuid::parse_str(id_str).ok()?;

        let author_id_str = msg_obj.get("author_id")?.as_str()?;
        let author_id = uuid::Uuid::parse_str(author_id_str).ok()?;

        let author_username = msg_obj.get("author_username")?.as_str()?.to_string();
        let content = msg_obj.get("content")?.as_str()?.to_string();
        let created_at = msg_obj.get("created_at")?.as_i64()?;
        let sequence_num = msg_obj.get("sequence_num").and_then(|s| s.as_i64()).map(|s| s as u64);

        Some(MessageDto {
            id,
            author_id,
            conversation_id,
            author_username,
            content,
            created_at,
            sequence_num,
            client_msg_id: None,
            is_confirmed: Some(true),
        })
    }
}