mod conversation_handler;
mod data_handler;
mod fetch_handler;
mod helpers;
mod message_handler;
mod websocket_handler;

mod auth_handler;

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
                    "User logged in successfully - user_id: {}, initial sequence: {}",
                    user_id, last_sequence
                );

                state.token = Some(token.clone());
                state.user_id = Some(user_id);
                state.last_sequence_received = last_sequence;
                state.last_sequence_confirmed = last_sequence;
                state.login_state = LoginState::LoggedIn;
                state.page = Page::Conversations;

                // Reset sequence stats on new login
                state.sequence_stats = Default::default();

                // RIMOSSO: DataLoader::preload_all_data(state, token.clone());


                // Richiedi connessione WebSocket
                state.request_ws_reconnect = true;
            }

            UiEvent::LoggedOut => {
                info!("User logged out");

                // Shutdown WebSocket se attivo
                if let Some(ctrl) = state.ws_ctrl.take() {
                    let _ = ctrl.shutdown.send(());
                }

                // Reset dello stato
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

                // Reset sequence system
                state.last_sequence_received = 0;
                state.last_sequence_confirmed = 0;
                state.sequence_stats = Default::default();
            }

            // ===== WEBSOCKET EVENTS =====
            UiEvent::WsConnected => {
                state.ws_status = WsStatus::Connected;
                state.reset_sequence_system();
                info!("WebSocket connected, sequence system reset");
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
                    "Incoming WebSocket message for conversation {}: {}",
                    msg.conversation_id,
                    msg.content.chars().take(50).collect::<String>()
                );

                // Se il messaggio è per la conversazione corrente, aggiungilo
                if state.cid == Some(msg.conversation_id) {
                    if !state.messages.iter().any(|m| m.id == msg.id) {
                        state.messages.push(msg.clone());
                    }
                }

                // Aggiorna cache messaggi
                state
                    .conversation_messages
                    .entry(msg.conversation_id)
                    .or_insert_with(Vec::new)
                    .push(msg.clone());

                // Se arriva un messaggio per una conversazione che non conosciamo,
                // richiedi un refresh delle conversazioni
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

            // ===== CONVERSATION EVENTS =====
            UiEvent::Opened(cid) => {
                info!("Opening conversation: {}", cid);
                state.cid = Some(cid);
                state.page = Page::Chat;

                // Carica messaggi dalla cache o fetch se necessario
                if let Some(cached) = state.conversation_messages.get(&cid) {
                    state.messages = cached.clone();
                    debug!("Loaded {} cached messages", state.messages.len());
                } else {
                    state.messages.clear();

                    // Solo se non è uno stub, carica i messaggi
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

                // 1. Aggiungi lo stub al tracking
                state.add_dm_stub(stub_id, target_username.clone());

                // 2. Imposta questo stub come conversazione corrente
                state.cid = Some(stub_id);
                state.page = Page::Chat;

                // 3. Inizializza la lista messaggi vuota nella cache
                state.messages.clear();
                state.conversation_messages.insert(stub_id, Vec::new());

                // 4. Imposta il titolo per la UI
                state.conv_title = target_username.clone();

                // 5. Aggiungi un messaggio di sistema informativo
                let system_msg = MessageDto {
                    id: Uuid::new_v4(),
                    author_id: Uuid::nil(),
                    conversation_id: stub_id,
                    author_username: "system".to_string(),
                    content: format!(
                        "Chat con {} pronta. Invia il primo messaggio per iniziare!",
                        target_username
                    ),
                    created_at: chrono::Utc::now().timestamp(),
                };

                state.messages.push(system_msg.clone());
                state
                    .conversation_messages
                    .get_mut(&stub_id)
                    .map(|msgs| msgs.push(system_msg));

                debug!(
                    "DM stub initialized for {} (not visible in sidebar)",
                    target_username
                );
            }

            // ===== DATA LOADING EVENTS =====
            UiEvent::ConversationsLoaded(conversations) => {
                info!("Loaded {} conversations", conversations.len());

                // Rimuovi stub che ora hanno conversazioni reali sul server
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
                    debug!("Removed obsolete DM stub: {}", stub_id);
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

                // Se abbiamo una conversazione aperta, aggiorna i messaggi
                if let Some(cid) = state.cid {
                    if let Some(messages) = state.conversation_messages.get(&cid) {
                        state.messages = messages.clone();
                    }
                }
            }

            UiEvent::RefreshedMsgs(messages) => {
                debug!("Refreshed {} messages", messages.len());
                state.messages = messages.clone();

                // Aggiorna anche la cache
                if let Some(cid) = state.cid {
                    state.conversation_messages.insert(cid, messages);
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

                // Rimuovi il messaggio ottimistico
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

                // Aggiorna conversazione
                if let Some(ref mut conversations) = state.conversations {
                    if let Some(pos) = conversations.iter().position(|c| c.id == conv.id) {
                        conversations[pos] = conv.clone();
                    } else {
                        conversations.push(conv.clone());
                    }
                } else {
                    state.conversations = Some(vec![conv.clone()]);
                }

                // Aggiorna messaggi
                state
                    .conversation_messages
                    .insert(conv.id, messages.clone());

                if state.cid == Some(conv.id) {
                    state.messages = messages;
                }

                // Rimuovi stub se esisteva
                state.remove_dm_stub(conv.id);
            }

            UiEvent::ConversationListUpdated => {
                debug!("Conversation list update requested");
                state.request_conversations_refresh = true;
            }

            // ===== SEQUENCE SYSTEM EVENTS =====
            UiEvent::SequenceReceived(seq) => {
                debug!("Sequence received: {}", seq);
                state.update_sequence(seq);
            }

            UiEvent::SendPing => {
                debug!("Manual ping requested");
                state.send_ping();
            }

            UiEvent::PongReceived {
                server_sequence,
                gap_detected,
                events_recovered,
            } => {
                debug!(
                    "Pong received - server_seq: {}, gap: {}, recovered: {:?}",
                    server_sequence, gap_detected, events_recovered
                );

                state.sequence_stats.pong_count += 1;
                state.missed_pings = 0;

                if gap_detected {
                    state.sequence_stats.gaps_detected += 1;
                    state.sequence_stats.last_gap_time = Some(std::time::Instant::now());

                    if let Some(recovered) = events_recovered {
                        state.sequence_stats.events_recovered += recovered as u32;

                        // Calcola media dimensione gap
                        let gap_size = server_sequence - state.last_sequence_confirmed;
                        let total_gap_size = state.sequence_stats.average_gap_size
                            * state.sequence_stats.gaps_detected as f64;
                        state.sequence_stats.average_gap_size = (total_gap_size + gap_size as f64)
                            / state.sequence_stats.gaps_detected as f64;

                        info!("Gap detected and recovering {} events", recovered);
                    }
                }

                state.is_recovering_sequence = gap_detected;
            }

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

                // CONTROLLO DUPLICATI: Se l'evento ha una sequenza già confermata, ignoralo
                if sequence > 0 && sequence <= state.last_sequence_confirmed {
                    debug!(
                        "Ignoring duplicate event with sequence {} (already confirmed up to {})",
                        sequence, state.last_sequence_confirmed
                    );
                    return;
                }

                // Aggiorna la sequenza
                state.update_sequence(sequence);

                // Se è un evento di recovery, incrementa il contatore
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
                            // Controlla duplicati basandosi sull'ID del messaggio
                            if state.cid == Some(msg.conversation_id) {
                                if !state.messages.iter().any(|m| m.id == msg.id) {
                                    state.messages.push(msg.clone());
                                    debug!("Added new message to UI");
                                } else {
                                    debug!("Ignoring duplicate message {}", msg.id);
                                }
                            }

                            // Aggiorna cache con controllo duplicati
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

                            // Se è una DM che abbiamo creato noi (stub), ora è stata confermata
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
