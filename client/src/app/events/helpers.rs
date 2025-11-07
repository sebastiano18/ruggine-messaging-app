// events/helpers.rs - Helper functions for event handling

use crate::models::MessageDto;
use crate::state::core::AppState;
use tracing::{debug, warn};
use uuid::Uuid;

/// Aggiunge un messaggio di sistema alla conversazione corrente
pub fn add_system_message(state: &mut AppState, content: String) {
    let system_msg = MessageDto::system_message(content);
    state.messages.push(system_msg.clone());

    // Se c'è una conversazione corrente, aggiungi anche alla cache
    if let Some(cid) = state.cid {
        if let Some(messages) = state.conversation_messages.get_mut(&cid) {
            messages.push(system_msg);
        }
    }

    debug!("Added system message to UI");
}

/// Aggiunge un messaggio di sistema a una conversazione specifica
pub fn add_system_message_to_conversation(state: &mut AppState, conversation_id: Uuid, content: String) {
    let mut system_msg = MessageDto::system_message(content);
    system_msg.conversation_id = conversation_id;
    
    // Aggiungi alla cache della conversazione specifica
    if let Some(messages) = state.conversation_messages.get_mut(&conversation_id) {
        messages.push(system_msg.clone());
        debug!("Added system message to conversation {} cache", conversation_id);
    } else {
        // Se la conversazione non è in cache, creala
        state.conversation_messages.insert(conversation_id, vec![system_msg.clone()]);
        debug!("Created cache and added system message for conversation {}", conversation_id);
    }
    
    // Se è la conversazione corrente, aggiungi anche a state.messages
    if state.cid == Some(conversation_id) {
        state.messages.push(system_msg);
        debug!("Added system message to current UI");
    }
}

/// Valida un messaggio in arrivo dal WebSocket
pub fn validate_incoming_message(msg: &MessageDto) -> bool {
    // Controlli di base
    if msg.content.is_empty() {
        warn!("Received message with empty content: {}", msg.id);
        return false;
    }

    if msg.author_username.trim().is_empty() && !msg.is_system_message() {
        warn!("Received message with empty username: {}", msg.id);
        return false;
    }

    if msg.content.len() > 50000 {
        warn!("Received message too long ({} chars): {}", msg.content.len(), msg.id);
        return false;
    }

    true
}

/// Pulisce le conversazioni vecchie dalla cache per evitare memory leaks
pub fn cleanup_old_conversations(state: &mut AppState) {
    const MAX_CACHED_CONVERSATIONS: usize = 50;

    if state.conversation_messages.len() > MAX_CACHED_CONVERSATIONS {
        // Ottieni le conversazioni attive
        let active_conversation_ids: Vec<Uuid> = state
            .conversations
            .as_ref()
            .map(|convs| convs.iter().map(|c| c.id).collect())
            .unwrap_or_default();

        // Rimuovi le conversazioni che non sono più nella lista
        let mut to_remove = Vec::new();
        for &cached_id in state.conversation_messages.keys() {
            if !active_conversation_ids.contains(&cached_id) && Some(cached_id) != state.cid {
                to_remove.push(cached_id);
            }
        }

        for id in to_remove {
            state.conversation_messages.remove(&id);
            debug!("Removed cached messages for inactive conversation: {}", id);
        }
    }
}

/// Trova una conversazione per ID
pub fn find_conversation_by_id(state: &AppState, conversation_id: Uuid) -> Option<&crate::models::ConversationDto> {
    state
        .conversations
        .as_ref()
        .and_then(|convs| convs.iter().find(|c| c.id == conversation_id))
}

/// Conta il numero totale di messaggi nella cache
pub fn count_cached_messages(state: &AppState) -> usize {
    state
        .conversation_messages
        .values()
        .map(|msgs| msgs.len())
        .sum()
}

/// Ottiene statistiche sullo stato dell'app
pub fn get_app_stats(state: &AppState) -> (usize, usize, usize) {
    let conversation_count = state.conversations.as_ref().map_or(0, |c| c.len());
    let cached_conversation_count = state.conversation_messages.len();
    let total_cached_messages = count_cached_messages(state);

    (conversation_count, cached_conversation_count, total_cached_messages)
}

pub fn deduplicate_messages(messages: &mut Vec<MessageDto>) {
    let mut seen_ids = std::collections::HashSet::new();
    messages.retain(|msg| seen_ids.insert(msg.id));
}