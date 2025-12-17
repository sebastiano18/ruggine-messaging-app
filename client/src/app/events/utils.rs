use crate::state::core::AppState;
use tracing::debug;
use uuid::Uuid;

pub fn move_conversation_to_top(state: &mut AppState, conversation_id: Uuid) {
    if let Some(ref mut conversations) = state.conversations {
        if let Some(pos) = conversations.iter().position(|c| c.id == conversation_id) {
            if pos > 0 {
                let conversation = conversations.remove(pos);
                conversations.insert(0, conversation);

                debug!("Moved conversation {} to top", conversation_id);
            }
        }
    }
}
