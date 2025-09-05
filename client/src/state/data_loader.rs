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

    /// Ricarica tutte le conversazioni (senza messaggi) - Metodo principale per il refresh
    pub fn refresh_conversations(state: &super::core::AppState) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversations(&base, &token).await {
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

    /// Refresh delle conversazioni con apertura automatica di una conversazione specifica
    /// Questo metodo gestisce la race condition tra creazione server-side e caricamento client
    pub fn refresh_conversations_and_open(state: &super::core::AppState, conversation_to_open: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                // Strategia di retry con backoff per gestire la sincronizzazione server
                let max_attempts = 5;
                let mut attempt = 0;
                let mut delay_ms = 200;

                while attempt < max_attempts {
                    attempt += 1;

                    // Aggiungi delay crescente per ogni tentativo (eccetto il primo)
                    if attempt > 1 {
                        let _ = tx.send(UiEvent::Info(format!("Sincronizzazione... (tentativo {})", attempt)));
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                        delay_ms = (delay_ms as f32 * 1.5) as u64; // Backoff esponenziale
                    }

                    match crate::api::conversation::get_conversations(&base, &token).await {
                        Ok(conversations) => {
                            // Verifica se la conversazione target esiste
                            if conversations.iter().any(|c| c.id == conversation_to_open) {
                                let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                                let _ = tx.send(UiEvent::Info("Nuova conversazione caricata".into()));
                                return; // Successo, esci dal loop
                            } else if attempt == max_attempts {
                                // Ultimo tentativo fallito, carica comunque e segnala errore
                                let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                                let _ = tx.send(UiEvent::Error("Conversazione non trovata dopo creazione".into()));
                                return;
                            }
                            // Continua il loop per retry
                        }
                        Err(e) => {
                            if attempt == max_attempts {
                                let _ = tx.send(UiEvent::Error(format!("Ricaricamento conversazioni fallito: {}", e)));
                                return;
                            }
                            // Continua il loop per retry
                        }
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
        let conversations = match crate::api::conversation::get_conversations(&base, &token).await {
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

        // 2. Carica i messaggi per ogni conversazione in parallelo (con limite di concorrenza)
        let total_conversations = conversations.len();
        let _ = tx.send(UiEvent::LoadingProgress(format!("Caricamento messaggi per {} conversazioni...", total_conversations)));

        let mut all_messages = HashMap::new();
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(5)); // Max 5 richieste parallele

        let mut handles = vec![];
        for (index, conv) in conversations.into_iter().enumerate() {
            let base_clone = base.clone();
            let token_clone = token.clone();
            let tx_clone = tx.clone();
            let semaphore_clone = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await.unwrap();

                let _ = tx_clone.send(UiEvent::LoadingProgress(
                    format!("Caricando messaggi ({}/{}): {}", index + 1, total_conversations, conv.title)
                ));

                let messages = match Self::load_conversation_messages(&base_clone, &token_clone, conv.id).await {
                    Ok(msgs) => msgs,
                    Err(e) => {
                        tracing::warn!("Failed to load messages for conversation {}: {}", conv.id, e);
                        // Ritorna messaggio di errore invece di fallire tutto
                        vec![MessageDto::system_message(
                            format!("Errore nel caricamento messaggi: {}", e)
                        )]
                    }
                };

                (conv.id, messages)
            });

            handles.push(handle);
        }

        // Attendi tutti i caricamenti
        for handle in handles {
            if let Ok((conv_id, messages)) = handle.await {
                all_messages.insert(conv_id, messages);
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
        let messages = crate::api::chat::get_messages(base, token, conversation_id).await?;
        Ok(messages)
    }

    /// Carica i messaggi più recenti per una conversazione (utile per aggiornamenti incrementali)
    pub fn load_recent_messages(state: &super::core::AppState, cid: Uuid, limit: Option<u32>) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
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

    /// Precarica solo i metadati delle conversazioni (senza messaggi) - utile per refresh rapidi
    pub fn preload_conversations_only(state: &mut super::core::AppState, token: String) {
        let base = state.base.clone();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            let _ = tx.send(UiEvent::LoadingProgress("Caricamento conversazioni...".into()));

            match crate::api::conversation::get_conversations(&base, &token).await {
                Ok(conversations) => {
                    let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {}", e)));
                }
            }
        });
    }

    /// Refresh intelligente che carica i messaggi solo per le nuove conversazioni
    /// Deprecato in favore di refresh_conversations_and_open
    #[deprecated(note = "Usa refresh_conversations_and_open invece")]
    pub fn smart_refresh_with_new_conversation(state: &super::core::AppState, new_conversation_id: Uuid) {
        Self::refresh_conversations_and_open(state, new_conversation_id);
    }

    /// Forza il refresh di una conversazione specifica e i suoi messaggi
    pub fn force_refresh_conversation(state: &super::core::AppState, cid: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                // Prima ricarica la lista conversazioni
                match crate::api::conversation::get_conversations(&base, &token).await {
                    Ok(conversations) => {
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));

                        // Poi ricarica i messaggi specifici
                        match Self::load_conversation_messages(&base, &token, cid).await {
                            Ok(messages) => {
                                let _ = tx.send(UiEvent::RefreshedMsgs(messages));
                            }
                            Err(e) => {
                                let _ = tx.send(UiEvent::Error(format!("Ricaricamento messaggi fallito: {}", e)));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Ricaricamento conversazioni fallito: {}", e)));
                    }
                }
            });
        }
    }

    /// Carica messaggi in batch per multiple conversazioni
    pub fn load_messages_batch(state: &super::core::AppState, conversation_ids: Vec<Uuid>) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                let mut messages_map = HashMap::new();
                let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3)); // Limite concorrenza

                let mut handles = vec![];
                for cid in conversation_ids {
                    let base_clone = base.clone();
                    let token_clone = token.clone();
                    let semaphore_clone = semaphore.clone();

                    let handle = tokio::spawn(async move {
                        let _permit = semaphore_clone.acquire().await.unwrap();
                        match Self::load_conversation_messages(&base_clone, &token_clone, cid).await {
                            Ok(messages) => Some((cid, messages)),
                            Err(_) => None,
                        }
                    });

                    handles.push(handle);
                }

                // Raccogli tutti i risultati
                for handle in handles {
                    if let Ok(Some((cid, messages))) = handle.await {
                        messages_map.insert(cid, messages);
                    }
                }

                if !messages_map.is_empty() {
                    let _ = tx.send(UiEvent::AllMessagesLoaded(messages_map));
                }
            });
        }
    }
}