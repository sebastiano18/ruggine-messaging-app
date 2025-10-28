mod auth_handler;
mod conversation_handler;
mod data_handler;
mod helpers;
mod message_handler;
mod websocket_handler;

use auth_handler::AuthHandler;
use conversation_handler::ConversationHandler;
use data_handler::DataHandler;
use message_handler::MessageHandler;
use websocket_handler::WebSocketHandler;

use crate::models::*;
use crate::state::AppState;
use reqwest::StatusCode;
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

            UiEvent::MessageConfirmation {
                client_msg_id,
                server_msg_id,
                sequence,
                status,
            } => {
                Self::handle_message_confirmation(
                    state,
                    client_msg_id,
                    server_msg_id,
                    sequence,
                    status,
                );
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

            UiEvent::DeleteAccountStart => {
                // Abilita modalità "doppia conferma"
                state.confirm_delete_account = true;
            }

            UiEvent::DeleteAccountCancel => {
                // Disabilita modalità "doppia conferma"
                state.confirm_delete_account = false;
                let _ = state
                    .ui_tx
                    .send(UiEvent::Info("Eliminazione annullata".into()));
            }

            UiEvent::DeleteAccountConfirm => {
                // Disabilita sempre il flag di conferma (per evitare UI bloccate)
                state.confirm_delete_account = false;

                // Se non c'è token, tratta come logout
                let Some(token) = state.token.clone() else {
                    let _ = state.ui_tx.send(UiEvent::LoggedOut);
                    return;
                };

                let base = state.base.clone();
                let tx = state.ui_tx.clone();

                // Esegui la chiamata HTTP in background
                state.rt.spawn(async move {
                    match crate::api::auth::delete_account(&base, &token).await {
                        Ok(()) => {
                            // Successo -> logout
                            let _ = tx.send(UiEvent::LoggedOut);
                        }
                        Err(e) => {
                            // 401 => token non valido -> logout
                            if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
                                if req_err.status() == Some(StatusCode::UNAUTHORIZED) {
                                    let _ = tx.send(UiEvent::LoggedOut);
                                    return;
                                }
                            }
                            // Altri errori => mostra errore
                            let _ = tx
                                .send(UiEvent::Error(format!("Eliminazione account fallita: {e}")));
                        }
                    }
                });
            }

            // ===== WEBSOCKET EVENTS =====
            // All WebSocket events delegated to WebSocketHandler
            UiEvent::WsConnected
            | UiEvent::WsDisconnected
            | UiEvent::WsControlReady(_)
            | UiEvent::WsError(_)
            | UiEvent::WsIncoming(_) => {
                WebSocketHandler::handle(state, event);
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

            // ===== PONG EVENT =====
            UiEvent::PongReceived {
                current_user_sequence,
                gaps_detected,
                user_events_gap,
            } => {
                debug!(
        "Pong - server_user_seq: {}, gaps_detected: {}",
        current_user_sequence, gaps_detected
    );

                state.sequence_stats.pong_count += 1;
                state.missed_pings = 0;

                // Check for user events gap
                if let Some(gap) = user_events_gap {
                    if gap.detected {
                        warn!(
                "User events gap detected: {} events missing (client: {}, server: {})",
                gap.gap_size, gap.client_seq, gap.server_seq
            );
                        state.sequence_stats.gaps_detected += 1;
                        state.request_user_events_resume(gap.client_seq);
                    }
                }

                if gaps_detected {
                    info!("Pong reported gap, resume request sent");
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
                info!(
                    "Processing {} resumed messages for {}",
                    messages.len(),
                    conversation_id
                );

                // LOG dettagliato dei messaggi ottimistici attuali nell'UI
                info!("Current UI messages for conversation {}:", conversation_id);
                if state.cid == Some(conversation_id) {
                    for (idx, msg) in state.messages.iter().enumerate() {
                        info!(
                            "  UI[{}]: id={}, client_msg_id={:?}, content_preview={}...",
                            idx,
                            msg.id,
                            msg.client_msg_id,
                            msg.content.chars().take(20).collect::<String>()
                        );
                    }
                }

                // LOG dettagliato dei pending confirmations
                info!("Current pending confirmations:");
                for (client_id, pending_msg) in &state.pending_confirmations {
                    info!(
                        "  Pending: client_id={}, msg_id={}, conv_id={}, content_preview={}...",
                        client_id,
                        pending_msg.id,
                        pending_msg.conversation_id,
                        pending_msg.content.chars().take(20).collect::<String>()
                    );
                }

                // LOG dettagliato dei messaggi in arrivo dal resume
                info!("Incoming resume messages:");
                for (idx, msg) in messages.iter().enumerate() {
                    info!(
                        "  Resume[{}]: id={}, client_msg_id={:?}, content_preview={}...",
                        idx,
                        msg.id,
                        msg.client_msg_id,
                        msg.content.chars().take(20).collect::<String>()
                    );
                }

                state.is_recovering_messages.insert(conversation_id, false);
                state.pending_resume_requests = state.pending_resume_requests.saturating_sub(1);

                // Aggiorna le sequenze
                if let Some(max_seq) = messages.iter().filter_map(|m| m.sequence_num).max() {
                    state
                        .conversation_sequences
                        .insert(conversation_id, max_seq);
                    state
                        .conversation_sequences_confirmed
                        .insert(conversation_id, max_seq);
                    info!(
                        "Updated conversation {} sequences to {}",
                        conversation_id, max_seq
                    );
                }

                // Raccogli ID esistenti
                let mut existing_ids = std::collections::HashSet::new();

                if let Some(cache) = state.conversation_messages.get(&conversation_id) {
                    for msg in cache {
                        existing_ids.insert(msg.id);
                    }
                }

                if state.cid == Some(conversation_id) {
                    for msg in &state.messages {
                        existing_ids.insert(msg.id);
                    }
                }

                let mut pending_to_remove = Vec::new();
                let mut optimistic_to_remove = Vec::new();
                let mut truly_new_messages = Vec::new();

                for new_msg in messages {
                    info!(
                        "Processing resume message id={}, client_msg_id={:?}",
                        new_msg.id, new_msg.client_msg_id
                    );

                    // Check se già esiste per server ID
                    if existing_ids.contains(&new_msg.id) {
                        info!(
                            "  -> Message {} already exists by server ID, skipping",
                            new_msg.id
                        );
                        continue;
                    }

                    let mut should_add = true;

                    // Se ha un client_msg_id, cerca di matchare con pending
                    if let Some(ref server_client_id) = new_msg.client_msg_id {
                        info!(
                            "  -> Checking for pending with client_id: {}",
                            server_client_id
                        );

                        // Cerca nei pending confirmations
                        if let Some(pending_msg) = state.pending_confirmations.get(server_client_id)
                        {
                            info!("    FOUND in pending! Pending msg_id={}, will replace with server msg",
                      pending_msg.id);
                            pending_to_remove.push(server_client_id.clone());
                            optimistic_to_remove.push((pending_msg.id, server_client_id.clone()));
                            // Non serve aggiungere, sostituiremo l'ottimistico
                        } else {
                            info!("    NOT found in pending confirmations");

                            // Cerca direttamente nei messaggi UI per client_msg_id
                            if state.cid == Some(conversation_id) {
                                for ui_msg in &state.messages {
                                    if ui_msg.client_msg_id.as_ref() == Some(server_client_id) {
                                        info!(
                                            "    FOUND in UI messages! UI msg_id={}, will replace",
                                            ui_msg.id
                                        );
                                        optimistic_to_remove
                                            .push((ui_msg.id, server_client_id.clone()));
                                        should_add = true; // Dobbiamo aggiungere il messaggio del server
                                        break;
                                    }
                                }
                            }
                        }
                    } else {
                        info!("  -> No client_msg_id, will add as new message");
                    }

                    if should_add {
                        truly_new_messages.push(new_msg);
                    }
                }

                // Rimuovi i pending confirmations
                for client_id in &pending_to_remove {
                    if let Some(removed) = state.pending_confirmations.remove(client_id) {
                        info!("Removed pending confirmation for client_id={}", client_id);
                    }
                }

                // Rimuovi i messaggi ottimistici dall'UI e dalla cache
                for (optimistic_id, client_id) in &optimistic_to_remove {
                    info!(
                        "Removing optimistic message id={} with client_id={}",
                        optimistic_id, client_id
                    );

                    // Rimuovi dall'UI
                    if state.cid == Some(conversation_id) {
                        let before = state.messages.len();
                        state.messages.retain(|m| {
                            // Rimuovi per ID ottimistico O per client_msg_id
                            !(m.id == *optimistic_id || m.client_msg_id.as_ref() == Some(client_id))
                        });
                        let after = state.messages.len();
                        info!("  Removed {} messages from UI", before - after);
                    }

                    // Rimuovi dalla cache
                    if let Some(cache) = state.conversation_messages.get_mut(&conversation_id) {
                        let before = cache.len();
                        cache.retain(|m| {
                            !(m.id == *optimistic_id || m.client_msg_id.as_ref() == Some(client_id))
                        });
                        let after = cache.len();
                        info!("  Removed {} messages from cache", before - after);
                    }
                }

                if truly_new_messages.is_empty() {
                    info!("No new messages to add after deduplication");
                    return;
                }

                info!("Adding {} truly new messages", truly_new_messages.len());

                // Aggiungi i nuovi messaggi
                if let Some(cache) = state.conversation_messages.get_mut(&conversation_id) {
                    cache.extend(truly_new_messages.clone());
                    cache.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                        (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                        _ => a.created_at.cmp(&b.created_at),
                    });
                } else {
                    state
                        .conversation_messages
                        .insert(conversation_id, truly_new_messages.clone());
                }

                if state.cid == Some(conversation_id) {
                    state.messages.extend(truly_new_messages);
                    state
                        .messages
                        .sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                            (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                            _ => a.created_at.cmp(&b.created_at),
                        });

                    info!("Final UI message count: {}", state.messages.len());
                }
            }

            // ===== CONVERSATION EVENTS =====

            // Circa riga 666 nel tuo mod.rs
            UiEvent::ConversationConfirmed {
                conversation,
                messages,
                client_temp_id,
            } => {
                info!(
                    "Processing conversation confirmation for {} (temp_id: {:?})",
                    conversation.id, client_temp_id
                );

                // CRITICO: Controlla se lo stub era la conversazione attiva
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

                // Rimuovi lo stub
                if let Some(stub_id) = stub_to_remove {
                    if let Some(target) = state.dm_stubs.remove(&stub_id) {
                        info!(
                            "Removed DM stub {} (target: {}) after confirmation",
                            stub_id, target
                        );
                    }

                    // NUOVO: Rimuovi anche le sequence dello stub
                    state.conversation_sequences.remove(&stub_id);
                    state.conversation_sequences_confirmed.remove(&stub_id);
                }

                // Aggiungi/aggiorna la conversazione reale
                if let Some(ref mut conversations) = state.conversations {
                    conversations.retain(|c| c.id != conversation.id);
                    conversations.push(conversation.clone());
                    conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                } else {
                    state.conversations = Some(vec![conversation.clone()]);
                }

                // Aggiungi messaggi se presenti
                if !messages.is_empty() {
                    state
                        .conversation_messages
                        .insert(conversation.id, messages.clone());

                    // NUOVO: Imposta le sequence basandosi sui messaggi ricevuti
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

                // CRITICO: Se lo stub era attivo, AGGIORNA alla conversazione reale
                if was_active_stub {
                    state.cid = Some(conversation.id); // Aggiorna ID attivo
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

                // Notifica utente
                helpers::add_system_message(
                    state,
                    format!("Conversazione '{}' confermata", conversation.title),
                );
            }

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
                all_messages.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                    (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                    _ => a.created_at.cmp(&b.created_at),
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
                state
                    .conversation_sequences_confirmed
                    .entry(cid)
                    .or_insert(0);

                // IMPORTANTE: Carica dalla cache se disponibile
                if let Some(cached) = state.conversation_messages.get(&cid) {
                    state.messages = cached.clone();

                    // Aggiorna sequence dall'ultimo messaggio cached
                    if let Some(max_seq) = cached.iter().filter_map(|m| m.sequence_num).max() {
                        state.conversation_sequences.insert(cid, max_seq);
                        state.conversation_sequences_confirmed.insert(cid, max_seq);
                    }

                    info!(
                        "Loaded {} messages from cache for conversation {}",
                        cached.len(),
                        cid
                    );
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
                state.send_ping();
            }

            // ===== USER NOTIFICATIONS =====
            UiEvent::UserNotification {
                sequence,
                event_type,
                event_data,
                conversation_id,
                recovery,
            } => {
                let expected = state.user_sequence_confirmed + 1;

                // NUOVO: Gestione buffer di riordino
                if sequence > expected {
                    warn!(
                        "User event seq {} out of order (expected {}), buffering",
                        sequence, expected
                    );
                    state.buffer_user_event_for_reorder(sequence, event_data);
                    return;
                } else if sequence < expected && sequence > 0 {
                    debug!(
                        "User event seq {} already processed (expected {}), skipping",
                        sequence, expected
                    );
                    return;
                }

                // sequence == expected o sequence == 0, processa normalmente
                if sequence > 0 {
                    // COMMENTATO PER TEST RESUME - client rimane sempre a seq 0
                     state.user_sequence_confirmed = sequence;
                }

                process_user_notification(
                    state,
                    sequence,
                    event_type,
                    event_data,
                    conversation_id,
                    recovery,
                );

                debug!("User event seq {} processed normally", sequence);

                // NUOVO: Controlla buffer dopo processing
                let buffered_events = state.try_deliver_buffered_user_events();
                if !buffered_events.is_empty() {
                    info!(
                        "Delivering {} buffered user events after processing seq {}",
                        buffered_events.len(),
                        sequence
                    );

                    for buffered_event in buffered_events {
                        if let Some(event_seq) =
                            buffered_event.get("sequence").and_then(|s| s.as_u64())
                        {
                            // COMMENTATO PER TEST RESUME
                            // state.user_sequence_confirmed = event_seq;

                            let evt_type = buffered_event
                                .get("event_type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("unknown")
                                .to_string();

                            let conv_id = buffered_event
                                .get("conversation_id")
                                .and_then(|id| id.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok());

                            process_user_notification(
                                state,
                                event_seq,
                                evt_type,
                                buffered_event.clone(),
                                conv_id,
                                false,
                            );
                        }
                    }
                }
            }
        }
    }

    fn handle_message_confirmation(
        state: &mut AppState,
        client_msg_id: String,
        server_msg_id: Uuid,
        sequence: Option<u64>,
        status: String,
    ) {
        info!("Processing confirmation for msg {}", client_msg_id);

        // Prima controlla se il messaggio è già stato ricevuto via resume
        let already_exists = state.messages.iter().any(|m| m.id == server_msg_id);

        if already_exists {
            // Il messaggio è già arrivato via resume, rimuovi solo l'ottimistico se ancora presente
            state
                .messages
                .retain(|m| m.client_msg_id.as_ref() != Some(&client_msg_id));
            state.pending_confirmations.remove(&client_msg_id);

            // Aggiorna anche nella cache
            if let Some(cid) = state.cid {
                if let Some(cache) = state.conversation_messages.get_mut(&cid) {
                    cache.retain(|m| m.client_msg_id.as_ref() != Some(&client_msg_id));
                }
            }

            info!(
                "Message {} already received via resume, cleaned optimistic",
                server_msg_id
            );
            return;
        }

        // Trova e aggiorna il messaggio pending
        if let Some(pending_msg) = state.pending_confirmations.remove(&client_msg_id) {
            let old_id = pending_msg.id;
            let conversation_id = pending_msg.conversation_id;

            // Aggiorna nei messaggi UI
            let mut updated = false;
            for msg in &mut state.messages {
                if msg.client_msg_id.as_ref() == Some(&client_msg_id) {
                    msg.id = server_msg_id;
                    msg.sequence_num = sequence;
                    msg.is_confirmed = Some(true);
                    updated = true;
                    debug!("Updated UI msg {} -> {}", old_id, server_msg_id);
                    break;
                }
            }

            // Se non trovato nell'UI, potrebbe essere stato già processato via resume
            if !updated {
                debug!(
                    "Message {} not found in UI, likely processed via resume",
                    client_msg_id
                );
                return;
            }

            // Aggiorna nella cache
            if let Some(cache) = state.conversation_messages.get_mut(&conversation_id) {
                for msg in cache.iter_mut() {
                    if msg.client_msg_id.as_ref() == Some(&client_msg_id) {
                        msg.id = server_msg_id;
                        msg.sequence_num = sequence;
                        msg.is_confirmed = Some(true);
                        break;
                    }
                }

                // Riordina se necessario
                if sequence.is_some() {
                    cache.sort_by(|a, b| match (a.sequence_num, b.sequence_num) {
                        (Some(seq_a), Some(seq_b)) => seq_a.cmp(&seq_b),
                        _ => a.created_at.cmp(&b.created_at),
                    });
                }
            }

            // Aggiorna tracking sequence
            if let Some(seq) = sequence {
                state.update_conversation_sequence(conversation_id, seq);
            }

            info!(
                "Message confirmed: {} -> {} (seq: {:?})",
                client_msg_id, server_msg_id, sequence
            );
        } else {
            warn!(
                "Received confirmation for unknown message: {}",
                client_msg_id
            );
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
        info!(
            "Processing recovery event: seq {} type {}",
            sequence, event_type
        );
    }

    match event_type.as_str() {
        "new_message" => {
            if let Ok(msg) = serde_json::from_value::<MessageDto>(event_data) {
                info!(
                    "Processing new_message from UserEvent seq {} (msg_id: {}, conv_seq: {:?})",
                    sequence, msg.id, msg.sequence_num
                );

                // ✅ 1. DEDUPLICAZIONE: Controlla se già processato via WebSocket
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

                    // ✅ 2. AGGIORNA conversation_sequence anche se duplicato
                    // Questo garantisce che conversation_sequences_confirmed sia sempre aggiornato
                    if let Some(conv_seq) = msg.sequence_num {
                        let current = state
                            .conversation_sequences_confirmed
                            .get(&msg.conversation_id)
                            .copied()
                            .unwrap_or(0);

                        if conv_seq > current {
                            state
                                .conversation_sequences_confirmed
                                .insert(msg.conversation_id, conv_seq);
                            debug!(
                                "Updated conversation {} sequence from {} to {} via UserEvent",
                                msg.conversation_id, current, conv_seq
                            );
                        }
                    }

                    return;
                }

                // ✅ 3. Messaggio NUOVO - Aggiorna conversation_sequence
                // user_sequence garantisce l'ordine, quindi possiamo fidarci di conv_seq
                if let Some(conv_seq) = msg.sequence_num {
                    state
                        .conversation_sequences_confirmed
                        .insert(msg.conversation_id, conv_seq);
                    debug!(
                        "Set conversation {} sequence to {} via UserEvent (trusted from user_seq)",
                        msg.conversation_id, conv_seq
                    );
                }

                // ✅ 4. Aggiungi alla CACHE con inserimento ordinato per sequence
                let cache = state
                    .conversation_messages
                    .entry(msg.conversation_id)
                    .or_insert_with(Vec::new);

                let cache_insert_pos = if let Some(msg_seq) = msg.sequence_num {
                    // Ordina per sequence se disponibile
                    cache
                        .binary_search_by(|existing| {
                            match (existing.sequence_num, msg.sequence_num) {
                                (Some(e_seq), Some(m_seq)) => e_seq.cmp(&m_seq),
                                _ => existing
                                    .created_at
                                    .cmp(&msg.created_at)
                                    .then_with(|| existing.id.cmp(&msg.id)),
                            }
                        })
                        .unwrap_or_else(|pos| pos)
                } else {
                    // Fallback su timestamp se non c'è sequence
                    cache
                        .binary_search_by(|existing| {
                            existing
                                .created_at
                                .cmp(&msg.created_at)
                                .then_with(|| existing.id.cmp(&msg.id))
                        })
                        .unwrap_or_else(|pos| pos)
                };

                cache.insert(cache_insert_pos, msg.clone());
                debug!(
                    "Added UserEvent message {} to cache at position {} (seq: {:?})",
                    msg.id, cache_insert_pos, msg.sequence_num
                );

                // ✅ 5. Aggiungi alla UI se è la conversazione corrente
                if state.cid == Some(msg.conversation_id) {
                    // Verifica che non sia già in UI
                    if !state.messages.iter().any(|m| m.id == msg.id) {
                        let ui_insert_pos = if let Some(msg_seq) = msg.sequence_num {
                            // Ordina per sequence se disponibile
                            state
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
                                .unwrap_or_else(|pos| pos)
                        } else {
                            // Fallback su timestamp
                            state
                                .messages
                                .binary_search_by(|existing| {
                                    existing
                                        .created_at
                                        .cmp(&msg.created_at)
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
                        debug!(
                            "UserEvent message {} already in UI, skipping",
                            msg.id
                        );
                    }
                } else {
                    debug!(
                        "UserEvent message {} cached for conversation {} (not current: {:?})",
                        msg.id, msg.conversation_id, state.cid
                    );
                }
            }
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

        "conversation_created_complete" => {
            info!(
                "Processing conversation_created_complete with sequence {}",
                sequence
            );

            if let Some(conv_obj) = event_data.get("conversation") {
                let id = conv_obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(|| {
                        warn!("Invalid conversation ID in conversation_created_complete");
                        Uuid::nil()
                    });

                if id == Uuid::nil() {
                    return;
                }

                let kind = conv_obj
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let owner_id = conv_obj
                    .get("owner_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(|| Uuid::nil());

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

                let conversation = ConversationDto {
                    id,
                    kind,
                    title,
                    owner_id,
                    created_at,
                };

                let mut messages = Vec::new();
                if let Some(last_msg) = conv_obj.get("last_message") {
                    let msg_id = last_msg
                        .get("id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| Uuid::parse_str(s).ok())
                        .unwrap_or_else(|| Uuid::new_v4());

                    let msg_author_id = last_msg
                        .get("author_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| Uuid::parse_str(s).ok())
                        .unwrap_or_else(|| Uuid::nil());

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
                        author_username: msg_author_username,
                        conversation_id: id,
                        content: msg_content,
                        created_at: msg_created_at,
                        sequence_num: msg_sequence,
                        client_msg_id: None,
                        is_confirmed: Some(true),
                    });
                }

                // Rimuovi DM stub se esiste
                if state.dm_stubs.contains_key(&id) {
                    state.remove_dm_stub(id);
                    info!(
                        "Removed DM stub {} after receiving complete conversation",
                        id
                    );
                }

                // Aggiorna lista conversazioni
                if let Some(ref mut conversations) = state.conversations {
                    conversations.retain(|c| c.id != id);
                    conversations.push(conversation.clone());
                    conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                } else {
                    state.conversations = Some(vec![conversation.clone()]);
                }

                // Aggiungi messaggi alla cache
                if !messages.is_empty() {
                    state.conversation_messages.insert(id, messages.clone());

                    if state.cid == Some(id) {
                        state.messages = messages;
                    }
                }

                // Se siamo in chat e la conversazione è quella corrente (o nessuna selezionata)
                if state.page == Page::Chat && (state.cid == Some(id) || state.cid.is_none()) {
                    state.cid = Some(id);
                    state.conv_title = conversation.title.clone();

                    if let Some(cached) = state.conversation_messages.get(&id) {
                        state.messages = cached.clone();
                    }
                }

                helpers::add_system_message(
                    state,
                    format!(
                        "Nuova conversazione '{}' creata e sincronizzata",
                        conversation.title
                    ),
                );

                info!(
                    "Successfully processed conversation_created_complete for {}",
                    id
                );
            } else {
                warn!("conversation_created_complete missing conversation object");
            }
        }

        _ => {
            debug!("Unhandled notification type: {}", event_type);
        }
    }
}