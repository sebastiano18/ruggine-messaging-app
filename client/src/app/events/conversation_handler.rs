// events/conversation_handler.rs - Gestione conversazioni
use crate::models::{UiEvent, Page, MessageDto, ConversationDto};
use tracing::{debug, info, error, warn};
use uuid::Uuid;

pub struct ConversationHandler;

impl ConversationHandler {
    pub fn handle(state: &mut crate::state::core::AppState, event: UiEvent) {
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
            UiEvent::FetchSingleConversation(conversation_id) => {
                Self::handle_fetch_single_conversation(state, conversation_id);
            }
            UiEvent::SingleConversationFetched(conversation) => {
                Self::handle_single_conversation_fetched(state, conversation);
            }
            _ => unreachable!("Invalid conversation event"),
        }
    }

    fn handle_conversation_opened(state: &mut crate::state::core::AppState, cid: Uuid) {
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
            debug!("Loaded {} messages from cache for conversation {}", cached_messages.len(), cid);
        } else if state.is_initial_load_complete {
            debug!("Loading messages from network for conversation {}", cid);
            state.load_single_conversation_messages(cid);
            state.messages = vec![];

            if let Some(ref conversations) = state.conversations {
                if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                    let welcome_msg = MessageDto::system_message(format!("Benvenuto in {}!", conv.title));
                    state.messages.push(welcome_msg);
                }
            }
        } else {
            state.messages = vec![];
            crate::app::events::helpers::add_system_message(state, "Caricamento messaggi...".into());
        }
    }

    fn handle_conversation_created(state: &mut crate::state::core::AppState, conversation_id: Uuid) {
        info!("New conversation created: {}", conversation_id);
        state.conversation_messages.insert(conversation_id, vec![]);
        crate::app::events::helpers::add_system_message(state, "Conversazione creata!".into());
        state.request_conversations_refresh = true;
    }

    fn handle_dm_stub_created(state: &mut crate::state::core::AppState, conversation_id: Uuid, other_username: String) {
        info!("Creating DM stub for conversation {} with {}", conversation_id, other_username);

        // CONTROLLO DUPLICATI: Verifica se esiste già una conversazione con questo ID
        if let Some(ref conversations) = state.conversations {
            let existing_count = conversations.iter().filter(|c| c.id == conversation_id).count();
            if existing_count > 0 {
                error!("ATTEMPTING TO CREATE DUPLICATE STUB! ID {} already exists {} times",
                       conversation_id, existing_count);
                crate::app::events::helpers::add_system_message(
                    state,
                    "Errore: conversazione già esistente".into()
                );
                return;
            }
        }

        // CONTROLLO DM STUB: Verifica se esiste già uno stub con questo ID
        if state.dm_stubs.contains_key(&conversation_id) {
            warn!("DM stub with ID {} already exists, not creating duplicate", conversation_id);
            crate::app::events::helpers::add_system_message(
                state,
                "Chat già esistente con questo utente".into()
            );
            return;
        }

        // Crea lo stub in modo sicuro
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

        crate::app::events::helpers::add_system_message(
            state,
            format!("Chat con {} aperta - invia un messaggio per iniziare!", other_username)
        );

        debug!("Successfully created DM stub: {} -> {}", conversation_id, other_username);
    }

    fn handle_conversation_added(state: &mut crate::state::core::AppState, conversation_id: Uuid, reason: String) {
        info!("User added to conversation {} (reason: {})", conversation_id, reason);
        crate::app::events::helpers::add_system_message(state, format!("Aggiunto a nuova conversazione ({})", reason));
        state.conversation_messages.entry(conversation_id).or_insert_with(Vec::new);

        // Richiedi refresh per ottenere i dettagli della nuova conversazione
        state.request_conversations_refresh = true;
    }

    fn handle_conversation_list_updated(state: &mut crate::state::core::AppState) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            debug!("Executing conversation list refresh");

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversations(&base, &token).await {
                    Ok(conversations) => {
                        info!("Successfully refreshed {} conversations", conversations.len());
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    }
                    Err(e) => {
                        error!("Failed to refresh conversations: {}", e);
                        let _ = tx.send(UiEvent::Error(format!("Errore aggiornamento conversazioni: {}", e)));
                    }
                }
            });
        } else {
            warn!("Cannot refresh conversations: no token available");
        }
    }

    fn handle_fetch_single_conversation(state: &mut crate::state::core::AppState, conversation_id: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            info!("Fetching single conversation: {}", conversation_id);

            state.rt.spawn(async move {
                match crate::api::conversation::get_single_conversation(&base, &token, conversation_id).await {
                    Ok(conversation) => {
                        debug!("Successfully fetched conversation: {} ({})", conversation.id, conversation.title);
                        let _ = tx.send(UiEvent::SingleConversationFetched(conversation));
                    }
                    Err(e) => {
                        error!("Failed to fetch single conversation {}: {}", conversation_id, e);
                        let _ = tx.send(UiEvent::Error(format!(
                            "Errore caricamento conversazione {}: {}",
                            conversation_id, e
                        )));
                    }
                }
            });
        } else {
            warn!("Cannot fetch conversation: no token available");
        }
    }

    fn handle_single_conversation_fetched(state: &mut crate::state::core::AppState, conversation: ConversationDto) {
        info!("Processing fetched conversation: {} ({})", conversation.id, conversation.title);

        // Initialize conversation messages cache if needed
        state.conversation_messages.entry(conversation.id).or_insert_with(Vec::new);

        // Add or update conversation in the list
        if let Some(ref mut conversations) = state.conversations {
            // Check if conversation already exists
            if let Some(existing_pos) = conversations.iter().position(|c| c.id == conversation.id) {
                // Update existing conversation
                debug!("Updating existing conversation: {}", conversation.id);
                conversations[existing_pos] = conversation.clone();
            } else {
                // Add new conversation
                debug!("Adding new conversation: {}", conversation.id);
                conversations.push(conversation.clone());

                // Sort conversations by created_at (most recent first)
                conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
        } else {
            // Create new conversation list
            state.conversations = Some(vec![conversation.clone()]);
        }

        crate::app::events::helpers::add_system_message(
            state,
            format!("Conversazione {} aggiornata", conversation.title)
        );

        // Clean up old conversation caches
        crate::app::events::helpers::cleanup_old_conversations(state);

        debug!("Single conversation fetch completed for: {}", conversation.id);
    }
}