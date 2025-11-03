// events/auth_handler.rs
use crate::models::*;
use crate::state::core::AppState;
use reqwest::StatusCode;
use tracing::info;

pub struct AuthHandler;

impl AuthHandler {
    pub fn handle_login_started(state: &mut AppState) {
        state.login_state = LoginState::LoggingIn;
        info!("Login started");
    }

    pub fn handle_register_started(state: &mut AppState) {
        state.login_state = LoginState::Registering;
        info!("Registration started");
    }

    pub fn handle_logged(state: &mut AppState, token: String, user_id: uuid::Uuid, last_sequence: u64) {
        info!(
            "User logged in - user_id: {}, initial sequence: {}",
            user_id, last_sequence
        );

        state.token = Some(token.clone());
        state.user_id = Some(user_id);

        state.user_sequence_confirmed = last_sequence;
        state.user_sequence_received = last_sequence;
        state.conversation_sequences.clear();
        state.conversation_sequences_confirmed.clear();

        state.login_state = LoginState::LoggedIn;
        state.page = Page::Conversations;
        state.sequence_stats = Default::default();
        state.request_ws_reconnect = true;
    }

    pub fn handle_logged_out(state: &mut AppState) {
        info!("User logged out");

        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        state.token = None;
        state.user_id = None;
        state.page = Page::Auth;
        state.cid = None;
        state.messages.clear();
        state.conversations = None;
        state.conversation_messages.clear();
        state.login_state = LoginState::Idle;
        state.ws_status = WsStatus::Disconnected;
        state.dm_stubs.clear();

        state.user_sequence_confirmed = 0;
        state.user_sequence_received = 0;
        state.conversation_sequences.clear();
        state.conversation_sequences_confirmed.clear();
        state.sequence_stats = Default::default();
    }

    pub fn handle_delete_account_start(state: &mut AppState) {
        state.confirm_delete_account = true;
    }

    pub fn handle_delete_account_cancel(state: &mut AppState) {
        state.confirm_delete_account = false;
        let _ = state.ui_tx.send(UiEvent::Info("Eliminazione annullata".into()));
    }

    pub fn handle_delete_account_confirm(state: &mut AppState) {
        state.confirm_delete_account = false;

        let Some(token) = state.token.clone() else {
            let _ = state.ui_tx.send(UiEvent::LoggedOut);
            return;
        };

        let base = state.base.clone();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            match crate::api::auth::delete_account(&base, &token).await {
                Ok(()) => {
                    let _ = tx.send(UiEvent::LoggedOut);
                }
                Err(e) => {
                    if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
                        if req_err.status() == Some(StatusCode::UNAUTHORIZED) {
                            let _ = tx.send(UiEvent::LoggedOut);
                            return;
                        }
                    }
                    let _ = tx.send(UiEvent::Error(format!("Eliminazione account fallita: {e}")));
                }
            }
        });
    }
}