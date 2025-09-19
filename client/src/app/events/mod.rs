mod auth_handler;
mod conversation_handler;
mod data_handler;
mod fetch_handler;
mod helpers;
mod message_handler;
mod websocket_handler;

use crate::api::ws::WsControl;
use crate::models::*;
use crate::state::data_loader::DataLoader;
use crate::state::AppState;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct EventDispatcher;

impl EventDispatcher {
    pub fn handle_event(state: &mut AppState, event: UiEvent) {
        match event {
            // ===== AUTH EVENTS =====
            UiEvent::LoginStarted => {
                state.login_state = LoginState::LoggingIn;
                info!("Login started");
            }

            UiEvent::RegisterStarted => {
                state.login_state = LoginState::Registering;
                info!("Registration started");
            }

            UiEvent::Logged(token, user_id, last_sequence) => {
                info!(
                    "User logged in - user_id: {}, initial sequence: {}",
                    user_id, last_sequence
                );

                state.token = Some(token.clone());
                state.user_id = Some(user_id);

                // Initialize dual sequence system
                state.user_sequence_confirmed = last_sequence;
                state.user_sequence_received = last_sequence;
                state.conversation_sequences.clear();
                state.conversation_sequences_confirmed.clear();

                state.login_state = LoginState::LoggedIn;
                state.page = Page::Conversations;
                state.sequence_stats = Default::default();
                state.request_ws_reconnect = true;
            }

            UiEvent::LoggedOut => {
                info!("User logged out");

                if let Some(ctrl) = state.ws_ctrl.take() {
                    let _ = ctrl.shutdown.send(());
                }

                // Reset all state
                state.token = None;
                state.user_id = None;
                state.page = Page::Auth;
                state.cid = None;
                state.messages.clear();
                state.conversations = None;
                state.conversation_messages.clear();
                state.login_state = LoginState::Idle;
                state.ws_status = WsStatus::Disconnected;
                state.dm_stubs.clear();

                // Reset dual sequence system
                state.user_sequence_confirmed = 0;
                state.user_sequence_received = 0;
                state.conversation_sequences.clear();
                state.conversation_sequences_confirmed.clear();
                state.sequence_stats = Default::default();
            }

            // ===== WEBSOCKET EVENTS =====
            UiEvent::WsConnected => {
                state.ws_status = WsStatus::Connected;
                state.reset_sequence_system();
                info!("WebSocket connected, sequence system active");
            }

            UiEvent::WsDisconnected => {
                state.ws_status = WsStatus::Disconnected;
                state.reset_sequence_on_disconnect();
                warn!("WebSocket disconnected");
            }

            UiEvent::WsControlReady(ctrl) => {
                state.ws_ctrl = Some(ctrl);
                debug!("WebSocket control ready");
            }

            UiEvent::WsError(err) => {
                error!("WebSocket error: {}", err);
            }

            UiEvent::WsIncoming(msg) => {
                debug!(
                    "Incoming message for conversation {} (seq: {:?})",
                    msg.conversation_id, msg.sequence_num
                );

                // Update conversation sequence if present
                if let Some(seq) = msg.sequence_num {
                    state.update_conversation_sequence(msg.conversation_id, seq);
                }

                // Add to current conversation if active
                if state.cid == Some(msg.conversation_id) {
                    if !state.messages.iter().any(|m| m.id == msg.id) {
                        state.messages.push(msg.clone());
                    }
                }

                // Update cache
                let cache = state
                    .conversation_messages
                    .entry(msg.conversation_id)
                    .or_insert_with(Vec::new);

                if !cache.iter().any(|m| m.id == msg.id) {
                    cache.push(msg.clone());
                }

                // Request refresh if unknown conversation
                if let Some(ref conversations) = state.conversations {
                    if !conversations.iter().any(|c| c.id == msg.conversation_id) {
                        debug!(
                            "Message for unknown conversation {}, requesting refresh",
                            msg.conversation_id
                        );
                        state.request_conversations_refresh = true;
                    }
                }
            }

            // ===== ENHANCED PONG EVENT =====
            UiEvent::EnhancedPongReceived {
                current_user_sequence,
                conversation_sequences,
                gaps_detected,
                user_events_gap,
                message_gap,
            } => {
                debug!(
                    "Enhanced pong - server_user_seq: {}, gaps_detected: {}",
                    current_user_sequence, gaps_detected
                );

                state.sequence_stats.pong_count += 1;
                state.missed_pings = 0;

                // IMPORTANTE: Il pong serve SOLO per decidere se fare resume
                // NON aggiorniamo MAI le sequence locali dal pong!

                // Conta i gap prima di consumare le Option
                let gap_count = (if user_events_gap.as_ref().map_or(false, |g| g.detected) {
                    1
                } else {
                    0
                }) + (if message_gap.as_ref().map_or(false, |g| g.detected) {
                    1
                } else {
                    0
                });

                // Check for user events gap (il server ha già fatto il confronto)
                if let Some(gap) = user_events_gap {
                    if gap.detected {
                        warn!("User events gap detected by server: {} events missing (client_seq: {}, server_seq: {})",
                              gap.gap_size, gap.client_seq, gap.server_seq);
                        state.sequence_stats.gaps_detected += 1;

                        // Request resume for user events dalla sequence del client
                        state.request_user_events_resume(gap.client_seq);
                    }
                }

                // Check for message gap nella conversazione corrente
                if let Some(gap) = message_gap {
                    if gap.detected {
                        if let Some(cid) = state.cid {
                            warn!("Messages gap detected by server for {}: {} messages missing (client_seq: {}, server_seq: {})",
                                  cid, gap.gap_size, gap.client_seq, gap.server_seq);

                            // Request resume for messages dalla sequence del client
                            state.request_messages_resume(cid, gap.client_seq);
                        }
                    }
                }

                // Opzionale: logging per debug (senza modificare nulla)
                if let Some(conv_seqs) = conversation_sequences {
                    for (conv_id_str, server_seq) in conv_seqs {
                        if let Ok(conv_id) = Uuid::parse_str(&conv_id_str) {
                            let local_seq = state
                                .conversation_sequences
                                .get(&conv_id)
                                .copied()
                                .unwrap_or(0);

                            if server_seq != local_seq {
                                debug!(
                                    "Sequence mismatch detected for conversation {}: server={}, local={} (gap: {})",
                                    conv_id, server_seq, local_seq,
                                    if server_seq > local_seq { server_seq - local_seq } else { 0 }
                                );
                            }
                        }
                    }
                }

                if gaps_detected && gap_count > 0 {
                    info!("Pong reported {} gap(s), resume requests sent", gap_count);
                }
            }

            // ===== RESUME EVENTS =====
            UiEvent::UserEventsResume { events } => {
                info!("Processing {} resumed user events", events.len());

                state.is_recovering_user_events = false;
                state.pending_resume_requests = state.pending_resume_requests.saturating_sub(1);
                state.sequence_stats.events_recovered += events.len() as u32;

                for event in events {
                    // Update sequence tracking
                    state.update_user_sequence(event.sequence);

                    // Process event based on type
                    let _ = state.ui_tx.send(UiEvent::UserNotification {
                        sequence: event.sequence,
                        event_type: event.event_type,
                        event_data: event.event_data,
                        conversation_id: event.conversation_id,
                        recovery: true,
                    });
                }
            }

            UiEvent::MessagesResume {
                conversation_id,
                messages,
            } => {
                info!(
                    "Processing {} resumed messages for {}",
                    messages.len(),
                    conversation_id
                );

                state.is_recovering_messages.insert(conversation_id, false);
                state.pending_resume_requests = state.pending_resume_requests.saturating_sub(1);

                // Trova la sequence massima nei messaggi resumed
                let max_resumed_seq = messages
                    .iter()
                    .filter_map(|m| m.sequence_num)
                    .max()
                    .unwrap_or(0);

                // Add messages to cache
                {
                    let cache = state
                        .conversation_messages
                        .entry(conversation_id)
                        .or_insert_with(Vec::new);

                    for msg in &messages {
                        if !cache.iter().any(|m| m.id == msg.id) {
                            cache.push(msg.clone());
                        }
                    }

                    // Sort messages by sequence first, then timestamp
                    cache.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                        (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                        _ => a.created_at.cmp(&b.created_at),
                    });
                }

                // Aggiorna la sequence solo se è maggiore di quella attuale
                let current_seq = state
                    .conversation_sequences
                    .get(&conversation_id)
                    .copied()
                    .unwrap_or(0);

                if max_resumed_seq > current_seq {
                    state
                        .conversation_sequences
                        .insert(conversation_id, max_resumed_seq);
                    state
                        .conversation_sequences_confirmed
                        .insert(conversation_id, max_resumed_seq);
                    debug!(
                        "Updated conversation {} sequence to {} after resume",
                        conversation_id, max_resumed_seq
                    );
                }

                // Update UI if current conversation
                if state.cid == Some(conversation_id) {
                    if let Some(cache) = state.conversation_messages.get(&conversation_id) {
                        state.messages = cache.clone();
                    }
                }
            }

            // ===== CONVERSATION EVENTS =====
            UiEvent::Opened(cid) => {
                info!("Opening conversation: {}", cid);
                state.cid = Some(cid);
                state.page = Page::Chat;

                // Controlla se abbiamo messaggi cached
                if let Some(cached) = state.conversation_messages.get(&cid) {
                    state.messages = cached.clone();

                    if !cached.is_empty() {
                        // Prova a estrarre le sequence dai messaggi cached
                        let max_seq = cached.iter().filter_map(|m| m.sequence_num).max();

                        if let Some(seq) = max_seq {
                            // Abbiamo sequence valide - usa quella
                            state.conversation_sequences.insert(cid, seq);
                            state.conversation_sequences_confirmed.insert(cid, seq);

                            debug!(
                                "Initialized conversation {} with sequence {} from cached messages",
                                cid, seq
                            );
                        } else {
                            // Messaggi cached senza sequence - triggera fetch per ottenere sequence
                            debug!(
                                "Conversation {} has cached messages without sequences - fetching",
                                cid
                            );
                            state.load_single_conversation_messages(cid);
                            // NON settare sequence - aspetta la fetch
                        }
                    } else {
                        // Cache vuota - carica messaggi
                        if !state.is_dm_stub(cid) {
                            state.load_single_conversation_messages(cid);
                        }
                    }
                } else {
                    // Nessuna cache - carica messaggi
                    state.messages.clear();
                    if !state.is_dm_stub(cid) {
                        state.load_single_conversation_messages(cid);
                    }
                }
            }

            UiEvent::ConversationCreated(cid) => {
                info!("Conversation created: {}", cid);
                state.cid = Some(cid);
                state.page = Page::Chat;
                state.messages.clear();
                state.request_conversations_refresh = true;
            }

            UiEvent::DmStubCreated(stub_id, target_username) => {
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

            // ===== DATA LOADING EVENTS =====
            UiEvent::ConversationsLoaded(conversations) => {
                info!("Loaded {} conversations", conversations.len());

                // Remove obsolete stubs
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

            UiEvent::AllMessagesLoaded(all_messages) => {
                let total_messages: usize = all_messages.values().map(|v| v.len()).sum();
                info!(
                    "Loaded all messages: {} conversations, {} total messages",
                    all_messages.len(),
                    total_messages
                );

                state.conversation_messages = all_messages;

                // Aggiorna le sequence per tutte le conversazioni caricate
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

                // Se c'è una conversazione attiva, aggiorna i messaggi UI
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

            UiEvent::RefreshedMsgs(messages) => {
                debug!("Refreshed {} messages", messages.len());
                state.messages = messages.clone();

                if let Some(cid) = state.cid {
                    // Aggiorna la cache
                    state.conversation_messages.insert(cid, messages.clone());

                    // Aggiorna le sequence per questa conversazione
                    if let Some(max_seq) = messages.iter().filter_map(|m| m.sequence_num).max() {
                        state.conversation_sequences.insert(cid, max_seq);
                        state.conversation_sequences_confirmed.insert(cid, max_seq);
                        debug!(
                            "Updated sequence for conversation {} to {} after refresh",
                            cid, max_seq
                        );
                    }
                }
            }

            UiEvent::SingleConversationLoaded(conv) => {
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

            UiEvent::InitialLoadComplete => {
                state.is_initial_load_complete = true;
                state.is_loading = false;
                info!("Initial data load complete");
            }

            UiEvent::LoadingProgress(msg) => {
                debug!("Loading progress: {}", msg);
            }

            // ===== MESSAGE EVENTS =====
            UiEvent::MessageSendFailed(msg_id) => {
                warn!("Message send failed: {}", msg_id);

                if let Some(pos) = state.messages.iter().position(|m| m.id == msg_id) {
                    state.messages.remove(pos);
                }

                if let Some(cid) = state.cid {
                    if let Some(messages) = state.conversation_messages.get_mut(&cid) {
                        if let Some(pos) = messages.iter().position(|m| m.id == msg_id) {
                            messages.remove(pos);
                        }
                    }
                }
            }

            // ===== GENERAL EVENTS =====
            UiEvent::Info(msg) => {
                info!("Info: {}", msg);
            }

            UiEvent::Error(msg) => {
                error!("Error: {}", msg);
            }

            UiEvent::InviteCreated(token) => {
                info!("Invite created: {}", token);
                state.last_created_invite = Some(token);
            }

            UiEvent::TriggerConversationFetch(cid, reason) => {
                debug!("Triggering conversation fetch for {}: {}", cid, reason);

                if let Some(ref token) = state.token {
                    let base = state.base.clone();
                    let token = token.clone();
                    let tx = state.ui_tx.clone();

                    state.rt.spawn(async move {
                        match crate::api::conversation::get_conversation_with_messages(
                            &base, &token, cid,
                        )
                        .await
                        {
                            Ok(conv_with_msgs) => {
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

            UiEvent::ConversationCompleteFetched(conv, messages) => {
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

                // Extract sequences from messages and update tracking
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
            }

            UiEvent::ConversationListUpdated => {
                debug!("Conversation list update requested");
                state.request_conversations_refresh = true;
            }

            // ===== MANUAL PING =====
            UiEvent::SendPing => {
                debug!("Manual ping requested");
                state.send_enhanced_ping();
            }

            // ===== USER NOTIFICATIONS =====
            UiEvent::UserNotification {
                sequence,
                event_type,
                event_data,
                conversation_id,
                recovery,
            } => {
                debug!(
                    "User notification - seq: {}, type: {}, recovery: {}",
                    sequence, event_type, recovery
                );

                // Check for duplicate based on sequence
                if sequence > 0 && sequence <= state.user_sequence_confirmed {
                    debug!(
                        "Ignoring duplicate event with sequence {} (already confirmed up to {})",
                        sequence, state.user_sequence_confirmed
                    );
                    return;
                }

                // Update user sequence
                state.update_user_sequence(sequence);

                if recovery {
                    state.sequence_stats.events_recovered += 1;
                    info!(
                        "Processing recovery event: seq {} type {}",
                        sequence, event_type
                    );
                }

                // Process based on event type
                match event_type.as_str() {
                    "new_message" => {
                        if let Ok(msg) = serde_json::from_value::<MessageDto>(event_data) {
                            // Check for duplicate message
                            if state.cid == Some(msg.conversation_id) {
                                if !state.messages.iter().any(|m| m.id == msg.id) {
                                    state.messages.push(msg.clone());
                                    debug!("Added new message to UI");
                                } else {
                                    debug!("Ignoring duplicate message {}", msg.id);
                                }
                            }

                            // Update cache
                            let messages = state
                                .conversation_messages
                                .entry(msg.conversation_id)
                                .or_insert_with(Vec::new);

                            if !messages.iter().any(|m| m.id == msg.id) {
                                messages.push(msg);
                                debug!("Added message to cache");
                            }
                        }
                    }
                    "conversation_created" => {
                        if let Some(cid) = conversation_id {
                            info!("New conversation created: {}, fetching with messages", cid);

                            let _ = state.ui_tx.send(UiEvent::TriggerConversationFetch(
                                cid,
                                "new_conversation_created".to_string(),
                            ));

                            // Remove stub if it exists
                            if state.dm_stubs.contains_key(&cid) {
                                state.remove_dm_stub(cid);
                            }
                        }
                    }
                    _ => {
                        debug!("Unhandled notification type: {}", event_type);
                    }
                }
            }
        }
    }
}
