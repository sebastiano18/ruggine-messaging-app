use crate::models::*;
use std::collections::HashMap;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct DataLoader;

impl DataLoader {
    /// Avvia il caricamento completo di tutti i dati dell'utente dopo il login
    /* pub fn preload_all_data(state: &mut super::core::AppState, token: String) {
        state.is_loading = true;

        let base = state.base.clone();
        let tx = state.ui_tx.clone();

        state.rt.spawn(async move {
            Self::execute_full_preload(base, token, tx).await;
        });
    }*/

    /// Carica i messaggi di una singola conversazione (DEPRECATO - usa TriggerConversationFetch)
    pub fn load_single_conversation_messages(state: &super::core::AppState, cid: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversation_with_messages(&base, &token, cid).await {
                    Ok(conv_with_msgs) => {
                        info!("Loaded {} messages for conversation {}", 
                              conv_with_msgs.messages.len(), cid);
                        let _ = tx.send(UiEvent::RefreshedMsgs(conv_with_msgs.messages));
                    }
                    Err(e) => {
                        error!("Failed to load messages: {}", e);
                        let _ = tx.send(UiEvent::Error(format!(
                            "Errore caricamento messaggi: {}",
                            e
                        )));
                    }
                }
            });
        }
    }

    /// Esegue il precaricamento completo (conversazioni + tutti i messaggi)
    /// MIGLIORATO: Gestione errori più robusta e timeout
    async fn execute_full_preload(
        base: String,
        token: String,
        tx: tokio::sync::mpsc::UnboundedSender<UiEvent>,
    ) {
        let _ = tx.send(UiEvent::LoadingProgress(
            "Caricamento conversazioni...".into(),
        ));

        // 1. Carica tutte le conversazioni con timeout
        let conversations = match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            crate::api::conversation::get_conversations(&base, &token),
        )
            .await
        {
            Ok(Ok(convs)) => {
                let _ = tx.send(UiEvent::ConversationsLoaded(convs.clone()));
                convs
            }
            Ok(Err(e)) => {
                error!("Failed to load conversations: {}", e);
                let _ = tx.send(UiEvent::Error(format!(
                    "Caricamento conversazioni fallito: {}",
                    e
                )));
                return;
            }
            Err(_) => {
                error!("Timeout loading conversations");
                let _ = tx.send(UiEvent::Error(
                    "Timeout nel caricamento conversazioni".into(),
                ));
                return;
            }
        };

        // Se non ci sono conversazioni, completa comunque il caricamento
        if conversations.is_empty() {
            let _ = tx.send(UiEvent::AllMessagesLoaded(HashMap::new()));
            let _ = tx.send(UiEvent::InitialLoadComplete);
            return;
        }

        // 2. NUOVO: Usa l'endpoint unificato per caricare conversazioni + messaggi
        let total_conversations = conversations.len();
        let _ = tx.send(UiEvent::LoadingProgress(format!(
            "Caricamento messaggi per {} conversazioni...",
            total_conversations
        )));

        let mut all_messages = HashMap::new();
        let max_concurrent = (total_conversations.min(8)).max(2);
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));

        let mut handles = vec![];
        for (index, conv) in conversations.into_iter().enumerate() {
            let base_clone = base.clone();
            let token_clone = token.clone();
            let tx_clone = tx.clone();
            let semaphore_clone = semaphore.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await.unwrap();

                let _ = tx_clone.send(UiEvent::LoadingProgress(format!(
                    "Caricando conversazione ({}/{}): {}",
                    index + 1,
                    total_conversations,
                    conv.title.chars().take(30).collect::<String>()
                )));

                // NUOVO: Usa endpoint unificato con timeout
                let messages = match tokio::time::timeout(
                    tokio::time::Duration::from_secs(20),
                    Self::load_conversation_with_messages(&base_clone, &token_clone, conv.id),
                )
                    .await
                {
                    Ok(Ok(msgs)) => msgs,
                    Ok(Err(e)) => {
                        warn!(
                            "Failed to load messages for conversation {}: {}",
                            conv.id, e
                        );
                        vec![MessageDto::system_message(format!(
                            "Errore caricamento messaggi: {}",
                            e
                        ))]
                    }
                    Err(_) => {
                        warn!("Timeout loading messages for conversation {}", conv.id);
                        vec![MessageDto::system_message(
                            "Timeout nel caricamento messaggi".into(),
                        )]
                    }
                };

                (conv.id, messages)
            });

            handles.push(handle);
        }

        // Attendi tutti i caricamenti con timeout globale
        let collect_future = async {
            for handle in handles {
                if let Ok((conv_id, messages)) = handle.await {
                    all_messages.insert(conv_id, messages);
                }
            }
        };

        match tokio::time::timeout(tokio::time::Duration::from_secs(120), collect_future).await {
            Ok(_) => {
                info!("Successfully loaded all conversation messages");
            }
            Err(_) => {
                warn!("Timeout waiting for all messages to load");
                let _ = tx.send(UiEvent::Info(
                    "Alcuni messaggi potrebbero non essere stati caricati completamente".into(),
                ));
            }
        }

        // 3. Invia tutti i messaggi e completa il caricamento
        let _ = tx.send(UiEvent::AllMessagesLoaded(all_messages));
        let _ = tx.send(UiEvent::InitialLoadComplete);
    }



    /// Esegue il caricamento dei messaggi di una singola conversazione (DEPRECATO)
    /// Mantenuto per compatibilità, ma usa TriggerConversationFetch nel nuovo sistema
    async fn execute_single_conversation_load(
        base: String,
        token: String,
        cid: Uuid,
        tx: tokio::sync::mpsc::UnboundedSender<UiEvent>,
    ) {
        warn!("Using deprecated single conversation load for conversation {}", cid);

        let load_future = Self::load_conversation_messages(&base, &token, cid);

        match tokio::time::timeout(tokio::time::Duration::from_secs(15), load_future).await {
            Ok(Ok(messages)) => {
                let _ = tx.send(UiEvent::RefreshedMsgs(messages));
            }
            Ok(Err(e)) => {
                let _ = tx.send(UiEvent::Error(format!(
                    "Caricamento messaggi fallito: {}",
                    e
                )));
            }
            Err(_) => {
                let _ = tx.send(UiEvent::Error("Timeout nel caricamento messaggi".into()));
            }
        }
    }

    /// Helper per caricare i messaggi di una conversazione specifica (DEPRECATO)
    /// Mantenuto per compatibility e fallback
    async fn load_conversation_messages(
        base: &str,
        token: &str,
        conversation_id: Uuid,
    ) -> Result<Vec<MessageDto>, Box<dyn std::error::Error + Send + Sync>> {
        let messages = crate::api::chat::get_messages(base, token, conversation_id).await?;
        Ok(messages)
    }


    async fn load_conversation_with_messages(
        base: &str,
        token: &str,
        conversation_id: Uuid,
    ) -> Result<Vec<MessageDto>, Box<dyn std::error::Error + Send + Sync>> {
        // Usa SOLO l'endpoint unificato
        let conversation_with_messages = crate::api::conversation::get_conversation_with_messages(base, token, conversation_id).await?;
        Ok(conversation_with_messages.messages)
    }

   
}
