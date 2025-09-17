// events/mod.rs - Event dispatcher unificato

use crate::models::UiEvent;
use crate::state::core::AppState;

pub mod conversation_handler;
pub mod websocket_handler;
pub mod data_handler;
pub mod message_handler;
pub mod helpers;
mod auth_handler;

use conversation_handler::ConversationHandler;
use websocket_handler::WebSocketHandler;
use data_handler::DataHandler;
use message_handler::MessageHandler;

pub struct EventDispatcher;

impl EventDispatcher {
    pub fn handle_event(state: &mut AppState, event: UiEvent) {
        match &event {
            // Auth events - gestiti direttamente qui
            UiEvent::LoginStarted => {
                state.login_state = crate::models::LoginState::LoggingIn;
            }
            UiEvent::RegisterStarted => {
                state.login_state = crate::models::LoginState::Registering;
            }
            UiEvent::Logged(token, user_id) => {
                state.token = Some(token.clone());
                state.user_id = Some(*user_id);
                state.login_state = crate::models::LoginState::LoggedIn;
                state.page = crate::models::Page::Conversations;
                
                helpers::add_system_message(state, "Login effettuato con successo!".into());
            }
            UiEvent::LoggedOut => {
                // Cleanup completo dello stato
                state.token = None;
                state.user_id = None;
                state.login_state = crate::models::LoginState::Idle;
                state.page = crate::models::Page::Auth;
                state.cid = None;
                state.conv_title.clear();
                state.messages.clear();
                state.conversations = None;
                state.conversation_messages.clear();
                state.dm_stubs.clear();
                state.is_initial_load_complete = false;
                state.is_loading = false;

                // Disconnetti WebSocket
                if let Some(ctrl) = state.ws_ctrl.take() {
                    let _ = ctrl.shutdown.send(());
                }
                state.ws_status = crate::models::WsStatus::Disconnected;

                helpers::add_system_message(state, "Logout effettuato".into());
            }
            UiEvent::InviteCreated(token) => {
                state.last_created_invite = Some(token.clone());
                helpers::add_system_message(state, "Token di invito generato!".into());
            }

            // WebSocket events
            UiEvent::WsConnected | UiEvent::WsDisconnected | UiEvent::WsError(_)
            | UiEvent::WsControlReady(_) | UiEvent::WsIncoming(_) => {
                WebSocketHandler::handle(state, event);
            }

            // NUOVO: Conversation events unificati
            UiEvent::Opened(_) | UiEvent::ConversationCreated(_) | UiEvent::DmStubCreated(_, _)
            | UiEvent::ConversationListUpdated
            | UiEvent::TriggerConversationFetch(_, _) // NUOVO: evento unificato
            | UiEvent::ConversationCompleteFetched(_, _) => {
                ConversationHandler::handle(state, event);
            }

            // Data loading events
            UiEvent::ConversationsLoaded(_) | UiEvent::AllMessagesLoaded(_)
            | UiEvent::RefreshedMsgs(_) | UiEvent::SingleConversationLoaded(_)
            | UiEvent::InitialLoadComplete | UiEvent::LoadingProgress(_) => {
                DataHandler::handle(state, event);
            }

            // Message events
            UiEvent::MessageSendFailed(_) => {
                MessageHandler::handle(state, event);
            }

            // Generic info/error events - gestiti direttamente
            UiEvent::Info(msg) => {
                helpers::add_system_message(state, format!("Info: {}", msg));
            }
            UiEvent::Error(msg) => {
                helpers::add_system_message(state, format!("Errore: {}", msg));
            }
        }
    }
}