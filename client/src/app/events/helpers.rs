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