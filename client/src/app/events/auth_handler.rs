// events/auth_handler.rs
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
            UiEvent::Logged(token, user_id, initial_sequence) => {
                Self::handle_login_success(state, token, user_id, initial_sequence);
            }
            UiEvent::LoggedOut => {
                Self::handle_logout(state);
            }
            _ => unreachable!("Invalid auth event"),
        }
    }

    fn handle_login_success(
        state: &mut crate::state::core::AppState,
        token: String,
        user_id: Uuid,
        initial_sequence: u64
    ) {
        state.token = Some(token.clone());
        state.user_id = Some(user_id);
        state.login_state = LoginState::LoggedIn;
        state.page = Page::Conversations;

        // Inizializza il sistema di sequenze con la sequenza fornita dal server
        state.last_sequence_received = initial_sequence;
        state.last_sequence_confirmed = initial_sequence;  // IMPORTANTE: entrambi allo stesso valore iniziale
        state.sequence_stats = Default::default();

        let success_msg = if initial_sequence > 0 {
            format!("Login effettuato con successo! Sequenza iniziale: #{}", initial_sequence)
        } else {
            "Login effettuato con successo".into()
        };

        crate::app::events::helpers::add_system_message(state, success_msg);

        info!("User {} logged in successfully with initial sequence: {}", user_id, initial_sequence);
    }

    fn handle_logout(state: &mut crate::state::core::AppState) {
        info!("User logout");

        // Shutdown WebSocket gracefully
        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        // Complete state reset
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

        // Clear group management state
        state.group_name.clear();
        state.dm_user_username_input.clear();
        state.invite_conversation_id.clear();
        state.last_created_invite = None;
        state.last_invite_token = None;

        // Clear conversation cache
        state.conversation_messages.clear();
        state.is_initial_load_complete = false;
        state.is_loading = false;
        state.dm_stubs.clear();

        // Reset del sistema di sequenze
        state.last_sequence_received = 0;
        state.last_sequence_confirmed = 0;  // Reset anche questo
        state.last_ping_time = std::time::Instant::now();
        state.missed_pings = 0;
        state.is_recovering_sequence = false;
        state.sequence_stats = Default::default();

        crate::app::events::helpers::add_system_message(state, "Logout effettuato".into());
    }
}