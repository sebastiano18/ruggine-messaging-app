// events/data_handler.rs - Gestione caricamento dati
// events/data_handler.rs - Gestione caricamento dati
use crate::models::{UiEvent, ConversationDto, MessageDto, Page};
use tracing::{info, warn, debug, error};
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
            UiEvent::ConversationDeleted(cid) => {
                // Rimuovi dalla lista conversazioni
                if let Some(ref mut list) = state.conversations {
                    list.retain(|c| c.id != cid);
                }
                // Pulisci cache messaggi
                state.conversation_messages.remove(&cid);

                // Se era la conversazione corrente, resetta vista
                if state.cid == Some(cid) {
                    state.cid = None;
                    state.conv_title.clear();
                    state.messages.clear();
                    state.page = Page::Conversations;
                }

                crate::app::events::helpers::add_system_message(
                    state,
                    format!("Conversazione {} eliminata", cid),
                );
            }
            _ => unreachable!("Invalid data event"),
        }
    }

    fn handle_conversations_loaded(
        state: &mut crate::state::core::AppState,
        server_conversations: Vec<ConversationDto>,
    ) {
        let old_count = state.conversations.as_ref().map(|c| c.len()).unwrap_or(0);
        let server_count = server_conversations.len();

        info!("Processing {} conversations from server (was {})", server_count, old_count);

        // DEDUPLICAZIONE ROBUSTA: Usa HashMap per evitare duplicati
        let mut conversations_map: std::collections::HashMap<Uuid, ConversationDto> =
            std::collections::HashMap::new();

        // 1. Aggiungi tutte le conversazioni dal server (priorità alta)
        for conv in server_conversations {
            debug!("Adding server conversation: {} - {}", conv.id, conv.title);
            conversations_map.insert(conv.id, conv);
        }

        // 2. Preserva SOLO gli stub DM attivi che non sono già nel server
        if let Some(ref local_conversations) = state.conversations {
            for local_conv in local_conversations {
                // Preserva solo se:
                // a) È uno stub DM attivo
                // b) NON esiste già nel server
                if state.dm_stubs.contains_key(&local_conv.id) &&
                    !conversations_map.contains_key(&local_conv.id) {
                    info!("Preserving active DM stub: {} ({})", local_conv.id, local_conv.title);
                    conversations_map.insert(local_conv.id, local_conv.clone());
                }
            }
        }

        // 3. Converti HashMap in Vec ordinato
        let mut final_conversations: Vec<ConversationDto> = conversations_map.into_values().collect();
        final_conversations.sort_by_key(|c| c.created_at);

        let new_count = final_conversations.len();
        let preserved_count = new_count - server_count;

        // 4. VERIFICA DUPLICATI prima di assegnare
        let mut seen_ids = std::collections::HashSet::new();
        let mut duplicates_found = Vec::new();

        for conv in &final_conversations {
            if !seen_ids.insert(conv.id) {
                duplicates_found.push(conv.id);
            }
        }

        if !duplicates_found.is_empty() {
            error!("DUPLICATES DETECTED before assignment: {:?}", duplicates_found);
            // Deduplicazione forzata
            let mut deduped = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for conv in final_conversations {
                if seen.insert(conv.id) {
                    deduped.push(conv);
                }
            }
            final_conversations = deduped;
        }

        // 5. Aggiorna lo stato
        state.conversations = Some(final_conversations.clone());

        info!("Updated conversations: {} (was {}) - {} from server, {} stubs preserved",
              final_conversations.len(), old_count, server_count, preserved_count);

        crate::app::events::helpers::add_system_message(state, format!("{} conversazioni disponibili", final_conversations.len()));

        // 6. Verifica se la conversazione corrente esiste ancora
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

        // 7. VERIFICA FINALE duplicati
        Self::debug_check_duplicates(state);
    }

    // Nuovo metodo helper per debug
    fn debug_check_duplicates(state: &crate::state::core::AppState) {
        if let Some(ref conversations) = state.conversations {
            let mut seen_ids = std::collections::HashSet::new();
            let mut duplicates = Vec::new();

            for conv in conversations {
                if !seen_ids.insert(conv.id) {
                    duplicates.push(format!("{} ({})", conv.id, conv.title));
                }
            }

            if !duplicates.is_empty() {
                error!("POST-ASSIGNMENT DUPLICATES DETECTED: {:?}", duplicates);
                let _ = state.ui_tx.send(UiEvent::Error(
                    format!("Rilevati duplicati conversazioni: {}", duplicates.len())
                ));
            } else {
                debug!("No duplicates found in final conversation list");
            }
        }
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