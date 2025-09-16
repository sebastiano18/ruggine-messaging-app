// events/mod.rs - Complete EventDispatcher with all handlers

use crate::models::UiEvent;
use crate::state::core::AppState;
use tracing::{debug, warn};

pub mod conversation_handler;
pub mod websocket_handler;
pub mod message_handler;
pub mod helpers;

use conversation_handler::ConversationHandler;
use websocket_handler::WebSocketHandler;
use message_handler::MessageHandler;

pub struct EventDispatcher;

impl EventDispatcher {
    pub fn handle_event(state: &mut AppState, event: UiEvent) {
        debug!("Handling event: {:?}", std::mem::discriminant(&event));

        match event {
            // Auth events
            UiEvent::LoginStarted => {
                debug!("Login started");
                state.login_state = crate::models::LoginState::LoggingIn;
            }
            UiEvent::RegisterStarted => {
                debug!("Registration started");
                state.login_state = crate::models::LoginState::Registering;
            }
            UiEvent::Logged(token, user_id) => {
                debug!("User logged in: {}", user_id);
                state.token = Some(token.clone());
                state.user_id = Some(user_id);
                state.login_state = crate::models::LoginState::LoggedIn;
                state.page = crate::models::Page::Conversations;

                // Start preloading data
                state.preload_all_data(token);
            }
            UiEvent::LoggedOut => {
                debug!("User logged out");
                state.token = None;
                state.user_id = None;
                state.login_state = crate::models::LoginState::Idle;
                state.page = crate::models::Page::Auth;
                state.conversations = None;
                state.conversation_messages.clear();
                state.messages.clear();
                state.dm_stubs.clear();
            }

            // WebSocket events
            UiEvent::WsControlReady(_) | UiEvent::WsConnected | UiEvent::WsDisconnected
            | UiEvent::WsError(_) | UiEvent::WsIncoming(_) => {
                WebSocketHandler::handle(state, event);
            }

            // Conversation events - AGGIORNATO con fetch events
            UiEvent::Opened(_) | UiEvent::ConversationCreated(_) | UiEvent::DmStubCreated(_, _)
            | UiEvent::ConversationAdded(_, _) | UiEvent::ConversationListUpdated
            | UiEvent::FetchSingleConversation(_) | UiEvent::SingleConversationFetched(_)
            | UiEvent::FetchConversationMessages(_, _) | UiEvent::FetchedMessages(_, _) => {
                ConversationHandler::handle(state, event);
            }

            // Message events
            UiEvent::MessageSendFailed(_) => {
                MessageHandler::handle(state, event);
            }

            // Data loading events
            UiEvent::ConversationsLoaded(conversations) => {
                debug!("Loaded {} conversations", conversations.len());
                state.conversations = Some(conversations);
                state.request_conversations_refresh = false;
            }
            UiEvent::AllMessagesLoaded(all_messages) => {
                debug!("Loaded messages for {} conversations", all_messages.len());
                state.conversation_messages = all_messages;
            }
            UiEvent::RefreshedMsgs(messages) => {
                debug!("Refreshed {} messages for current conversation", messages.len());
                state.messages = messages;
            }
            UiEvent::InitialLoadComplete => {
                debug!("Initial load completed");
                state.is_loading = false;
                state.is_initial_load_complete = true;
            }
            UiEvent::LoadingProgress(progress) => {
                debug!("Loading progress: {}", progress);
                // Progress can be shown in UI if needed
            }

            // General events
            UiEvent::InviteCreated(token) => {
                debug!("Invite created: {}", token);
                state.last_created_invite = Some(token);
            }
            UiEvent::Info(message) => {
                debug!("Info message: {}", message);
                helpers::add_system_message(state, message);
            }
            UiEvent::Error(error) => {
                warn!("Error occurred: {}", error);
                helpers::add_system_message(state, format!("Errore: {}", error));
            }
            UiEvent::SingleConversationLoaded(conversation) => {
                debug!("Single conversation loaded: {} ({})", conversation.id, conversation.title);
                // This might be used for specific conversation updates
            }
        }
    }
}