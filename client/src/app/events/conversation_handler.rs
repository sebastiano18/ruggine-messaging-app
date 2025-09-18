use crate::models::{UiEvent, Page, MessageDto, ConversationDto};
use tracing::{debug, info, error, warn};
use uuid::Uuid;

pub struct ConversationHandler;

impl ConversationHandler {
    pub fn handle(state: &mut crate::state::core::AppState, event: UiEvent) {
        match event {
            // NUOVO: Handler unificato per tutti i trigger di fetch
            UiEvent::TriggerConversationFetch(conversation_id, reason) => {
                Self::handle_trigger_conversation_fetch(state, conversation_id, reason);
            }
            UiEvent::ConversationCompleteFetched(conversation, messages) => {
                Self::handle_conversation_complete_fetched(state, conversation, messages);
            }

            // Eventi esistenti
            UiEvent::Opened(cid) => {
                Self::handle_conversation_opened(state, cid);
            }
            UiEvent::ConversationCreated(conversation_id) => {
                Self::handle_conversation_created(state, conversation_id);
            }
            UiEvent::DmStubCreated(conversation_id, other_username) => {
                Self::handle_dm_stub_created(state, conversation_id, other_username);
            }
            UiEvent::ConversationListUpdated => {
                Self::handle_conversation_list_updated(state);
            }

            _ => unreachable!("Invalid conversation event"),
        }
    }

