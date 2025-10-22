use crate::models::*;
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct DataLoader;

impl DataLoader {
    /// Carica i messaggi di una singola conversazione
    pub fn load_single_conversation_messages(state: &super::core::AppState, cid: Uuid) {
        if let Some(ref token) = state.token {
            let base = state.base.clone();
            let token = token.clone();
            let tx = state.ui_tx.clone();

            state.rt.spawn(async move {
                match crate::api::conversation::get_conversation_with_messages(&base, &token, cid).await {
                    Ok(conv_with_msgs) => {
                        info!(
                            "Loaded {} messages for conversation {}",
                            conv_with_msgs.messages.len(),
                            cid
                        );

                        // DEBUG: Stampa le sequence dei messaggi
                        for (i, msg) in conv_with_msgs.messages.iter().enumerate() {
                            debug!(
                                "Message {}: id={}, content='{}', sequence={:?}",
                                i,
                                msg.id,
                                msg.content.chars().take(50).collect::<String>(),
                                msg.sequence_num
                            );
                        }

                        // Conta quanti messaggi hanno sequence
                        let with_seq = conv_with_msgs.messages.iter().filter(|m| m.sequence_num.is_some()).count();
                        let without_seq = conv_with_msgs.messages.len() - with_seq;

                        warn!(
                            "Conversation {} messages: {} total, {} with sequence, {} without sequence",
                            cid, conv_with_msgs.messages.len(), with_seq, without_seq
                        );

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

        if conversations.is_empty() {
            let _ = tx.send(UiEvent::AllMessagesLoaded(HashMap::new()));
            let _ = tx.send(UiEvent::InitialLoadComplete);
            return;
        }

        // 2. Carica tutti i messaggi
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

                let messages = match tokio::time::timeout(
                    tokio::time::Duration::from_secs(20),
                    Self::load_conversation_with_messages(&base_clone, &token_clone, conv.id),
                )
                .await
                {
                    Ok(Ok(msgs)) => {
                        // DEBUG: Log delle sequence per ogni conversazione caricata
                        let with_seq = msgs.iter().filter(|m| m.sequence_num.is_some()).count();
                        let without_seq = msgs.len() - with_seq;

                        warn!(
                            "Loaded conversation {}: {} messages ({} with seq, {} without)",
                            conv.id,
                            msgs.len(),
                            with_seq,
                            without_seq
                        );

                        if without_seq > 0 && with_seq == 0 {
                            error!("CRITICAL: Conversation {} has {} messages but NONE have sequences!", 
                                   conv.id, msgs.len());
                        }

                        msgs
                    }
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

        // Attendi tutti i caricamenti
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

                // DEBUG: Stampa riassunto finale
                let total_msgs: usize = all_messages.values().map(|v| v.len()).sum();
                let msgs_with_seq: usize = all_messages
                    .values()
                    .flat_map(|v| v.iter())
                    .filter(|m| m.sequence_num.is_some())
                    .count();

                warn!(
                    "FINAL LOAD SUMMARY: {} total messages, {} with sequences ({:.1}%)",
                    total_msgs,
                    msgs_with_seq,
                    if total_msgs > 0 {
                        (msgs_with_seq as f64 / total_msgs as f64) * 100.0
                    } else {
                        0.0
                    }
                );
            }
            Err(_) => {
                warn!("Timeout waiting for all messages to load");
                let _ = tx.send(UiEvent::Info(
                    "Alcuni messaggi potrebbero non essere stati caricati completamente".into(),
                ));
            }
        }

        let _ = tx.send(UiEvent::AllMessagesLoaded(all_messages));
        let _ = tx.send(UiEvent::InitialLoadComplete);
    }

    async fn load_conversation_with_messages(
        base: &str,
        token: &str,
        conversation_id: Uuid,
    ) -> Result<Vec<MessageDto>, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Calling get_conversation_with_messages for {}",
            conversation_id
        );

        let conversation_with_messages =
            crate::api::conversation::get_conversation_with_messages(base, token, conversation_id)
                .await?;

        // DEBUG: Verifica le sequence
        for msg in &conversation_with_messages.messages {
            if msg.sequence_num.is_none() {
                warn!(
                    "Message {} in conversation {} has NO sequence!",
                    msg.id, conversation_id
                );
            } else {
                debug!("Message {} has sequence: {:?}", msg.id, msg.sequence_num);
            }
        }

        Ok(conversation_with_messages.messages)
    }
}
