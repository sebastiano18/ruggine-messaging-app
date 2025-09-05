use crate::models::*;
use uuid::Uuid;
use std::collections::HashMap;

pub struct DataLoader;

impl DataLoader {
    /// Avvia il caricamento completo di tutti i dati dell'utente dopo il login
    pub fn preload_all_data(state: &mut super::core::AppState, token: String) {
        state.is_loading = true;

        let base = state.base.clone();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            Self::execute_full_preload(base, token, tx).await;
        });
    }

    /// Carica i messaggi di una singola conversazione
    pub fn load_single_conversation_messages(state: &super::core::AppState, cid: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                Self::execute_single_conversation_load(base, token, cid, tx).await;
            });
        }
    }

    /// Ricarica tutte le conversazioni (senza messaggi)
    pub fn refresh_conversations(state: &super::core::AppState) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::net::conversation::get_conversations(&base, &token).await {
                    Ok(conversations) => {
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Ricaricamento conversazioni fallito: {}", e)));
                    }
                }
            });
        }
    }

    /// Esegue il precaricamento completo (conversazioni + tutti i messaggi)
    async fn execute_full_preload(
        base: String,
        token: String,
        tx: tokio::sync::mpsc::UnboundedSender<UiEvent>
    ) {
        let _ = tx.send(UiEvent::LoadingProgress("Caricamento conversazioni...".into()));

        // 1. Carica tutte le conversazioni
        let conversations = match crate::net::conversation::get_conversations(&base, &token).await {
            Ok(convs) => {
                let _ = tx.send(UiEvent::ConversationsLoaded(convs.clone()));
                convs
            }
            Err(e) => {
                let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {}", e)));
                return;
            }
        };

        // Se non ci sono conversazioni, completa comunque il caricamento
        if conversations.is_empty() {
            let _ = tx.send(UiEvent::AllMessagesLoaded(HashMap::new()));
            let _ = tx.send(UiEvent::InitialLoadComplete);
            return;
        }

        // 2. Carica i messaggi per ogni conversazione
        let total_conversations = conversations.len();
        let mut all_messages = HashMap::new();
        let mut loaded_count = 0;

        for conv in conversations {
            loaded_count += 1;
            let _ = tx.send(UiEvent::LoadingProgress(
                format!("Caricamento messaggi ({}/{}): {}", loaded_count, total_conversations, conv.title)
            ));

            match Self::load_conversation_messages(&base, &token, conv.id).await {
                Ok(messages) => {
                    all_messages.insert(conv.id, messages);
                }
                Err(e) => {
                    tracing::warn!("Failed to load messages for conversation {}: {}", conv.id, e);
                    // Inserisci un messaggio di errore invece di fallire tutto
                    let error_msg = MessageDto::system_message(
                        format!("Errore nel caricamento messaggi per '{}': {}", conv.title, e)
                    );
                    all_messages.insert(conv.id, vec![error_msg]);
                }
            }
        }

        // 3. Invia tutti i messaggi e completa il caricamento
        let _ = tx.send(UiEvent::AllMessagesLoaded(all_messages));
        let _ = tx.send(UiEvent::InitialLoadComplete);
    }

    /// Esegue il caricamento dei messaggi di una singola conversazione
    async fn execute_single_conversation_load(
        base: String,
        token: String,
        cid: Uuid,
        tx: tokio::sync::mpsc::UnboundedSender<UiEvent>
    ) {
        match Self::load_conversation_messages(&base, &token, cid).await {
            Ok(messages) => {
                let _ = tx.send(UiEvent::RefreshedMsgs(messages));
            }
            Err(e) => {
                let _ = tx.send(UiEvent::Error(format!("Caricamento messaggi fallito: {}", e)));
            }
        }
    }

    /// Helper per caricare i messaggi di una conversazione specifica
    async fn load_conversation_messages(
        base: &str,
        token: &str,
        conversation_id: Uuid
    ) -> Result<Vec<MessageDto>, Box<dyn std::error::Error + Send + Sync>> {
        let messages = crate::net::chat::get_messages(base, token, conversation_id).await?;
        Ok(messages)
    }

    /// Carica i messaggi più recenti per una conversazione (utile per aggiornamenti)
    pub fn load_recent_messages(state: &super::core::AppState, cid: Uuid, limit: Option<u32>) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                // Se hai un endpoint per messaggi recenti, usalo qui
                // Altrimenti usa il metodo standard
                match Self::load_conversation_messages(&base, &token, cid).await {
                    Ok(mut messages) => {
                        // Limita i messaggi se richiesto
                        if let Some(limit) = limit {
                            let start = messages.len().saturating_sub(limit as usize);
                            messages = messages.split_off(start);
                        }
                        let _ = tx.send(UiEvent::RefreshedMsgs(messages));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Caricamento messaggi recenti fallito: {}", e)));
                    }
                }
            });
        }
    }

    /// Precarica solo i metadati delle conversazioni (senza messaggi)
    pub fn preload_conversations_only(state: &mut super::core::AppState, token: String) {
        let base = state.base.clone();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            let _ = tx.send(UiEvent::LoadingProgress("Caricamento conversazioni...".into()));

            match crate::net::conversation::get_conversations(&base, &token).await {
                Ok(conversations) => {
                    let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    let _ = tx.send(UiEvent::LoadingProgress("Conversazioni caricate".into()));
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {}", e)));
                }
            }
        });
    }
}