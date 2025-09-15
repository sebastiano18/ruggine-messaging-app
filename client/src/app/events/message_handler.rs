// events/message_handler.rs - Gestione messaggi
use crate::models::UiEvent;
use tracing::{warn, debug};
use uuid::Uuid;

pub struct MessageHandler;

impl MessageHandler {
    pub fn handle(state: &mut crate::state::core::AppState, event: UiEvent) {
        match event {
            UiEvent::MessageSendFailed(failed_message_id) => {
                Self::handle_message_send_failed(state, failed_message_id);
            }
            _ => unreachable!("Invalid message event"),
        }
    }

    fn handle_message_send_failed(state: &mut crate::state::core::AppState, failed_message_id: Uuid) {
        warn!("Message send failed: {}", failed_message_id);

        let old_ui_len = state.messages.len();
        state.messages.retain(|msg| msg.id != failed_message_id);
        let ui_removed = old_ui_len - state.messages.len();

        if let Some(cid) = state.cid {
            if let Some(messages) = state.conversation_messages.get_mut(&cid) {
                let old_cache_len = messages.len();
                messages.retain(|msg| msg.id != failed_message_id);
                let cache_removed = old_cache_len - messages.len();
                debug!("Removed failed message - UI: {}, Cache: {}", ui_removed, cache_removed);
            }
        }

        crate::app::events::helpers::add_system_message(state, "Invio messaggio fallito".into());
    }
}