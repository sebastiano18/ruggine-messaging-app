use std::time::Instant;
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

        // SEMPRE genera nuovo session_id ad ogni login
        // Questo previene riutilizzo di vecchi session_id che causano doppie connessioni
        state.current_session_id = Some(uuid::Uuid::new_v4());
        info!("Generated new session_id: {:?}", state.current_session_id);

        state.user_sequence_confirmed = last_sequence;
        state.user_sequence_received = last_sequence;
        state.conversation_sequences.clear();
        state.conversation_sequences_confirmed.clear();

        state.login_state = LoginState::LoggedIn;
        state.page = Page::Conversations;
        state.sequence_stats = Default::default();
        state.request_ws_reconnect = true;

        // Pulisci eventuali messaggi di errore dalla schermata di login
        state.clear_auth_message();
    }

    pub fn handle_logged_out(state: &mut AppState) {
        info!("User logged out");

        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        state.token = None;
        state.user_id = None;
        // NON pulire username/password - l'utente potrebbe voler riprovare
        state.page = Page::Auth;
        state.cid = None;

        // === CHIUSURA TUTTI I POPUP/MODAL ===
        state.show_account_modal = false;                    // Modal impostazioni account
        state.show_create_group_modal = false;               // Modal creazione gruppo
        state.show_invite_popup = false;                     // ← Popup invito membri gruppo
        state.show_group_info_popup = false;                 // ← Popup info gruppo
        state.confirm_delete_account = false;                // Modal conferma eliminazione account
        state.pending_deletion = None;                       // Popup conferma eliminazione conversazione
        state.pending_message_deletion = None;               // ← Conferma eliminazione messaggio
        state.pending_member_kick = None;                    // ← Conferma kick membro
        state.pending_user_check = None;                     // Reset verifica utente in corso
        state.user_check_request_id = None;
        state.user_check_timestamp = None;

        // Reset stato interno dei popup
        state.create_group_popup.reset();                    // Reset popup creazione gruppo
        state.invite_popup.reset();                          // ← Reset popup invito membri

        // Pulizia dati
        state.messages.clear();
        state.conversations = None;
        state.conversation_messages.clear();
        state.conversation_unread_counts.clear();
        state.members_list.clear();                          // ← Pulisci lista membri gruppi
        state.is_loading_members = false;

        // Reset pagination state
        state.is_loading_more_conversations = false;
        state.has_more_conversations = true;
        state.next_cursor = None;
        state.fetching_conversations.clear();

        state.login_state = LoginState::Idle;
        state.ws_status = WsStatus::Disconnected;
        state.dm_stubs.clear();
        state.group_stubs.clear();
        state.pending_confirmations.clear();

        // Pulizia campi UI
        state.group_name.clear();
        state.dm_username.clear();
        state.dm_user_username_input.clear();
        state.conv_title.clear();
        state.input.clear();
        state.invite_conversation_id.clear();
        state.last_invite_token = None;
        state.last_created_invite = None;

        // Reset sequenze
        state.user_sequence_confirmed = 0;
        state.user_sequence_received = 0;
        state.conversation_sequences.clear();
        state.conversation_sequences_confirmed.clear();
        state.sequence_stats = Default::default();

        // Reset buffer riordinamento
        state.message_reorder_buffer.clear();
        state.user_event_reorder_buffer.clear();

        // Reset stato caricamento
        state.is_loading = false;
        state.is_loading_more = false;
        state.is_initial_load_complete = false;
        state.has_more_messages.clear();
        state.is_recovering_user_events = false;
        state.is_recovering_messages.clear();
        state.pending_resume_requests = 0;

        // Reset ping/pong
        state.missed_pings = 0;
        state.last_ping_time = Instant::now();

        // CRITICAL: Reset session_id per forzare nuovo session al prossimo login
        state.current_session_id = None;
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

        // Verifica che ci sia una connessione WebSocket attiva
        if state.ws_ctrl.is_none() {
            let _ = state.ui_tx.send(UiEvent::Error(ErrorType::Auth(
                "WebSocket non connesso".into()
            )));
            return;
        };

        if let Err(e) = state.ui_to_net_tx.try_send(Outgoing::DeleteUser) {
            let _ = state.ui_tx.send(UiEvent::Error(ErrorType::Auth(
                format!("Impossibile inviare richiesta: {}", e)
            )));
            return;
        }
    }
}