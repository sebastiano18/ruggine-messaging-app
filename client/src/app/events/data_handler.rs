// events/data_handler.rs - Gestione caricamento dati
use crate::models::{UiEvent, ConversationDto, MessageDto, Page};
use tracing::{info, warn, debug};
use std::collections::HashMap;
use uuid::Uuid;

pub struct DataHandler;

impl DataHandler {
    pub fn handle(state: &mut crate::state::core::AppState, event: UiEvent) {
        match event {
            UiEvent::ConversationsLoaded(conversations) => {
                Self::handle_conversations_loaded(state, conversations);
            }
            UiEvent::AllMessagesLoaded(messages_map) => {
                Self::handle_all_messages_loaded(state, messages_map);
            }
            UiEvent::RefreshedMsgs(list) => {
                Self::handle_messages_refreshed(state, list);
            }
            UiEvent::InitialLoadComplete => {
                state.is_initial_load_complete = true;
                state.is_loading = false;
                crate::app::events::helpers::add_system_message(state, "Tutti i dati caricati!".into());
                info!("Initial data load completed");
            }
            UiEvent::LoadingProgress(progress) => {
                crate::app::events::helpers::add_system_message(state, format!("{}", progress));
            }
            UiEvent::SingleConversationLoaded(conversation) => {
                Self::handle_single_conversation_loaded(state, conversation);
            }
            _ => unreachable!("Invalid data event"),
        }
    }

    fn handle_conversations_loaded(
        state: &mut crate::state::core::AppState,
        server_conversations: Vec<ConversationDto>,
    ) {
        let old_count = state.conversations.as_ref().map(|c| c.len()).unwrap_or(0);
        let server_count = server_conversations.len(); // Salva prima del move

        // STRATEGIA SEMPLICE: Server vince sempre, aggiungi solo DM stub attivi
        let mut final_conversations = server_conversations;

        // Prima raccogli le conversazioni locali da preservare
        let stubs_to_preserve: Vec<ConversationDto> = if let Some(ref local_conversations) = state.conversations {
            local_conversations
                .iter()
                .filter(|local_conv| {
                    state.dm_stubs.contains_key(&local_conv.id) &&
                        !final_conversations.iter().any(|c| c.id == local_conv.id)
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        // Aggiungi gli stub preservati
        for stub in stubs_to_preserve {
            info!("Preserving active DM stub: {} ({})", stub.id, stub.title);
            final_conversations.push(stub);
        }

        // Ordina per data di creazione
        final_conversations.sort_by_key(|c| c.created_at);

        let new_count = final_conversations.len();
        let preserved_count = new_count - server_count; // Usa server_count invece di server_conversations.len()

        // Aggiorna lo stato
        state.conversations = Some(final_conversations.clone());

        info!("Updated conversations: {} (was {}) - {} from server, {} stubs preserved",
              new_count, old_count, server_count, preserved_count);

        crate::app::events::helpers::add_system_message(state, format!("{} conversazioni disponibili", new_count));

        // Verifica se la conversazione corrente esiste ancora
        if let Some(current_cid) = state.cid {
            if !final_conversations.iter().any(|c| c.id == current_cid) {
                warn!("Current conversation {} no longer exists, clearing selection", current_cid);
                state.cid = None;
                state.conv_title.clear();
                state.messages.clear();
                state.page = Page::Conversations;
                crate::app::events::helpers::add_system_message(
                    state,
                    "La conversazione corrente non esiste più".into(),
                );
            }
        }

        crate::app::events::helpers::cleanup_old_conversations(state);
    }

    fn handle_messages_refreshed(state: &mut crate::state::core::AppState, mut list: Vec<MessageDto>) {
        list.retain(|msg| crate::app::events::helpers::validate_incoming_message(msg));
        crate::app::events::helpers::deduplicate_messages(&mut list);
        list.sort_by_key(|m| m.created_at);

        debug!("Refreshing messages: {} valid messages", list.len());

        state.messages = list.clone();

        if let Some(cid) = state.cid {
            state.conversation_messages.insert(cid, list);
            debug!("Refreshed {} messages for conversation {}", state.messages.len(), cid);
        }
    }

    fn handle_single_conversation_loaded(
        state: &mut crate::state::core::AppState,
        conversation: ConversationDto,
    ) {
        // Controlla se esiste già
        let already_exists = state.conversations
            .as_ref()
            .map(|convs| convs.iter().any(|c| c.id == conversation.id))
            .unwrap_or(false);

        if !already_exists {
            if let Some(ref mut conversations) = state.conversations {
                conversations.push(conversation.clone());
                info!("Added new conversation '{}' to existing list", conversation.title);
                crate::app::events::helpers::add_system_message(
                    state,
                    format!("Nuova conversazione: {}", conversation.title),
                );
            } else {
                state.conversations = Some(vec![conversation.clone()]);
                info!("Initialized conversations list with new conversation");
            }
        } else {
            debug!("Conversation {} already exists, not adding duplicate", conversation.id);
        }
    }

    fn handle_all_messages_loaded(
        state: &mut crate::state::core::AppState,
        messages_map: HashMap<Uuid, Vec<MessageDto>>,
    ) {
        let total_messages: usize = messages_map.values().map(|v| v.len()).sum();
        info!("Loaded {} total messages across {} conversations", total_messages, messages_map.len());

        let mut cleaned_map = HashMap::new();
        for (conv_id, mut messages) in messages_map {
            messages.retain(|msg| crate::app::events::helpers::validate_incoming_message(msg));
            crate::app::events::helpers::deduplicate_messages(&mut messages);
            messages.sort_by_key(|m| m.created_at);

            if !messages.is_empty() {
                cleaned_map.insert(conv_id, messages);
            }
        }

        // Unisci con messaggi esistenti (preserva messaggi già in cache)
        for (conv_id, existing_messages) in &state.conversation_messages {
            if !cleaned_map.contains_key(conv_id) {
                cleaned_map.insert(*conv_id, existing_messages.clone());
            }
        }

        state.conversation_messages = cleaned_map;

        if let Some(cid) = state.cid {
            if let Some(msgs) = state.conversation_messages.get(&cid).cloned() {
                state.messages = msgs;
                debug!("Loaded {} messages for current conversation {}", state.messages.len(), cid);
            }
        }
    }
}