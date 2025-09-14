// events.rs - Updated with conversation management

use crate::models::*;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct EventHandler;

impl EventHandler {
    pub fn handle_event(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            // Authentication events
            UiEvent::LoginStarted
            | UiEvent::RegisterStarted
            | UiEvent::Logged(..)
            | UiEvent::LoggedOut => {
                Self::handle_auth_events(state, event);
            }

            // WebSocket events
            UiEvent::WsConnected
            | UiEvent::WsDisconnected
            | UiEvent::WsError(..)
            | UiEvent::WsControlReady(..)
            | UiEvent::WsIncoming(..) => {
                Self::handle_websocket_events(state, event);
            }

            // Data loading events
            UiEvent::ConversationsLoaded(..)
            | UiEvent::AllMessagesLoaded(..)
            | UiEvent::RefreshedMsgs(..)
            | UiEvent::InitialLoadComplete
            | UiEvent::LoadingProgress(..)
            | UiEvent::SingleConversationLoaded(..) => {
                Self::handle_data_events(state, event);
            }

            // Conversation events
            UiEvent::Opened(..) | UiEvent::ConversationCreated(..) | UiEvent::DmStubCreated(..)
            | UiEvent::ConversationAdded(..) | UiEvent::ConversationListUpdated => {
                Self::handle_conversation_events(state, event);
            }

            // Message events
            UiEvent::MessageSendFailed(..) => {
                Self::handle_message_events(state, event);
            }

            // FETCH-ON-SUBSCRIBE EVENTS
            UiEvent::FetchConversationMessages(..) | UiEvent::FetchedMessages(..) => {
                Self::handle_fetch_events(state, event);
            }

            // General events
            UiEvent::Info(..) | UiEvent::Error(..) | UiEvent::InviteCreated(..) => {
                Self::handle_general_events(state, event);
            }
        }
    }

    // === FETCH EVENT HANDLERS ===
    fn handle_fetch_events(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::FetchConversationMessages(conversation_id, reason) => {
                Self::handle_fetch_request(state, conversation_id, reason);
            }
            UiEvent::FetchedMessages(conversation_id, messages) => {
                Self::handle_fetched_messages(state, conversation_id, messages);
            }
            _ => unreachable!("Invalid fetch event"),
        }
    }

    fn handle_fetch_request(state: &mut super::core::AppState, conversation_id: Uuid, reason: String) {
        info!("Processing fetch request for conversation {} (reason: {})", conversation_id, reason);

        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::chat::fetch_conversation_messages(&base, &token, conversation_id, Some(50)).await {
                    Ok(messages) => {
                        info!("Successfully fetched {} messages for conversation {}", messages.len(), conversation_id);
                        let _ = tx.send(UiEvent::FetchedMessages(conversation_id, messages));
                    }
                    Err(e) => {
                        error!("Failed to fetch messages for conversation {}: {}", conversation_id, e);
                        let _ = tx.send(UiEvent::Error(format!(
                            "Errore nel caricamento messaggi: {}", e
                        )));
                    }
                }
            });

            Self::add_system_message(state, format!(
                "Sincronizzando messaggi... ({})", reason
            ));
        } else {
            warn!("Cannot fetch messages: no authentication token");
        }
    }

    fn handle_fetched_messages(state: &mut super::core::AppState, conversation_id: Uuid, mut messages: Vec<MessageDto>) {
        info!("Processing {} fetched messages for conversation {}", messages.len(), conversation_id);

        messages.retain(|msg| Self::validate_incoming_message(msg));
        Self::deduplicate_messages(&mut messages);
        messages.sort_by_key(|m| m.created_at);

        if messages.is_empty() {
            debug!("No valid messages to process for conversation {}", conversation_id);
            return;
        }

        let mut new_messages = Vec::new();
        {
            let conversation_cache = state
                .conversation_messages
                .entry(conversation_id)
                .or_insert_with(Vec::new);

            for msg in messages {
                if !conversation_cache.iter().any(|existing| existing.id == msg.id) {
                    let insert_pos = conversation_cache
                        .binary_search_by(|existing| {
                            existing
                                .created_at
                                .cmp(&msg.created_at)
                                .then_with(|| existing.id.cmp(&msg.id))
                        })
                        .unwrap_or_else(|pos| pos);

                    conversation_cache.insert(insert_pos, msg.clone());
                    new_messages.push(msg);
                }
            }
        }

        let new_messages_count = new_messages.len();

        if Some(conversation_id) == state.cid && !new_messages.is_empty() {
            for msg in new_messages {
                Self::update_ui_messages_improved(state, msg);
            }
        }

        if new_messages_count > 0 {
            info!("Added {} new messages to cache for conversation {}", new_messages_count, conversation_id);

            if Some(conversation_id) == state.cid {
                let notification = MessageDto::fetch_notification(
                    conversation_id,
                    new_messages_count,
                    "fetch automatico"
                );
                state.messages.push(notification);
            }
        } else {
            debug!("All fetched messages were already in cache for conversation {}", conversation_id);
        }
    }

    // === CONVERSATION EVENT HANDLERS ===
    fn handle_conversation_events(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::Opened(cid) => {
                Self::handle_conversation_opened(state, cid);
            }
            UiEvent::ConversationCreated(conversation_id) => {
                Self::handle_conversation_created(state, conversation_id);
            }
            UiEvent::DmStubCreated(conversation_id, other_username) => {
                Self::handle_dm_stub_created(state, conversation_id, other_username);
            }
            UiEvent::ConversationAdded(conversation_id, reason) => {
                Self::handle_conversation_added(state, conversation_id, reason);
            }
            UiEvent::ConversationListUpdated => {
                Self::handle_conversation_list_updated(state);
            }
            _ => unreachable!("Invalid conversation event"),
        }
    }

    // NEW: Handle conversation added event
    fn handle_conversation_added(state: &mut super::core::AppState, conversation_id: Uuid, reason: String) {
        info!("User added to conversation {} (reason: {})", conversation_id, reason);

        Self::add_system_message(state, format!("Aggiunto a nuova conversazione ({})", reason));
        state.conversation_messages.entry(conversation_id).or_insert_with(Vec::new);
    }

    // NEW: Handle conversation list refresh
    fn handle_conversation_list_updated(state: &mut super::core::AppState) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversations(&base, &token).await {
                    Ok(conversations) => {
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    }
                    Err(e) => {
                        error!("Failed to refresh conversations: {}", e);
                        let _ = tx.send(UiEvent::Error(format!("Errore aggiornamento conversazioni: {}", e)));
                    }
                }
            });
        }
    }

    fn handle_dm_stub_created(state: &mut super::core::AppState, conversation_id: Uuid, other_username: String) {
        info!("Creating DM stub for conversation {} with {}", conversation_id, other_username);

        state.add_dm_stub(conversation_id, other_username.clone());
        state.conversation_messages.insert(conversation_id, vec![]);

        state.cid = Some(conversation_id);
        state.conv_title = other_username.clone();
        state.page = Page::Chat;
        state.messages = vec![];

        let welcome_msg = MessageDto::system_message(
            format!("Nuova chat con {}. Scrivi il primo messaggio!", other_username)
        );
        state.messages.push(welcome_msg);

        Self::add_system_message(state, format!("Chat con {} aperta - invia un messaggio per iniziare!", other_username));
    }

    // === AUTH EVENT HANDLERS ===
    fn handle_auth_events(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::LoginStarted => {
                state.login_state = LoginState::LoggingIn;
                Self::add_system_message(state, "Effettuando login...".into());
            }
            UiEvent::RegisterStarted => {
                state.login_state = LoginState::Registering;
                Self::add_system_message(state, "Registrando utente...".into());
            }
            UiEvent::Logged(token, user_id) => {
                Self::handle_login_success(state, token, user_id);
            }
            UiEvent::LoggedOut => {
                Self::handle_logout(state);
            }
            _ => unreachable!("Invalid auth event"),
        }
    }

    fn handle_websocket_events(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::WsControlReady(ctrl) => {
                state.ws_ctrl = Some(ctrl);
                debug!("WebSocket control ready");
            }
            UiEvent::WsConnected => {
                state.ws_status = WsStatus::Connected;
                Self::add_system_message(state, "WebSocket connesso".into());
            }
            UiEvent::WsDisconnected => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;
                Self::add_system_message(state, "WebSocket disconnesso".into());
            }
            UiEvent::WsError(error) => {
                state.ws_status = WsStatus::Disconnected;
                state.ws_ctrl = None;
                Self::add_system_message(state, format!("WebSocket errore: {}", error));
            }
            UiEvent::WsIncoming(msg) => {
                Self::handle_incoming_message(state, msg);
            }
            _ => unreachable!("Invalid websocket event"),
        }
    }

    fn handle_data_events(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::ConversationsLoaded(conversations) => {
                Self::handle_conversations_loaded(state, conversations);
            }
            UiEvent::AllMessagesLoaded(messages_map) => {
                Self::handle_all_messages_loaded(state, messages_map);
            }
            UiEvent::RefreshedMsgs(list) => {
                Self::handle_messages_refreshed(state, list);
            }
            UiEvent::InitialLoadComplete => {
                state.is_initial_load_complete = true;
                state.is_loading = false;
                Self::add_system_message(state, "Tutti i dati caricati!".into());
                info!("Initial data load completed");
            }
            UiEvent::LoadingProgress(progress) => {
                Self::add_system_message(state, format!("{}", progress));
            }
            UiEvent::SingleConversationLoaded(conversation) => {
                Self::handle_single_conversation_loaded(state, conversation);
            }
            _ => unreachable!("Invalid data event"),
        }
    }

    fn handle_message_events(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::MessageSendFailed(failed_message_id) => {
                Self::handle_message_send_failed(state, failed_message_id);
            }
            _ => unreachable!("Invalid message event"),
        }
    }

    fn handle_general_events(state: &mut super::core::AppState, event: UiEvent) {
        match event {
            UiEvent::Info(s) => {
                Self::add_system_message(state, s);
            }
            UiEvent::Error(s) => {
                Self::add_system_message(state, format!("{}", s));
                state.login_state = LoginState::Idle;
                state.is_loading = false;
            }
            UiEvent::InviteCreated(token) => {
                state.last_created_invite = Some(token);
                Self::add_system_message(state, "Invito creato con successo".into());
            }
            _ => unreachable!("Invalid general event"),
        }
    }

    fn handle_login_success(state: &mut super::core::AppState, token: String, user_id: Uuid) {
        state.token = Some(token.clone());
        state.user_id = Some(user_id);
        state.login_state = LoginState::LoggedIn;
        Self::add_system_message(state, "Login effettuato con successo".into());
        state.page = Page::Conversations;

        info!("User {} logged in successfully", user_id);
        state.preload_all_data(token);
    }

    fn handle_conversation_opened(state: &mut super::core::AppState, cid: Uuid) {
        debug!("Opening conversation: {}", cid);

        state.cid = Some(cid);
        state.page = Page::Chat;

        if let Some(ref conversations) = state.conversations {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                state.conv_title = conv.title.clone();
                debug!("Conversation title set to: {}", conv.title);
            }
        }

        if let Some(cached_messages) = state.conversation_messages.get(&cid) {
            state.messages = cached_messages.clone();
            debug!(
                "Loaded {} messages from cache for conversation {}",
                cached_messages.len(),
                cid
            );
        } else if state.is_initial_load_complete {
            debug!("Loading messages from network for conversation {}", cid);
            state.load_single_conversation_messages(cid);
            state.messages = vec![];

            if let Some(ref conversations) = state.conversations {
                if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                    let welcome_msg =
                        MessageDto::system_message(format!("Benvenuto in {}!", conv.title));
                    state.messages.push(welcome_msg);
                }
            }
        } else {
            state.messages = vec![];
            Self::add_system_message(state, "Caricamento messaggi...".into());
        }
    }

    fn handle_incoming_message(state: &mut super::core::AppState, msg: MessageDto) {
        let message_conversation_id = msg.conversation_id;

        if !Self::validate_incoming_message(&msg) {
            return;
        }

        debug!(
            "Processing incoming message: {} from {} in conversation {}",
            msg.content.chars().take(50).collect::<String>(),
            msg.author_username,
            message_conversation_id
        );

        if state.dm_stubs.contains_key(&message_conversation_id) {
            info!("Converting DM stub {} to real conversation", message_conversation_id);

            let target_username = state.dm_stubs.remove(&message_conversation_id).unwrap();

            let real_conversation = ConversationDto {
                id: message_conversation_id,
                kind: "dm".to_string(),
                title: if msg.author_id == state.user_id.unwrap_or(Uuid::nil()) {
                    target_username
                } else {
                    msg.author_username.clone()
                },
                owner_id: state.user_id.unwrap_or(Uuid::nil()),
                created_at: msg.created_at,
            };

            if let Some(ref mut conversations) = state.conversations {
                conversations.push(real_conversation);
            } else {
                state.conversations = Some(vec![real_conversation]);
            }

            Self::add_system_message(state, format!("Chat con {} ora attiva!",
                                                    if msg.author_id == state.user_id.unwrap_or(Uuid::nil()) {
                                                        "te stesso"
                                                    } else {
                                                        &msg.author_username
                                                    }
            ));
        }

        let conversation_exists = state
            .conversations
            .as_ref()
            .map(|convs| convs.iter().any(|c| c.id == message_conversation_id))
            .unwrap_or(false);

        if !conversation_exists {
            info!(
                "Received message for unknown conversation {}",
                message_conversation_id
            );

            let stub_conversation = ConversationDto {
                id: message_conversation_id,
                kind: "dm".to_string(),
                title: msg.author_username.clone(),
                owner_id: msg.author_id,
                created_at: msg.created_at,
            };

            if let Some(ref mut conversations) = state.conversations {
                conversations.push(stub_conversation);
            } else {
                state.conversations = Some(vec![stub_conversation]);
            }

            Self::add_system_message(state, format!("Nuovo messaggio da {}", msg.author_username));
        }

        if !Self::update_message_cache_improved(state, &msg) {
            debug!("Message already exists in cache, skipping: {}", msg.id);
            return;
        }

        if Some(message_conversation_id) == state.cid {
            Self::update_ui_messages_improved(state, msg);
        } else {
            debug!(
                "Message cached for different conversation: {} -> {} (current: {:?})",
                msg.author_username, message_conversation_id, state.cid
            );
        }
    }

    fn handle_messages_refreshed(state: &mut super::core::AppState, mut list: Vec<MessageDto>) {
        list.retain(|msg| Self::validate_incoming_message(msg));
        Self::deduplicate_messages(&mut list);
        list.sort_by_key(|m| m.created_at);

        debug!("Refreshing messages: {} valid messages", list.len());

        state.messages = list.clone();

        if let Some(cid) = state.cid {
            state.conversation_messages.insert(cid, list);
            debug!(
                "Refreshed {} messages for conversation {}",
                state.messages.len(),
                cid
            );
        }
    }

    fn handle_conversations_loaded(
        state: &mut super::core::AppState,
        mut conversations: Vec<ConversationDto>,
    ) {
        let old_count = state.conversations.as_ref().map(|c| c.len()).unwrap_or(0);

        if let Some(ref existing_conversations) = state.conversations {
            for existing_conv in existing_conversations {
                if !conversations.iter().any(|c| c.id == existing_conv.id) {
                    if state.conversation_messages.contains_key(&existing_conv.id) {
                        info!("Keeping DM stub for conversation {}: {}", existing_conv.id, existing_conv.title);
                        conversations.push(existing_conv.clone());
                    }
                }
            }
        }

        state.conversations = Some(conversations.clone());
        let new_count = conversations.len();

        info!("Loaded {} conversations (was {})", new_count, old_count);
        Self::add_system_message(state, format!("{} conversazioni caricate", new_count));

        if new_count > old_count {
            Self::add_system_message(state, "Lista conversazioni aggiornata".into());
        }

        if let Some(current_cid) = state.cid {
            if !conversations.iter().any(|c| c.id == current_cid) {
                warn!(
                    "Current conversation {} no longer exists, clearing selection",
                    current_cid
                );
                state.cid = None;
                state.conv_title.clear();
                state.messages.clear();
                state.page = Page::Conversations;
                Self::add_system_message(
                    state,
                    "La conversazione corrente non esiste più".into(),
                );
            }
        }

        Self::cleanup_old_conversations(state);
    }

    fn handle_single_conversation_loaded(
        state: &mut super::core::AppState,
        conversation: ConversationDto,
    ) {
        if let Some(ref mut conversations) = state.conversations {
            if !conversations.iter().any(|c| c.id == conversation.id) {
                conversations.push(conversation.clone());
                info!(
                    "Added new conversation '{}' to existing list",
                    conversation.title
                );
                Self::add_system_message(
                    state,
                    format!("Nuova conversazione: {}", conversation.title),
                );
            }
        } else {
            state.conversations = Some(vec![conversation.clone()]);
            info!("Initialized conversations list with new conversation");
        }
    }

    fn handle_all_messages_loaded(
        state: &mut super::core::AppState,
        messages_map: HashMap<Uuid, Vec<MessageDto>>,
    ) {
        let total_messages: usize = messages_map.values().map(|v| v.len()).sum();
        info!(
            "Loaded {} total messages across {} conversations",
            total_messages,
            messages_map.len()
        );

        let mut cleaned_map = HashMap::new();
        for (conv_id, mut messages) in messages_map {
            messages.retain(|msg| Self::validate_incoming_message(msg));
            Self::deduplicate_messages(&mut messages);
            messages.sort_by_key(|m| m.created_at);

            if !messages.is_empty() {
                cleaned_map.insert(conv_id, messages);
            }
        }

        for (conv_id, existing_messages) in &state.conversation_messages {
            if !cleaned_map.contains_key(conv_id) {
                cleaned_map.insert(*conv_id, existing_messages.clone());
            }
        }

        state.conversation_messages = cleaned_map;

        if let Some(cid) = state.cid {
            if let Some(msgs) = state.conversation_messages.get(&cid).cloned() {
                state.messages = msgs;
                debug!(
                    "Loaded {} messages for current conversation {}",
                    state.messages.len(),
                    cid
                );
            }
        }
    }

    fn handle_message_send_failed(state: &mut super::core::AppState, failed_message_id: Uuid) {
        warn!("Message send failed: {}", failed_message_id);

        let old_ui_len = state.messages.len();
        state.messages.retain(|msg| msg.id != failed_message_id);
        let ui_removed = old_ui_len - state.messages.len();

        if let Some(cid) = state.cid {
            if let Some(messages) = state.conversation_messages.get_mut(&cid) {
                let old_cache_len = messages.len();
                messages.retain(|msg| msg.id != failed_message_id);
                let cache_removed = old_cache_len - messages.len();
                debug!(
                    "Removed failed message - UI: {}, Cache: {}",
                    ui_removed, cache_removed
                );
            }
        }

        Self::add_system_message(state, "Invio messaggio fallito".into());
    }

    fn handle_conversation_created(state: &mut super::core::AppState, conversation_id: Uuid) {
        info!("New conversation created: {}", conversation_id);
        state.conversation_messages.insert(conversation_id, vec![]);
        Self::add_system_message(state, "Conversazione creata!".into());
        state.request_conversations_refresh = true;
    }

    fn handle_logout(state: &mut super::core::AppState) {
        info!("User logout");

        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

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

        state.group_name.clear();
        state.dm_user_username_input.clear();
        state.invite_conversation_id.clear();
        state.last_created_invite = None;
        state.last_invite_token = None;

        state.conversation_messages.clear();
        state.is_initial_load_complete = false;
        state.is_loading = false;
        state.dm_stubs.clear();

        state
            .messages
            .push(MessageDto::system_message("Logout effettuato".into()));
    }

    // === HELPER METHODS ===

    fn add_system_message(state: &mut super::core::AppState, content: String) {
        let msg = MessageDto::system_message(content);
        state.messages.push(msg);

        let system_message_count = state
            .messages
            .iter()
            .filter(|m| m.author_id == Uuid::nil())
            .count();

        if system_message_count > 50 {
            let mut non_system: Vec<_> = state
                .messages
                .iter()
                .filter(|m| m.author_id != Uuid::nil())
                .cloned()
                .collect();

            let recent_system: Vec<_> = state
                .messages
                .iter()
                .filter(|m| m.author_id == Uuid::nil())
                .rev()
                .take(20)
                .cloned()
                .collect();

            non_system.extend(recent_system);
            non_system.sort_by_key(|m| m.created_at);
            state.messages = non_system;
        }
    }

    fn validate_incoming_message(msg: &MessageDto) -> bool {
        if msg.conversation_id == Uuid::nil() {
            warn!(
                "Received message with nil conversation_id, ignoring: {:?}",
                msg.id
            );
            return false;
        }

        if msg.content.trim().is_empty() {
            warn!(
                "Received message with empty content, ignoring: {:?}",
                msg.id
            );
            return false;
        }

        if msg.author_id == Uuid::nil() && msg.author_username != "system" {
            warn!(
                "Received message with nil author_id (non-system), ignoring: {:?}",
                msg.id
            );
            return false;
        }

        true
    }

    fn update_message_cache_improved(state: &mut super::core::AppState, msg: &MessageDto) -> bool {
        let conversation_cache = state
            .conversation_messages
            .entry(msg.conversation_id)
            .or_insert_with(Vec::new);

        if conversation_cache
            .iter()
            .any(|existing| existing.id == msg.id)
        {
            return false;
        }

        let insert_pos = conversation_cache
            .binary_search_by(|existing| {
                existing
                    .created_at
                    .cmp(&msg.created_at)
                    .then_with(|| existing.id.cmp(&msg.id))
            })
            .unwrap_or_else(|pos| pos);

        conversation_cache.insert(insert_pos, msg.clone());

        debug!(
            "Message added to cache for conversation {}: {} chars from {}",
            msg.conversation_id,
            msg.content.len(),
            msg.author_username
        );

        true
    }

    fn update_ui_messages_improved(state: &mut super::core::AppState, msg: MessageDto) {
        if state.messages.iter().any(|existing| existing.id == msg.id) {
            debug!("Message already exists in UI, skipping: {}", msg.id);
            return;
        }

        let ui_insert_pos = state
            .messages
            .binary_search_by(|existing| {
                existing
                    .created_at
                    .cmp(&msg.created_at)
                    .then_with(|| existing.id.cmp(&msg.id))
            })
            .unwrap_or_else(|pos| pos);

        state.messages.insert(ui_insert_pos, msg.clone());

        debug!(
            "Message added to current conversation UI: {} characters from {}",
            msg.content.len(),
            msg.author_username
        );
    }

    fn deduplicate_messages(messages: &mut Vec<MessageDto>) {
        messages.sort_by_key(|m| m.id);
        messages.dedup_by_key(|m| m.id);
        messages.sort_by_key(|m| m.created_at);
    }

    pub fn cleanup_old_conversations(state: &mut super::core::AppState) {
        if let Some(ref conversations) = state.conversations {
            let valid_ids: std::collections::HashSet<_> =
                conversations.iter().map(|c| c.id).collect();

            let old_count = state.conversation_messages.len();
            state
                .conversation_messages
                .retain(|cid, _| valid_ids.contains(cid));
            let removed = old_count - state.conversation_messages.len();

            if removed > 0 {
                info!("Cleaned up {} old conversation caches", removed);
            }
        }
    }
}