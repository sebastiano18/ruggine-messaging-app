use crate::models::*;
use uuid::Uuid;
use std::collections::HashMap;
use tracing::{debug, warn, error, info};

pub struct EventHandler;

impl EventHandler {
    pub fn handle_event(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::Info(s) => {
                Self::add_system_message(state, s);
            }
            UiEvent::Error(s) => {
                Self::add_system_message(state, format!("⚠ {}", s));
                state.login_state = LoginState::Idle;
                state.is_loading = false;
            }
            UiEvent::LoginStarted => {
                state.login_state = LoginState::LoggingIn;
                Self::add_system_message(state, "🔐 Effettuando login...".into());
            }
            UiEvent::RegisterStarted => {
                state.login_state = LoginState::Registering;
                Self::add_system_message(state, "📝 Registrando utente...".into());
            }
            UiEvent::Logged(token, user_id) => {
                Self::handle_login_success(state, token, user_id);
            }
            UiEvent::Opened(cid) => {
                Self::handle_conversation_opened(state, cid);
            }
            UiEvent::WsControlReady(ctrl) => {
                state.ws_ctrl = Some(ctrl);
                debug!("WebSocket control ready");
            }
            UiEvent::WsConnected => {
                state.ws_status = WsStatus::Connected;
                Self::add_system_message(state, "🟢 WebSocket connesso".into());
            }
            UiEvent::WsDisconnected => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;
                Self::add_system_message(state, "🔴 WebSocket disconnesso".into());
            }
            UiEvent::WsError(error) => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;
                Self::add_system_message(state, format!("⚠ WebSocket errore: {}", error));
            }
            UiEvent::WsIncoming(msg) => {
                Self::handle_incoming_message(state, msg);
            }
            UiEvent::RefreshedMsgs(list) => {
                Self::handle_messages_refreshed(state, list);
            }
            UiEvent::ConversationsLoaded(conversations) => {
                Self::handle_conversations_loaded(state, conversations);
            }
            UiEvent::AllMessagesLoaded(messages_map) => {
                Self::handle_all_messages_loaded(state, messages_map);
            }
            UiEvent::InitialLoadComplete => {
                state.is_initial_load_complete = true;
                state.is_loading = false;
                Self::add_system_message(state, "✅ Tutti i dati caricati!".into());
                info!("Initial data load completed");
            }
            UiEvent::LoadingProgress(progress) => {
                Self::add_system_message(state, format!("📊 {}", progress));
            }
            UiEvent::InviteCreated(token) => {
                state.last_created_invite = Some(token);
                Self::add_system_message(state, "🎉 Invito creato con successo".into());
            }
            UiEvent::MessageSendFailed(failed_message_id) => {
                Self::handle_message_send_failed(state, failed_message_id);
            }
            UiEvent::LoggedOut => {
                Self::handle_logout(state);
            }
            UiEvent::ConversationCreated(conversation_id) => {
                Self::handle_conversation_created(state, conversation_id);
            }
        }
    }

    fn handle_login_success(state: &mut super::core::AppState, token: String, user_id: Uuid) {
        state.token = Some(token.clone());
        state.user_id = Some(user_id);
        state.login_state = LoginState::LoggedIn;
        Self::add_system_message(state, "✅ Login effettuato con successo".into());
        state.page = Page::Conversations;

        info!("User {} logged in successfully", user_id);

        // Avvia precaricamento completo dopo il login
        state.preload_all_data(token);
    }

    fn handle_conversation_opened(state: &mut super::core::AppState, cid: Uuid) {
        debug!("Opening conversation: {}", cid);

        state.cid = Some(cid);
        state.page = Page::Chat;

        // Aggiorna titolo conversazione
        if let Some(ref conversations) = state.conversations {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                state.conv_title = conv.title.clone();
                debug!("Conversation title set to: {}", conv.title);
            }
        }

        // Carica messaggi dalla cache locale
        if let Some(cached_messages) = state.conversation_messages.get(&cid) {
            state.messages = cached_messages.clone();
            debug!("Loaded {} messages from cache for conversation {}", 
                   cached_messages.len(), cid);
        } else if state.is_initial_load_complete {
            // Caricamento iniziale completato ma non abbiamo questi messaggi - carica dalla rete
            debug!("Loading messages from network for conversation {}", cid);
            state.load_single_conversation_messages(cid);
            state.messages = vec![];

            // Messaggio di benvenuto per nuove conversazioni
            if let Some(ref conversations) = state.conversations {
                if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                    let welcome_msg = MessageDto::system_message(
                        format!("Benvenuto in {}! 🎉", conv.title)
                    );
                    state.messages.push(welcome_msg);
                }
            }
        } else {
            // Caricamento iniziale ancora in corso
            state.messages = vec![];
            Self::add_system_message(state, "Caricamento messaggi...".into());
        }
    }

    fn handle_incoming_message(state: &mut super::core::AppState, msg: MessageDto) {
        let message_conversation_id = msg.conversation_id;

        // Validazione: ignora messaggi senza conversation_id valido
        if message_conversation_id == Uuid::nil() {
            warn!("Received message with nil conversation_id, ignoring: {:?}", msg);
            return;
        }

        // Validazione: ignora messaggi vuoti
        if msg.content.trim().is_empty() {
            warn!("Received message with empty content, ignoring: {:?}", msg);
            return;
        }

        debug!("Processing incoming message: {} from {} in conversation {}", 
               msg.content.chars().take(50).collect::<String>(), 
               msg.author_username, 
               message_conversation_id);

        // Aggiungi SEMPRE alla cache della conversazione corretta
        let conversation_cache = state
            .conversation_messages
            .entry(message_conversation_id)
            .or_insert_with(Vec::new);

        // Evita duplicati basati su ID messaggio
        if conversation_cache.iter().any(|existing| existing.id == msg.id) {
            debug!("Duplicate message ignored: {}", msg.id);
            return;
        }

        // Inserisci in ordine cronologico nella cache
        let insert_pos = conversation_cache
            .binary_search_by_key(&msg.created_at, |m| m.created_at)
            .unwrap_or_else(|pos| pos);
        conversation_cache.insert(insert_pos, msg.clone());

        // Aggiungi ai messaggi UI SOLO se siamo nella conversazione corretta
        if Some(message_conversation_id) == state.cid {
            // Inserisci in ordine cronologico anche nell'UI
            let ui_insert_pos = state.messages
                .binary_search_by_key(&msg.created_at, |m| m.created_at)
                .unwrap_or_else(|pos| pos);
            state.messages.insert(ui_insert_pos, msg.clone());

            debug!("Message added to current conversation UI: {} characters from {}", 
                   msg.content.len(), msg.author_username);
        } else {
            debug!("Message cached for different conversation: {} -> {} (current: {:?})",
                   msg.author_username, message_conversation_id, state.cid);
        }
    }

    fn handle_messages_refreshed(state: &mut super::core::AppState, mut list: Vec<MessageDto>) {
        // Ordina per timestamp
        list.sort_by_key(|m| m.created_at);

        // Aggiorna UI
        state.messages = list.clone();

        // Aggiorna cache se abbiamo una conversazione corrente
        if let Some(cid) = state.cid {
            state.conversation_messages.insert(cid, list.clone());
            debug!("Refreshed {} messages for conversation {}", list.len(), cid);
        }
    }

    fn handle_conversations_loaded(state: &mut super::core::AppState, conversations: Vec<ConversationDto>) {
        let old_count = state.conversations.as_ref().map(|c| c.len()).unwrap_or(0);
        state.conversations = Some(conversations.clone());
        let new_count = conversations.len();

        info!("Loaded {} conversations (was {})", new_count, old_count);

        Self::add_system_message(state, format!("📋 {} conversazioni caricate", new_count));

        if new_count > old_count {
            Self::add_system_message(state, "✨ Lista conversazioni aggiornata".into());
        }

        // Verifica se la conversazione corrente esiste ancora
        if let Some(current_cid) = state.cid {
            if !conversations.iter().any(|c| c.id == current_cid) {
                warn!("Current conversation {} no longer exists, clearing selection", current_cid);
                state.cid = None;
                state.conv_title.clear();
                state.messages.clear();
                state.page = Page::Conversations;
            }
        }

        // Cleanup delle cache per conversazioni che non esistono più
        Self::cleanup_old_conversations(state);
    }

    fn handle_all_messages_loaded(
        state: &mut super::core::AppState,
        messages_map: HashMap<Uuid, Vec<MessageDto>>,
    ) {
        let total_messages: usize = messages_map.values().map(|v| v.len()).sum();
        info!("Loaded {} total messages across {} conversations", 
              total_messages, messages_map.len());

        state.conversation_messages = messages_map;

        // Se c'è una conversazione corrente, carica i suoi messaggi nell'UI
        if let Some(cid) = state.cid {
            if let Some(msgs) = state.conversation_messages.get(&cid).cloned() {
                state.messages = msgs;
                debug!("Loaded {} messages for current conversation {}", 
                       state.messages.len(), cid);
            }
        }
    }

    fn handle_message_send_failed(state: &mut super::core::AppState, failed_message_id: Uuid) {
        warn!("Message send failed: {}", failed_message_id);

        // Rimuovi il messaggio fallito dalla UI
        let ui_removed = state.messages.len();
        state.messages.retain(|msg| msg.id != failed_message_id);
        let ui_removed = ui_removed - state.messages.len();

        // Rimuovi dalla cache
        if let Some(cid) = state.cid {
            if let Some(messages) = state.conversation_messages.get_mut(&cid) {
                let cache_removed = messages.len();
                messages.retain(|msg| msg.id != failed_message_id);
                let cache_removed = cache_removed - messages.len();

                debug!("Removed failed message - UI: {}, Cache: {}", ui_removed, cache_removed);
            }
        }

        Self::add_system_message(state, "❌ Invio messaggio fallito".into());
    }

    fn handle_conversation_created(state: &mut super::core::AppState, conversation_id: Uuid) {
        info!("New conversation created: {}", conversation_id);

        // Inizializza cache vuota per la nuova conversazione
        state.conversation_messages.insert(conversation_id, vec![]);
        Self::add_system_message(state, "🎉 Conversazione creata!".into());

        // Ricarica la lista conversazioni per includere la nuova
        state.request_conversations_refresh = true;
    }

    fn handle_logout(state: &mut super::core::AppState) {
        info!("User logout");

        // Chiudi WebSocket se attivo
        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        // Reset completo dello stato
        state.token = None;
        state.user_id = None;
        state.page = Page::Auth;
        state.login_state = LoginState::Idle;
        state.password.clear();
        state.cid = None;
        state.conv_title.clear();
        state.input.clear();
        state.messages.clear();
        state.conversations = None;
        state.ws_status = WsStatus::Disconnected;
        state.request_ws_reconnect = false;
        state.request_conversations_refresh = false;

        // Pulizia campi specifici
        state.group_name.clear();
        state.dm_user_username_input.clear();
        state.invite_conversation_id.clear();
        state.last_created_invite = None;
        state.last_invite_token = None;

        // Pulizia cache e stato di caricamento
        state.conversation_messages.clear();
        state.is_initial_load_complete = false;
        state.is_loading = false;

        // Messaggio finale
        state.messages.push(MessageDto::system_message("👋 Logout effettuato".into()));
    }

    // === HELPER METHODS ===

    fn add_system_message(state: &mut super::core::AppState, content: String) {
        let msg = MessageDto::system_message(content);
        state.messages.push(msg);
    }

    pub fn sync_ui_with_cache(state: &mut super::core::AppState) {
        if let Some(current_cid) = state.cid {
            if let Some(cached_messages) = state.conversation_messages.get(&current_cid).cloned() {
                // Mantieni solo i messaggi di sistema più recenti dall'UI
                let recent_system_messages: Vec<_> = state.messages
                    .iter()
                    .rev()
                    .take(20)
                    .filter(|m| m.author_id == Uuid::nil())
                    .cloned()
                    .collect();

                // Ricostruisci UI con cache + messaggi di sistema
                let mut new_messages = cached_messages;
                new_messages.extend(recent_system_messages);
                new_messages.sort_by_key(|m| m.created_at);

                state.messages = new_messages;

                info!("UI synchronized with message cache for conversation {}", current_cid);
            }
        }
    }

    pub fn force_refresh_current_conversation(state: &mut super::core::AppState) {
        if let Some(cid) = state.cid {
            info!("Force refreshing conversation {}", cid);
            state.load_single_conversation_messages(cid);
        }
    }

    pub fn get_conversation_message_count(state: &super::core::AppState, cid: Uuid) -> usize {
        state.conversation_messages
            .get(&cid)
            .map(|msgs| msgs.len())
            .unwrap_or(0)
    }

    pub fn cleanup_old_conversations(state: &mut super::core::AppState) {
        if let Some(ref conversations) = state.conversations {
            let valid_ids: std::collections::HashSet<_> = conversations
                .iter()
                .map(|c| c.id)
                .collect();

            let old_count = state.conversation_messages.len();
            state.conversation_messages.retain(|cid, _| valid_ids.contains(cid));
            let removed = old_count - state.conversation_messages.len();

            if removed > 0 {
                info!("Cleaned up {} old conversation caches", removed);
            }
        }
    }

    pub fn add_message_to_conversation(
        state: &mut super::core::AppState,
        conversation_id: Uuid,
        message: MessageDto
    ) {
        // Aggiungi alla cache
        let conversation_cache = state
            .conversation_messages
            .entry(conversation_id)
            .or_insert_with(Vec::new);

        if !conversation_cache.iter().any(|existing| existing.id == message.id) {
            let insert_pos = conversation_cache
                .binary_search_by_key(&message.created_at, |m| m.created_at)
                .unwrap_or_else(|pos| pos);
            conversation_cache.insert(insert_pos, message.clone());
        }

        // Aggiungi all'UI se è la conversazione corrente
        if Some(conversation_id) == state.cid {
            if !state.messages.iter().any(|existing| existing.id == message.id) {
                let ui_insert_pos = state.messages
                    .binary_search_by_key(&message.created_at, |m| m.created_at)
                    .unwrap_or_else(|pos| pos);
                state.messages.insert(ui_insert_pos, message);
            }
        }
    }

    pub fn get_messages_for_conversation(
        state: &super::core::AppState,
        conversation_id: Uuid
    ) -> Option<&Vec<MessageDto>> {
        state.conversation_messages.get(&conversation_id)
    }

    pub fn clear_conversation_cache(state: &mut super::core::AppState, conversation_id: Uuid) {
        state.conversation_messages.remove(&conversation_id);
        if Some(conversation_id) == state.cid {
            state.messages.clear();
        }
    }
}