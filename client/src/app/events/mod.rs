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

            // ===== NUOVI HANDLER PER INITIAL_STATE =====
            UiEvent::InitialStateReceived {
                conversations,
                user_sequence,
            } => {
                info!(
                    "Processing initial state: {} conversations, user_seq: {}",
                    conversations.len(),
                    user_sequence
                );

                // Aggiorna la sequenza utente
                state.user_sequence_confirmed = user_sequence;
                state.user_sequence_received = user_sequence;

                // Salva le conversazioni
                state.conversations = Some(conversations.clone());

                // Pulisci eventuali DM stubs che ora esistono come conversazioni reali
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

                // Notifica UI
                helpers::add_system_message(
                    state,
                    format!("Sincronizzate {} conversazioni", conversations.len()),
                );

                // Richiedi refresh se necessario
                if state.cid.is_some() && state.messages.is_empty() {
                    state.request_conversations_refresh = true;
                }
            }

            UiEvent::LastMessageUpdate {
                conversation_id,
                message,
            } => {
                debug!("Updating last message for conversation {}", conversation_id);

                // Aggiorna la cache dei messaggi
                let cache = state
                    .conversation_messages
                    .entry(conversation_id)
                    .or_insert_with(Vec::new);

                // Aggiungi solo se non esiste già
                if !cache.iter().any(|m| m.id == message.id) {
                    // Inserisci come ultimo messaggio
                    cache.push(message.clone());
                }

                // Aggiorna sequence per questa conversazione se presente
                if let Some(seq) = message.sequence_num {
                    //state.conversation_sequences.insert(conversation_id, seq);
                    //state
                       // .conversation_sequences_confirmed
                        //.insert(conversation_id, seq);
                    debug!(
                        "Updated conversation {} sequence to {} from last message",
                        conversation_id, seq
                    );
                }

                // Se è la conversazione corrente e non ci sono messaggi, mostra questo
                if state.cid == Some(conversation_id) && state.messages.is_empty() {
                    state.messages = vec![message];
                }
            }

            UiEvent::ConversationMessagesReceived {
                conversation_id,
                messages,
                has_more,
            } => {
                info!(
                    "Received {} messages for conversation {} (has_more: {})",
                    messages.len(),
                    conversation_id,
                    has_more
                );

                // DEBUG: Mostra le sequence ricevute
                let sequences: Vec<u64> = messages.iter().filter_map(|m| m.sequence_num).collect();
                info!("Message sequences received: {:?}", sequences);

                // Ordina i messaggi PRIMA di salvarli
                let mut sorted_messages = messages.clone();
                sorted_messages.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                    (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                    _ => a.created_at.cmp(&b.created_at),
                });

                // Aggiorna la cache con i messaggi ordinati
                state
                    .conversation_messages
                    .insert(conversation_id, sorted_messages.clone());

                // Trova la sequenza massima
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

                // Se è la conversazione corrente, aggiorna UI con messaggi ordinati
                if state.cid == Some(conversation_id) {
                    state.messages = sorted_messages; // Usa i messaggi ordinati

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

                // Il pong serve SOLO per decidere se fare resume
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

                // Check for user events gap
                if let Some(gap) = user_events_gap {
                    if gap.detected {
                        warn!("User events gap detected by server: {} events missing (client_seq: {}, server_seq: {})",
                              gap.gap_size, gap.client_seq, gap.server_seq);
                        state.sequence_stats.gaps_detected += 1;
                        state.request_user_events_resume(gap.client_seq);
                    }
                }

                // Check for message gap nella conversazione corrente
                if let Some(gap) = message_gap {
                    if gap.detected {
                        if let Some(cid) = state.cid {
                            warn!("Messages gap detected by server for {}: {} messages missing (client_seq: {}, server_seq: {})",
                                  cid, gap.gap_size, gap.client_seq, gap.server_seq);
                            state.request_messages_resume(cid, gap.client_seq);
                        }
                    }
                }

                // Debug logging per sequenze conversazioni
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
                    state.update_user_sequence(event.sequence);

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
                info!("Processing {} resumed messages for {}", messages.len(), conversation_id);

                state.is_recovering_messages.insert(conversation_id, false);
                state.pending_resume_requests = state.pending_resume_requests.saturating_sub(1);

                // Raccogli tutti gli ID dei messaggi esistenti (cache + UI)
                let mut existing_ids = std::collections::HashSet::new();

                // ID dalla cache
                if let Some(cache) = state.conversation_messages.get(&conversation_id) {
                    for msg in cache {
                        existing_ids.insert(msg.id);
                    }
                }

                // ID dall'UI se è la conversazione corrente
                if state.cid == Some(conversation_id) {
                    for msg in &state.messages {
                        existing_ids.insert(msg.id);
                    }
                }

                // Filtra solo i messaggi veramente nuovi
                let truly_new_messages: Vec<MessageDto> = messages
                    .into_iter()
                    .filter(|msg| !existing_ids.contains(&msg.id))
                    .collect();

                if truly_new_messages.is_empty() {
                    debug!("All resumed messages already exist, skipping");
                    return;
                }

                info!("Adding {} truly new messages from resume", truly_new_messages.len());

                // Aggiorna cache con i nuovi messaggi
                {
                    let cache = state
                        .conversation_messages
                        .entry(conversation_id)
                        .or_insert_with(Vec::new);

                    cache.extend(truly_new_messages.clone());

                    // Riordina per sequence
                    cache.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                        (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                        _ => a.created_at.cmp(&b.created_at),
                    });
                }

                // Aggiorna sequence
                let max_resumed_seq = truly_new_messages
                    .iter()
                    .filter_map(|m| m.sequence_num)
                    .max()
                    .unwrap_or(0);

                if max_resumed_seq > 0 {
                    let current_seq = state
                        .conversation_sequences
                        .get(&conversation_id)
                        .copied()
                        .unwrap_or(0);

                    if max_resumed_seq > current_seq {
                        state.conversation_sequences.insert(conversation_id, max_resumed_seq);
                        state.conversation_sequences_confirmed.insert(conversation_id, max_resumed_seq);
                        debug!("Updated conversation {} sequence to {} after resume",
                   conversation_id, max_resumed_seq);
                    }
                }

                // Aggiorna UI se è la conversazione corrente
                if state.cid == Some(conversation_id) {
                    // Aggiungi solo i nuovi messaggi all'UI
                    state.messages.extend(truly_new_messages);

                    // Riordina
                    state.messages.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                        (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                        _ => a.created_at.cmp(&b.created_at),
                    });
                }
            }

            // ===== CONVERSATION EVENTS =====
            UiEvent::OlderMessagesLoaded(new_messages) => {
                state.is_loading_more = false;

                let Some(cid) = state.cid else { return };

                // Se ritornati 0 messaggi, interrompi il caricamento
                if new_messages.is_empty() {
                    state.has_more_messages.insert(cid, false);
                    // Rimuovi lo spinner mostrando che non ci sono messaggi
                    return;
                }

                // Se meno di 30, non ce ne sono più
                if new_messages.len() < 30 {
                    state.has_more_messages.insert(cid, false);
                }

                // Combina messaggi
                let mut all_messages = Vec::new();
                all_messages.extend(new_messages);
                all_messages.extend(state.messages.clone());

                // Rimuovi duplicati
                let mut seen_ids = std::collections::HashSet::new();
                all_messages.retain(|msg| seen_ids.insert(msg.id));

                // Ordina
                all_messages.sort_by(|a, b| {
                    match (a.sequence_num, b.sequence_num) {
                        (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                        _ => a.created_at.cmp(&b.created_at),
                    }
                });

                state.messages = all_messages.clone();
                state.conversation_messages.insert(cid, all_messages);
            }

            UiEvent::LoadingError => {
                state.is_loading_more = false;
            }

            UiEvent::Opened(cid) => {
                info!("Opening conversation: {}", cid);

                state.cid = Some(cid);
                state.page = Page::Chat;

                // Resetta lo stato dei messaggi
                state.messages.clear();
                state.is_loading_more = false;

                // Inizializza has_more_messages per questa conversazione
                state.has_more_messages.insert(cid, true);

                // Inizializza le sequence per questa conversazione
                state.conversation_sequences.entry(cid).or_insert(0);
                state.conversation_sequences_confirmed.entry(cid).or_insert(0);

                // IMPORTANTE: Carica dalla cache se disponibile
                if let Some(cached) = state.conversation_messages.get(&cid) {
                    state.messages = cached.clone();

                    // Aggiorna sequence dall'ultimo messaggio cached
                    if let Some(max_seq) = cached.iter().filter_map(|m| m.sequence_num).max() {
                        state.conversation_sequences.insert(cid, max_seq);
                        state.conversation_sequences_confirmed.insert(cid, max_seq);
                    }

                    info!("Loaded {} messages from cache for conversation {}", cached.len(), cid);
                }
            }


            UiEvent::ConversationDeleted(cid) => {
                info!("Conversation deleted: {}", cid);

                // Rimuovi dalla lista conversazioni
                if let Some(ref mut conversations) = state.conversations {
                    conversations.retain(|c| c.id != cid);
                }

                // Rimuovi cache messaggi
                state.conversation_messages.remove(&cid);

                // Pulisci tracking sequence e resume per questa conversazione
                state.conversation_sequences.remove(&cid);
                state.conversation_sequences_confirmed.remove(&cid);
                state.is_recovering_messages.remove(&cid);

                // Se era la conversazione corrente, chiudi la vista chat
                if state.cid == Some(cid) {
                    state.cid = None;
                    state.messages.clear();
                    state.page = Page::Conversations;
                    // Se nel tuo AppState hai anche un titolo corrente:
                    // state.conv_title.clear();
                }

                // Rimuovi eventuale stub DM locale
                if state.is_dm_stub(cid) {
                    state.remove_dm_stub(cid);
                }

                // Opzionale: richiedi un refresh lista (se necessario)
                // state.request_conversations_refresh = true;
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
                    state.conversation_messages.insert(cid, messages.clone());

                    if let Some(max_seq) = messages.iter().filter_map(|m| m.sequence_num).max() {
                        // Usa update_conversation_sequence per rilevare gap!
                        state.update_conversation_sequence(cid, max_seq);
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

                if sequence > 0 && sequence <= state.user_sequence_confirmed {
                    debug!(
                        "Ignoring duplicate event with sequence {} (already confirmed up to {})",
                        sequence, state.user_sequence_confirmed
                    );
                    return;
                }

                state.update_user_sequence(sequence);

                if recovery {
                    state.sequence_stats.events_recovered += 1;
                    info!(
                        "Processing recovery event: seq {} type {}",
                        sequence, event_type
                    );
                }

                match event_type.as_str() {
                    "new_message" => {
                        if let Ok(msg) = serde_json::from_value::<MessageDto>(event_data) {
                            if state.cid == Some(msg.conversation_id) {
                                if !state.messages.iter().any(|m| m.id == msg.id) {
                                    state.messages.push(msg.clone());
                                    debug!("Added new message to UI");
                                } else {
                                    debug!("Ignoring duplicate message {}", msg.id);
                                }
                            }

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
                    // Nella sezione UserNotification, aggiungi questo case:

                    "conversation_created_complete" => {
                        info!("Processing conversation_created_complete with sequence {}", sequence);

                        // Estrai la conversazione completa dai dati
                        if let Some(conv_obj) = event_data.get("conversation") {
                            // Parse della conversazione
                            let id = conv_obj.get("id")
                                .and_then(|v| v.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok())
                                .unwrap_or_else(|| {
                                    warn!("Invalid conversation ID in conversation_created_complete");
                                    Uuid::nil()
                                });

                            if id == Uuid::nil() {
                                return; // Skip invalid conversation
                            }

                            let kind = conv_obj.get("kind")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();

                            let owner_id = conv_obj.get("owner_id")
                                .and_then(|v| v.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok())
                                .unwrap_or_else(|| Uuid::nil());

                            let created_at = conv_obj.get("created_at")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0);

                            // Usa display_title se presente, altrimenti title
                            let title = conv_obj.get("display_title")
                                .and_then(|t| t.as_str())
                                .or_else(|| conv_obj.get("title").and_then(|t| t.as_str()))
                                .unwrap_or("")
                                .to_string();

                            let conversation = ConversationDto {
                                id,
                                kind,
                                title,
                                owner_id,
                                created_at,
                            };

                            // Parse dell'ultimo messaggio se presente
                            let mut messages = Vec::new();
                            if let Some(last_msg) = conv_obj.get("last_message") {
                                let msg_id = last_msg.get("id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| Uuid::parse_str(s).ok())
                                    .unwrap_or_else(|| Uuid::new_v4());

                                let msg_author_id = last_msg.get("author_id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| Uuid::parse_str(s).ok())
                                    .unwrap_or_else(|| Uuid::nil());

                                let msg_author_username = last_msg.get("author_username")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();

                                let msg_content = last_msg.get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();

                                let msg_created_at = last_msg.get("created_at")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0);

                                let msg_sequence = last_msg.get("sequence_num")
                                    .and_then(|v| v.as_u64());

                                messages.push(MessageDto {
                                    id: msg_id,
                                    author_id: msg_author_id,
                                    author_username: msg_author_username,
                                    conversation_id: id,
                                    content: msg_content,
                                    created_at: msg_created_at,
                                    sequence_num: msg_sequence,
                                });
                            }

                            // Rimuovi eventuali DM stub
                            if state.dm_stubs.contains_key(&id) {
                                state.remove_dm_stub(id);
                                info!("Removed DM stub {} after receiving complete conversation", id);
                            }

                            // Aggiungi la conversazione
                            if let Some(ref mut conversations) = state.conversations {
                                conversations.retain(|c| c.id != id); // Rimuovi duplicati
                                conversations.push(conversation.clone());
                                conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                            } else {
                                state.conversations = Some(vec![conversation.clone()]);
                            }

                            // Aggiungi messaggi alla cache
                            if !messages.is_empty() {
                                state.conversation_messages.insert(id, messages.clone());

                                // Se è la conversazione corrente, aggiorna UI
                                if state.cid == Some(id) {
                                    state.messages = messages;
                                }
                            }

                            // Se siamo in chat view con questa conversazione, assicurati che sia selezionata
                            if state.page == Page::Chat && (state.cid == Some(id) || state.cid.is_none()) {
                                state.cid = Some(id);
                                state.conv_title = conversation.title.clone();

                                if let Some(cached) = state.conversation_messages.get(&id) {
                                    state.messages = cached.clone();
                                }
                            }

                            helpers::add_system_message(
                                state,
                                format!("Nuova conversazione '{}' creata e sincronizzata", conversation.title)
                            );

                            info!("Successfully processed conversation_created_complete for {}", id);
                        } else {
                            warn!("conversation_created_complete missing conversation object");
                        }
                    }
                    _ => {
                        debug!("Unhandled notification type: {}", event_type);
                    }
                }
            }
        }
    }

    /// Richiede i messaggi di una conversazione via WebSocket
    fn request_conversation_messages(state: &mut AppState, conversation_id: Uuid) {
        debug!(
            "Requesting messages for conversation {} via WebSocket",
            conversation_id
        );

        // Invia richiesta "open_conversation" al server
        let request = serde_json::json!({
            "type": "open_conversation",
            "conversation_id": conversation_id.to_string()
        });

        if let Some(ref ws_ctrl) = state.ws_ctrl {
            if let Ok(json_str) = serde_json::to_string(&request) {
                let _ = ws_ctrl.outgoing_tx.send(json_str);

                helpers::add_system_message(state, "Caricamento messaggi...".into());
            }
        } else {
            warn!("Cannot request messages: WebSocket not connected");
        }
    }
}
