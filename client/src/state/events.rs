use crate::models::*;
use uuid::Uuid;

pub struct EventHandler;

impl EventHandler {
    pub fn handle_event(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::Info(s) => {
                state.messages.push(MessageDto::system_message(s));
            }
            UiEvent::Error(s) => {
                state.messages.push(MessageDto::system_message(format!("⚠ {}", s)));
                state.login_state = LoginState::Idle;
                state.is_loading = false;
            }
            UiEvent::LoginStarted => {
                state.login_state = LoginState::LoggingIn;
                state.messages.push(MessageDto::system_message("🔄 Effettuando login...".into()));
            }
            UiEvent::RegisterStarted => {
                state.login_state = LoginState::Registering;
                state.messages.push(MessageDto::system_message("🔄 Registrando utente...".into()));
            }
            UiEvent::Logged(token, user_id) => {
                Self::handle_login_success(state, token, user_id);
            }
            UiEvent::Opened(cid) => {
                Self::handle_conversation_opened(state, cid);
            }
            UiEvent::WsControlReady(ctrl) => {
                state.ws_ctrl = Some(ctrl);
            }
            UiEvent::WsConnected => {
                state.ws_status = WsStatus::Connected;
                state.messages.push(MessageDto::system_message("🟢 WebSocket connesso".into()));
            }
            UiEvent::WsDisconnected => {
                state.ws_status = WsStatus::Disconnected;
                state.messages.push(MessageDto::system_message("🔴 WebSocket disconnesso".into()));
            }
            UiEvent::WsError(error) => {
                state.ws_status = WsStatus::Disconnected;
                state.messages.push(MessageDto::system_message(format!("⚠ WebSocket errore: {}", error)));
            }
            UiEvent::WsIncoming(msg) => {
                Self::handle_incoming_message(state, msg);
            }
            UiEvent::RefreshedMsgs(list) => {
                Self::handle_messages_refreshed(state, list);
            }
            UiEvent::ConversationsLoaded(conversations) => {
                state.conversations = Some(conversations);
                state.messages.push(MessageDto::system_message("📋 Conversazioni caricate".into()));
            }
            UiEvent::AllMessagesLoaded(messages_map) => {
                Self::handle_all_messages_loaded(state, messages_map);
            }
            UiEvent::InitialLoadComplete => {
                state.is_initial_load_complete = true;
                state.is_loading = false;
                state.messages.push(MessageDto::system_message("✅ Tutti i dati caricati!".into()));
            }
            UiEvent::LoadingProgress(progress) => {
                state.messages.push(MessageDto::system_message(format!("📊 {}", progress)));
            }
            UiEvent::InviteCreated(token) => {
                state.last_created_invite = Some(token);
                state.messages.push(MessageDto::system_message("🎉 Invito creato con successo".into()));
            }
            UiEvent::MessageSendFailed(failed_message_id) => {
                Self::handle_message_send_failed(state, failed_message_id);
            }
            UiEvent::LoggedOut => {
                Self::handle_logout(state);
            }
        }
    }

    fn handle_login_success(state: &mut super::core::AppState, token: String, user_id: Uuid) {
        state.token = Some(token.clone());
        state.user_id = Some(user_id);
        state.login_state = LoginState::LoggedIn;
        state.messages.push(MessageDto::system_message("✅ Login effettuato con successo".into()));
        state.page = Page::Conversations;

        // Avvia precaricamento completo dopo il login
        state.preload_all_data(token);
    }

    fn handle_conversation_opened(state: &mut super::core::AppState, cid: Uuid) {
        state.cid = Some(cid);
        if let Some(ref conversations) = state.conversations {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                state.conv_title = conv.title.clone();
            }
        }
        state.page = Page::Chat;

        // Carica messaggi dalla cache invece che dalla rete
        if let Some(cached_messages) = state.conversation_messages.get(&cid) {
            state.messages = cached_messages.clone();
        } else if state.is_initial_load_complete {
            // Se il caricamento iniziale è completo ma non abbiamo questi messaggi,
            // probabilmente è una conversazione nuova - carica dalla rete
            state.load_single_conversation_messages(cid);
        } else {
            // Se il caricamento iniziale non è completo, svuota i messaggi
            state.messages.clear();
        }
    }

    fn handle_incoming_message(state: &mut super::core::AppState, msg: MessageDto) {
        // Aggiungi ai messaggi correnti se siamo nella chat
        state.messages.push(msg.clone());

        // Aggiungi anche alla cache se abbiamo una conversazione corrente
        if let Some(cid) = state.cid {
            state.conversation_messages
                .entry(cid)
                .or_insert_with(Vec::new)
                .push(msg);
        }
    }

    fn handle_messages_refreshed(state: &mut super::core::AppState, list: Vec<MessageDto>) {
        state.messages = list.clone();
        // Aggiorna anche la cache se abbiamo una conversazione corrente
        if let Some(cid) = state.cid {
            state.conversation_messages.insert(cid, list);
        }
    }

    fn handle_all_messages_loaded(state: &mut super::core::AppState, messages_map: std::collections::HashMap<Uuid, Vec<MessageDto>>) {
        state.conversation_messages = messages_map;
        // Se c'è una conversazione corrente, carica i suoi messaggi
        if let Some(cid) = state.cid {
            if let Some(msgs) = state.conversation_messages.get(&cid) {
                state.messages = msgs.clone();
            }
        }
    }

    fn handle_message_send_failed(state: &mut super::core::AppState, failed_message_id: Uuid) {
        // Remove the failed optimistic message from both current messages and cache
        state.messages.retain(|msg| msg.id != failed_message_id);

        if let Some(cid) = state.cid {
            if let Some(messages) = state.conversation_messages.get_mut(&cid) {
                messages.retain(|msg| msg.id != failed_message_id);
            }
        }
    }

    fn handle_logout(state: &mut super::core::AppState) {
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

        // Pulizia campi inviti
        state.invite_conversation_id.clear();
        state.last_created_invite = None;

        // Pulizia cache precaricamento
        state.conversation_messages.clear();
        state.is_initial_load_complete = false;
        state.is_loading = false;

        state.messages.push(MessageDto::system_message("👋 Logout effettuato".into()));
    }
}