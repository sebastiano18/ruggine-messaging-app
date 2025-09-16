// events/mod.rs - Entry point modulare
pub mod auth_handler;
pub mod websocket_handler;
pub mod conversation_handler;
pub mod message_handler;
pub mod data_handler;
pub mod fetch_handler;
pub mod helpers;

use crate::app::events::conversation_handler::ConversationHandler;
use crate::models::UiEvent;

pub struct EventDispatcher;

impl EventDispatcher {
    pub fn handle_event(state: &mut crate::state::core::AppState, event: UiEvent) {
        match event {
            // Authentication events
            UiEvent::LoginStarted
            | UiEvent::RegisterStarted
            | UiEvent::Logged(..)
            | UiEvent::LoggedOut => {
                auth_handler::AuthHandler::handle(state, event);
            }

            // WebSocket events
            UiEvent::WsConnected
            | UiEvent::WsDisconnected
            | UiEvent::WsError(..)
            | UiEvent::WsControlReady(..)
            | UiEvent::WsIncoming(..) => {
                websocket_handler::WebSocketHandler::handle(state, event);
            }

            // Conversation events
            UiEvent::Opened(..)
            | UiEvent::ConversationCreated(..)
            | UiEvent::DmStubCreated(..)
            | UiEvent::ConversationAdded(..)
            | UiEvent::ConversationListUpdated => {
                ConversationHandler::handle(state, event);
            }
            UiEvent::FetchSingleConversation(_) |
            UiEvent::SingleConversationFetched(_) => {
                ConversationHandler::handle(state, event);
            }

            // Message events
            UiEvent::MessageSendFailed(..) => {
                message_handler::MessageHandler::handle(state, event);
            }

            // Data loading events
            UiEvent::ConversationsLoaded(..)
            | UiEvent::AllMessagesLoaded(..)
            | UiEvent::RefreshedMsgs(..)
            | UiEvent::InitialLoadComplete
            | UiEvent::LoadingProgress(..)
            | UiEvent::SingleConversationLoaded(..) => {
                data_handler::DataHandler::handle(state, event);
            }

            // Fetch events
            UiEvent::FetchConversationMessages(..)
            | UiEvent::FetchedMessages(..) => {
                fetch_handler::FetchHandler::handle(state, event);
            }

            // General events
            UiEvent::Info(..) | UiEvent::Error(..) | UiEvent::InviteCreated(..) => {
                match event {
                    UiEvent::Info(s) => {
                        helpers::add_system_message(state, s);
                    }
                    UiEvent::Error(s) => {
                        helpers::add_system_message(state, format!("{}", s));
                        state.login_state = crate::models::LoginState::Idle;
                        state.is_loading = false;
                    }
                    UiEvent::InviteCreated(token) => {
                        state.last_created_invite = Some(token);
                        helpers::add_system_message(state, "Invito creato con successo".into());
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}