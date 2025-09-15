// events/auth_handler.rs - Gestione autenticazione
use crate::models::{UiEvent, LoginState, Page, WsStatus, MessageDto};
use tracing::info;
use uuid::Uuid;


pub struct AuthHandler;

impl AuthHandler {
    pub fn handle(state: &mut crate::state::core::AppState, event: UiEvent) {
        match event {
            UiEvent::LoginStarted => {
                state.login_state = LoginState::LoggingIn;
                crate::app::events::helpers::add_system_message(state, "Effettuando login...".into());
            }
            UiEvent::RegisterStarted => {
                state.login_state = LoginState::Registering;
                crate::app::events::helpers::add_system_message(state, "Registrando utente...".into());
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

    fn handle_login_success(state: &mut crate::state::core::AppState, token: String, user_id: Uuid) {
        state.token = Some(token.clone());
        state.user_id = Some(user_id);
        state.login_state = LoginState::LoggedIn;
        crate::app::events::helpers::add_system_message(state, "Login effettuato con successo".into());
        state.page = Page::Conversations;

        info!("User {} logged in successfully", user_id);
        state.preload_all_data(token);
    }

    fn handle_logout(state: &mut crate::state::core::AppState) {
        info!("User logout");

        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        // Reset completo stato
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

        state.messages.push(MessageDto::system_message("Logout effettuato".into()));
    }
}