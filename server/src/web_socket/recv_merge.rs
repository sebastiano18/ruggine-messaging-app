// recv_merge.rs - Complete rewrite with centralized notification handling
use futures::StreamExt;
use serde_json::Value;
use tokio::{
    select,
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Duration, sleep},
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn, debug};
use uuid::Uuid;
use std::collections::HashMap;

use super::{actor::OutboundMsg, helpers::handle_user_notification};
use crate::{error::Result, state::AppState, error::AppError};

// Messaggi interni per gestire i stream dinamici
#[derive(Debug)]
enum InternalMessage {
    ConversationMessage {
        conversation_id: Uuid,
        message: Value,
    },
    StreamClosed {
        conversation_id: Uuid,
    },
}

// Gestisce stream dinamici con task separati per ogni conversazione
struct DynamicStreamManager {
    // Task handles per ogni conversazione
    conversation_tasks: HashMap<Uuid, JoinHandle<()>>,
    // Canale per ricevere messaggi da tutti i task
    message_rx: mpsc::Receiver<InternalMessage>,
    message_tx: mpsc::Sender<InternalMessage>,
}

impl DynamicStreamManager {
    fn new() -> Self {
        let (message_tx, message_rx) = mpsc::channel(1000);
        Self {
            conversation_tasks: HashMap::new(),
            message_rx,
            message_tx,
        }
    }

    // Aggiunge un nuovo stream conversazione con task dedicato
    async fn add_conversation_stream(
        &mut self,
        conv_id: Uuid,
        mut stream: BroadcastStream<Value>,
    ) {
        if self.conversation_tasks.contains_key(&conv_id) {
            debug!("Conversation {} already has active stream", conv_id);
            return;
        }

        let tx = self.message_tx.clone();

        // Spawn task dedicato per questa conversazione
        let task = tokio::spawn(async move {
            info!("Stream task started for conversation {}", conv_id);

            while let Some(result) = stream.next().await {
                match result {
                    Ok(message) => {
                        let internal_msg = InternalMessage::ConversationMessage {
                            conversation_id: conv_id,
                            message,
                        };

                        if tx.send(internal_msg).await.is_err() {
                            break; // Receiver chiuso
                        }
                    }
                    Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped)) => {
                        warn!("Stream for conversation {} lagged, skipped {} messages", conv_id, skipped);
                        // Continua comunque
                    }
                }
            }

            // Notifica che questo stream si è chiuso
            let _ = tx.send(InternalMessage::StreamClosed {
                conversation_id: conv_id,
            }).await;

            info!("Stream task ended for conversation {}", conv_id);
        });

        self.conversation_tasks.insert(conv_id, task);
        info!("Added dynamic stream for conversation {} (total: {})", 
              conv_id, self.conversation_tasks.len());
    }

    // Rimuove e termina il task di una conversazione
    fn remove_conversation_stream(&mut self, conv_id: Uuid) {
        if let Some(task) = self.conversation_tasks.remove(&conv_id) {
            task.abort();
            info!("Removed stream for conversation {} (remaining: {})", 
                  conv_id, self.conversation_tasks.len());
        }
    }

    fn conversation_count(&self) -> usize {
        self.conversation_tasks.len()
    }

    // Cleanup quando la connessione si chiude
    fn cleanup(&mut self) {
        for (conv_id, task) in self.conversation_tasks.drain() {
            task.abort();
            debug!("Aborted stream task for conversation {}", conv_id);
        }
    }

    // Aggiorna dinamicamente i stream con le nuove conversazioni dell'utente
    async fn refresh_conversation_streams(&mut self, state: &AppState, user_id: Uuid) {
        // Ottieni le conversazioni correnti dell'utente
        let current_channels = match state.get_user_channels(user_id).await {
            Ok(channels) => channels,
            Err(e) => {
                error!("Failed to get user channels for {}: {}", user_id, e);
                return;
            }
        };

        let current_conv_ids: std::collections::HashSet<Uuid> =
            current_channels.iter().map(|(id, _)| *id).collect();
        let existing_conv_ids: std::collections::HashSet<Uuid> =
            self.conversation_tasks.keys().cloned().collect();

        // Aggiungi nuove conversazioni
        for (conv_id, tx) in current_channels {
            if !existing_conv_ids.contains(&conv_id) {
                let receiver = tx.subscribe();
                let stream = BroadcastStream::new(receiver);
                self.add_conversation_stream(conv_id, stream).await;

                let receiver_count = tx.receiver_count();
                info!("User {} subscribed to new conversation {} ({} receivers)", 
                      user_id, conv_id, receiver_count);
            }
        }

        // Rimuovi conversazioni che non esistono più
        let conversations_to_remove: Vec<Uuid> = existing_conv_ids
            .difference(&current_conv_ids)
            .cloned()
            .collect();

        for conv_id in conversations_to_remove {
            self.remove_conversation_stream(conv_id);
            info!("Removed stream for conversation {} (user {} no longer participant)", 
                  conv_id, user_id);
        }
    }
}

