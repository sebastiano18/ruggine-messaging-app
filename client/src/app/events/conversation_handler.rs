// events/conversation_handler.rs - Gestione conversazioni
use crate::models::{UiEvent, Page, MessageDto};
use tracing::{debug, info, error};
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

        crate::app::events::helpers::add_system_message(state, format!("Chat con {} aperta - invia un messaggio per iniziare!", other_username));
    }

    fn handle_conversation_added(state: &mut crate::state::core::AppState, conversation_id: Uuid, reason: String) {
        info!("User added to conversation {} (reason: {})", conversation_id, reason);
        crate::app::events::helpers::add_system_message(state, format!("Aggiunto a nuova conversazione ({})", reason));
        state.conversation_messages.entry(conversation_id).or_insert_with(Vec::new);
    }

    fn handle_conversation_list_updated(state: &mut crate::state::core::AppState) {
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
}