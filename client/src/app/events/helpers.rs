// events/helpers.rs - Utilities condivise
use crate::models::MessageDto;
use uuid::Uuid;
use tracing::{info, warn};

pub fn add_system_message(state: &mut crate::state::core::AppState, content: String) {
    let msg = MessageDto::system_message(content);
    state.messages.push(msg);

    let system_message_count = state
        .messages
        .iter()
        .filter(|m| m.author_id == Uuid::nil())
        .count();

    if system_message_count > 50 {
        let mut non_system: Vec<_> = state
            .messages
            .iter()
            .filter(|m| m.author_id != Uuid::nil())
            .cloned()
            .collect();

        let recent_system: Vec<_> = state
            .messages
            .iter()
            .filter(|m| m.author_id == Uuid::nil())
            .rev()
            .take(20)
            .cloned()
            .collect();

        non_system.extend(recent_system);
        non_system.sort_by_key(|m| m.created_at);
        state.messages = non_system;
    }
}

pub fn validate_incoming_message(msg: &MessageDto) -> bool {
    if msg.conversation_id == Uuid::nil() {
        warn!("Received message with nil conversation_id, ignoring: {:?}", msg.id);
        return false;
    }

    if msg.content.trim().is_empty() {
        warn!("Received message with empty content, ignoring: {:?}", msg.id);
        return false;
    }

    if msg.author_id == Uuid::nil() && msg.author_username != "system" {
        warn!("Received message with nil author_id (non-system), ignoring: {:?}", msg.id);
        return false;
    }

    true
}

pub fn deduplicate_messages(messages: &mut Vec<MessageDto>) {
    messages.sort_by_key(|m| m.id);
    messages.dedup_by_key(|m| m.id);
    messages.sort_by_key(|m| m.created_at);
}

pub fn cleanup_old_conversations(state: &mut crate::state::core::AppState) {
    if let Some(ref conversations) = state.conversations {
        let valid_ids: std::collections::HashSet<_> =
            conversations.iter().map(|c| c.id).collect();

        let old_count = state.conversation_messages.len();
        state
            .conversation_messages
            .retain(|cid, _| valid_ids.contains(cid));
        let removed = old_count - state.conversation_messages.len();

        if removed > 0 {
            info!("Cleaned up {} old conversation caches", removed);
        }
    }
}