impl Drop for DynamicStreamManager {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub async fn spawn_receiver(
    state: AppState,
    user_id: Uuid,
    out_tx: mpsc::Sender<OutboundMsg>,
    stop_tx: watch::Sender<bool>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<JoinHandle<()>> {
    // Carica le conversazioni iniziali
    let initial_channels = state
        .get_user_channels(user_id)
        .await
        .map_err(|e| crate::error::AppError::from(e))?;

    info!("User {} found {} initial conversation channels", user_id, initial_channels.len());

    // Inizializza il manager dinamico degli stream
    let mut stream_manager = DynamicStreamManager::new();

    // Aggiungi le conversazioni iniziali
    for (conv_id, tx) in initial_channels {
        let receiver = tx.subscribe();
        let stream = BroadcastStream::new(receiver);
        stream_manager.add_conversation_stream(conv_id, stream).await;

        let receiver_count = tx.receiver_count();
        info!("User {} subscribed to conversation {} (now {} receivers)", 
              user_id, conv_id, receiver_count);
    }

    // Setup canale utente per notifiche (conversation_created, etc.)
    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    let mut user_channel_stream = BroadcastStream::new(user_tx.subscribe());
    info!("User {} subscribed to notification channel", user_id);

    // Variabili per gestione backoff e refresh periodici
    let mut empty_backoff_seconds = 30u64;
    const MAX_BACKOFF_SECONDS: u64 = 300;
    let mut consecutive_none_count = 0u32;
    const MAX_CONSECUTIVE_NONE: u32 = 5;

    // Timer per refresh periodico delle conversazioni
    let mut refresh_interval = tokio::time::interval(Duration::from_secs(300)); // 5 minuti
    refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    Ok(tokio::spawn(async move {
        loop {
            select! {
                _ = stop_rx.changed() => {
                    info!("Stop signal received for recv-merge user {}", user_id);
                    stream_manager.cleanup();
                    break;
                }

                // Messaggi dal canale utente (notifiche, conversation_created, etc.)
                user_msg = user_channel_stream.next() => {
                    match user_msg {
                        Some(Ok(val)) => {
                            if let Some(msg_type) = val.get("type").and_then(|t| t.as_str()) {
                                match msg_type {
                                    "conversation_created" => {
                                        debug!("Received conversation_created notification for user {}", user_id);

                                        // CENTRALIZZATO: Tutta la logica in handle_user_notification
                                        if let Err(e) = handle_user_notification(&state, &val, user_id, &out_tx).await {
                                            warn!("Failed to handle conversation_created notification for user {}: {}", user_id, e);
                                        } else {
                                            // Aggiorna i stream per includere la nuova conversazione
                                            stream_manager.refresh_conversation_streams(&state, user_id).await;
                                            
                                            // Reset contatori di backoff su successo
                                            empty_backoff_seconds = 30;
                                            consecutive_none_count = 0;
                                        }
                                    }
                                    _ => {
                                        // Altri tipi di notifiche utente - forward al client
                                        debug!("Received user notification type '{}' for user {}", msg_type, user_id);
                                        if let Ok(txt) = serde_json::to_string(&val) {
                                            if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                                                warn!("Failed to send user notification to user {}, stopping receiver", user_id);
                                                let _ = stop_tx.send(true);
                                                break;
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Notifiche senza type - forward comunque
                                debug!("Received user notification without type for user {}", user_id);
                                if let Ok(txt) = serde_json::to_string(&val) {
                                    if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                                        warn!("Failed to send user notification to user {}, stopping receiver", user_id);
                                        let _ = stop_tx.send(true);
                                        break;
                                    }
                                }
                            }
                        }
                        Some(Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped))) => {
                            warn!("User {} lagged behind on user channel, skipped {} messages", user_id, skipped);
                        }
                        None => {
                            warn!("User notification channel closed for user {}", user_id);
                            // Non interrompere per questo - potrebbe essere temporaneo
                        }
                    }
                }

                // Messaggi dalle conversazioni (tramite stream manager dinamico)
                internal_msg = stream_manager.message_rx.recv() => {
                    match internal_msg {
                        Some(InternalMessage::ConversationMessage { conversation_id, message }) => {
                            // Reset contatori quando riceviamo messaggi validi
                            empty_backoff_seconds = 30;
                            consecutive_none_count = 0;

                            // Prevenzione echo: non inviare messaggi al mittente
                            if let Some(author) = message.get("author_id")
                                .and_then(|x| x.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok())
                            {
                                if author == user_id {
                                    debug!("Skipping echo message from same user {} in conversation {}", user_id, conversation_id);
                                    continue;
                                }
                            }

                            // Forward messaggio al client WebSocket
                            match serde_json::to_string(&message) {
                                Ok(txt) => {
                                    if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                                        warn!("Failed to send message to user {}, stopping receiver", user_id);
                                        let _ = stop_tx.send(true);
                                        break;
                                    }
                                    debug!("Forwarded message from conversation {} to user {}", conversation_id, user_id);
                                }
                                Err(e) => {
                                    error!("Serialization error for user {}: {}", user_id, e);
                                    let _ = out_tx.send(OutboundMsg::Text(
                                        r#"{"type":"error","message":"serialization_failed"}"#.into()
                                    )).await;
                                }
                            }
                        }
                        Some(InternalMessage::StreamClosed { conversation_id }) => {
                            warn!("Stream closed for conversation {} (user {})", conversation_id, user_id);
                            stream_manager.remove_conversation_stream(conversation_id);
                            consecutive_none_count += 1;

                            // Backoff progressivo per evitare thrashing
                            if consecutive_none_count >= MAX_CONSECUTIVE_NONE {
                                warn!("Too many stream closures for user {}, backing off for {} seconds",
                                      user_id, empty_backoff_seconds);
                                sleep(Duration::from_secs(empty_backoff_seconds)).await;
                                empty_backoff_seconds = (empty_backoff_seconds * 2).min(MAX_BACKOFF_SECONDS);
                                consecutive_none_count = 0;
                                
                                // Refresh dopo backoff per recuperare eventuali conversazioni perse
                                stream_manager.refresh_conversation_streams(&state, user_id).await;
                            }
                        }
                        None => {
                            // Il canale interno è chiuso - probabilmente in shutdown
                            info!("Internal message channel closed for user {}", user_id);
                            break;
                        }
                    }
                }

                // Refresh periodico delle conversazioni (ogni 5 minuti)
                _ = refresh_interval.tick() => {
                    debug!("Performing periodic conversation refresh for user {}", user_id);
                    stream_manager.refresh_conversation_streams(&state, user_id).await;
                }

                // Branch eseguito quando non ci sono conversazioni attive
                _ = sleep(Duration::from_secs(10)), if stream_manager.conversation_count() == 0 => {
                    debug!("No active conversations for user {}, checking for new ones...", user_id);
                    // Prova a ricaricare le conversazioni nel caso ne siano state aggiunte
                    stream_manager.refresh_conversation_streams(&state, user_id).await;
                }
            }
        }

        // Cleanup finale
        stream_manager.cleanup();
        info!("recv-merge ended for user {} (processed conversations: {})", 
              user_id, stream_manager.conversation_tasks.len());
    }))
}

/// Forza un refresh completo con fetch - usata per recovery da errori
/// NOTA: Questa funzione è mantenuta per compatibilità ma non dovrebbe essere necessaria
/// con il nuovo sistema di gestione dinamica
pub async fn force_refresh_with_fetch(
    state: &AppState,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) {
    info!("Force refresh with fetch requested for user {} (legacy function)", user_id);

    // Nel nuovo sistema, il refresh avviene automaticamente
    // Ma possiamo inviare un messaggio di debug al client
    let debug_msg = serde_json::json!({
        "type": "debug",
        "message": "Conversation refresh requested",
        "user_id": user_id,
        "timestamp": chrono::Utc::now().timestamp()
    });

    if let Ok(txt) = serde_json::to_string(&debug_msg) {
        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
    }

    warn!("force_refresh_with_fetch is deprecated - using dynamic stream management instead");
}