    // NUOVO: Handler unificato per qualsiasi trigger di fetch conversazione
    fn handle_trigger_conversation_fetch(
        state: &mut crate::state::core::AppState,
        conversation_id: Uuid,
        reason: String
    ) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            info!("Triggering unified conversation fetch for {} (reason: {})", conversation_id, reason);

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversation_with_messages(&base, &token, conversation_id).await {
                    Ok(complete_data) => {
                        debug!("Successfully fetched conversation {} with {} messages (reason: {})",
                               complete_data.conversation.id, complete_data.messages.len(), reason);
                        let _ = tx.send(UiEvent::ConversationCompleteFetched(
                            complete_data.conversation,
                            complete_data.messages
                        ));
                    }
                    Err(e) => {
                        error!("Failed to fetch complete conversation {} (reason: {}): {}", conversation_id, reason, e);
                        let _ = tx.send(UiEvent::Error(format!(
                            "Errore caricamento conversazione completa {}: {}",
                            conversation_id, e
                        )));
                    }
                }
            });
        } else {
            warn!("Cannot fetch conversation: no token available");
        }
    }

    // Gestione conversazione completa ricevuta
    fn handle_conversation_complete_fetched(
        state: &mut crate::state::core::AppState,
        conversation: ConversationDto,
        messages: Vec<MessageDto>
    ) {
        info!("Processing complete conversation: {} ({}) with {} messages",
              conversation.id, conversation.title, messages.len());

        // 1. Aggiorna la lista delle conversazioni
        if let Some(ref mut conversations) = state.conversations {
            if let Some(existing_pos) = conversations.iter().position(|c| c.id == conversation.id) {
                debug!("Updating existing conversation: {}", conversation.id);
                conversations[existing_pos] = conversation.clone();
            } else {
                debug!("Adding new conversation to list: {}", conversation.id);
                conversations.push(conversation.clone());
                conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                crate::app::events::helpers::add_system_message(
                    state,
                    format!("Nuova conversazione aggiunta: {}", conversation.title)
                );
            }
        } else {
            debug!("Creating new conversation list with: {}", conversation.id);
            state.conversations = Some(vec![conversation.clone()]);
        }

        // 2. Aggiorna cache messaggi
        let mut validated_messages = messages;
        let validated_messages_len = validated_messages.len();
        validated_messages.retain(|msg| crate::app::events::helpers::validate_incoming_message(msg));
        validated_messages.sort_by_key(|m| m.created_at);

        state.conversation_messages.insert(conversation.id, validated_messages.clone());

        // 3. Se è la conversazione corrente, aggiorna anche la UI
        if Some(conversation.id) == state.cid {
            info!("Updating UI with {} messages for current conversation {}", validated_messages.len(), conversation.id);
            state.messages = validated_messages;
        } else {
            debug!("Messages cached for conversation {} (not current)", conversation.id);
        }

        // 4. Notifica successo
        crate::app::events::helpers::add_system_message(
            state,
            format!("Conversazione '{}' caricata con {} messaggi", conversation.title, validated_messages_len)
        );

        debug!("Complete conversation fetch completed for: {}", conversation.id);
    }

    fn handle_conversation_opened(state: &mut crate::state::core::AppState, cid: Uuid) {
        debug!("Opening conversation: {}", cid);

        state.cid = Some(cid);
        state.page = Page::Chat;

        if let Some(ref conversations) = state.conversations {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                state.conv_title = conv.title.clone();
                debug!("Conversation title set to: {}", conv.title);
            }
        }

        // Controlla se abbiamo già i messaggi in cache
        if let Some(cached_messages) = state.conversation_messages.get(&cid) {
            state.messages = cached_messages.clone();
            debug!("Loaded {} messages from cache for conversation {}", cached_messages.len(), cid);
        } else {
            // Nessuna cache, carica i messaggi ora
            debug!("No cached messages, loading from server for conversation {}", cid);
            state.messages = vec![];

            // Aggiungi messaggio di caricamento
            crate::app::events::helpers::add_system_message(state, "Caricamento messaggi...".into());

            // Carica i messaggi per questa specifica conversazione
            if let Some(ref token) = state.token {
                let base = state.base.clone();
                let token = token.clone();
                let tx = state.ui_tx.clone();

                state.rt.spawn(async move {
                    match crate::api::conversation::get_conversation_with_messages(&base, &token, cid).await {
                        Ok(conv_with_msgs) => {
                            debug!("Loaded {} messages for conversation {}",
                               conv_with_msgs.messages.len(), cid);
                            let _ = tx.send(UiEvent::RefreshedMsgs(conv_with_msgs.messages));
                        }
                        Err(e) => {
                            error!("Failed to load messages for conversation {}: {}", cid, e);
                            let _ = tx.send(UiEvent::Error(format!(
                                "Errore caricamento messaggi: {}",
                                e
                            )));
                        }
                    }
                });
            }
        }
    }

    fn handle_conversation_created(state: &mut crate::state::core::AppState, conversation_id: Uuid) {
        info!("New conversation created: {}", conversation_id);
        state.conversation_messages.insert(conversation_id, vec![]);
        crate::app::events::helpers::add_system_message(state, "Conversazione creata!".into());
        state.request_conversations_refresh = true;
    }

    fn handle_dm_stub_created(state: &mut crate::state::core::AppState, conversation_id: Uuid, other_username: String) {
        info!("Creating DM stub for conversation {} with {}", conversation_id, other_username);

        if let Some(ref conversations) = state.conversations {
            let existing_count = conversations.iter().filter(|c| c.id == conversation_id).count();
            if existing_count > 0 {
                error!("ATTEMPTING TO CREATE DUPLICATE STUB! ID {} already exists {} times",
                       conversation_id, existing_count);
                crate::app::events::helpers::add_system_message(
                    state,
                    "Errore: conversazione già esistente".into()
                );
                return;
            }
        }

        if state.dm_stubs.contains_key(&conversation_id) {
            warn!("DM stub with ID {} already exists, not creating duplicate", conversation_id);
            crate::app::events::helpers::add_system_message(
                state,
                "Chat già esistente con questo utente".into()
            );
            return;
        }

        state.add_dm_stub(conversation_id, other_username.clone());
        state.conversation_messages.insert(conversation_id, vec![]);

        state.cid = Some(conversation_id);
        state.conv_title = other_username.clone();
        state.page = Page::Chat;
        state.messages = vec![];

        let welcome_msg = MessageDto::system_message(
            format!("Nuova chat con {}. Scrivi il primo messaggio!", other_username)
        );
        state.messages.push(welcome_msg);

        crate::app::events::helpers::add_system_message(
            state,
            format!("Chat con {} aperta - invia un messaggio per iniziare!", other_username)
        );

        debug!("Successfully created DM stub: {} -> {}", conversation_id, other_username);
    }

    fn handle_conversation_list_updated(state: &mut crate::state::core::AppState) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            debug!("Executing conversation list refresh");

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversations(&base, &token).await {
                    Ok(conversations) => {
                        info!("Successfully refreshed {} conversations", conversations.len());
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    }
                    Err(e) => {
                        error!("Failed to refresh conversations: {}", e);
                        let _ = tx.send(UiEvent::Error(format!("Errore aggiornamento conversazioni: {}", e)));
                    }
                }
            });
        } else {
            warn!("Cannot refresh conversations: no token available");
        }
    }
}