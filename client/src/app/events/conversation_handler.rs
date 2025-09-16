// conversation_handler.rs - Handler conversazioni ottimizzato

use crate::models::{UiEvent, Page, MessageDto, ConversationDto};
use tracing::{debug, info, error, warn};
use uuid::Uuid;

pub struct ConversationHandler;

impl ConversationHandler {
    // ... (altri metodi rimangono identici fino a handle_fetched_messages)

    // MODIFICATO: Rimosso refresh automatico per conversazioni sconosciute
    fn handle_fetched_messages(state: &mut crate::state::core::AppState, conversation_id: Uuid, messages: Vec<MessageDto>) {
        info!("Processing {} fetched messages for conversation {}", messages.len(), conversation_id);

        let messages_count = messages.len();
        // Aggiorna la cache dei messaggi
        state.conversation_messages.insert(conversation_id, messages.clone());

        // Se è la conversazione corrente, aggiorna anche la UI
        if Some(conversation_id) == state.cid {
            info!("Updating UI with {} messages for current conversation {}", messages.len(), conversation_id);
            state.messages = messages;
        } else {
            debug!("Messages cached for conversation {} (not current)", conversation_id);
        }

        // RIMOSSO: Non fare più refresh automatico se la conversazione non esiste
        // Ora viene gestito tramite FetchSingleConversation specifico

        // Notifica successo
        crate::app::events::helpers::add_system_message(
            state,
            format!("Sincronizzati {} messaggi", messages_count)
        );
    }

    // MODIFICATO: Migliore gestione del single conversation fetched
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
                debug!("Adding new conversation to list: {}", conversation.id);
                conversations.push(conversation.clone());

                // Sort conversations by created_at (most recent first)
                conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                // Notifica che è stata aggiunta una nuova conversazione
                crate::app::events::helpers::add_system_message(
                    state,
                    format!("Nuova conversazione aggiunta: {}", conversation.title)
                );
            }
        } else {
            // Create new conversation list
            debug!("Creating new conversation list with: {}", conversation.id);
            state.conversations = Some(vec![conversation.clone()]);
        }

        // Clean up old conversation caches periodically
        crate::app::events::helpers::cleanup_old_conversations(state);

        debug!("Single conversation fetch completed for: {}", conversation.id);
    }

    // NUOVO: Metodo helper per verificare se una conversazione esiste
    fn conversation_exists_in_list(state: &crate::state::core::AppState, conversation_id: Uuid) -> bool {
        state
            .conversations
            .as_ref()
            .map(|convs| convs.iter().any(|c| c.id == conversation_id))
            .unwrap_or(false)
    }

    // Gli altri metodi rimangono identici...
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
            UiEvent::FetchConversationMessages(conversation_id, reason) => {
                Self::handle_fetch_conversation_messages(state, conversation_id, reason);
            }
            UiEvent::FetchedMessages(conversation_id, messages) => {
                Self::handle_fetched_messages(state, conversation_id, messages);
            }
            _ => unreachable!("Invalid conversation event"),
        }
    }

    // ... (tutti gli altri metodi rimangono identici al file originale)
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

        if state.dm_stubs.contains_key(&conversation_id) {
            warn!("DM stub with ID {} already exists, not creating duplicate", conversation_id);
            crate::app::events::helpers::add_system_message(
                state,
                "Chat già esistente con questo utente".into()
            );
            return;
        }

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

        // Usa fetch specifico invece di refresh globale
        let _ = state.ui_tx.send(UiEvent::FetchSingleConversation(conversation_id));
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

    fn handle_fetch_conversation_messages(state: &mut crate::state::core::AppState, conversation_id: Uuid, reason: String) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            info!("Fetching messages for conversation {} (reason: {})", conversation_id, reason);

            state.rt.spawn(async move {
                match crate::api::chat::get_messages(&base, &token, conversation_id).await {
                    Ok(messages) => {
                        debug!("Successfully fetched {} messages for conversation {}", messages.len(), conversation_id);
                        let _ = tx.send(UiEvent::FetchedMessages(conversation_id, messages));
                    }
                    Err(e) => {
                        error!("Failed to fetch messages for conversation {}: {}", conversation_id, e);
                        let _ = tx.send(UiEvent::Error(format!(
                            "Errore caricamento messaggi conversazione {}: {}",
                            conversation_id, e
                        )));
                    }
                }
            });
        } else {
            warn!("Cannot fetch messages: no token available");
        }
    }
}