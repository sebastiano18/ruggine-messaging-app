// recv_merge.rs - Implementazione con dynamic stream management
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::{
    select,
    sync::{mpsc, watch},
    task::{JoinHandle, JoinSet},
    time::{Duration, sleep},
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn, debug};
use uuid::Uuid;
use std::collections::HashMap;

use super::{actor::OutboundMsg, helpers::handle_user_notification};
use crate::{error::Result, state::AppState, error::AppError};

// Messaggi interni per gestire i stream
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

// Struttura per gestire stream dinamici con task separati
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

    // Canale utente per notifiche (separato dal manager dinamico)
    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    let mut user_channel_stream = BroadcastStream::new(user_tx.subscribe());
    info!("User {} subscribed to notification channel", user_id);

    // Variabili per tracking del backoff
    let mut empty_backoff_seconds = 30u64;
    const MAX_BACKOFF_SECONDS: u64 = 300;
    let mut consecutive_none_count = 0u32;
    const MAX_CONSECUTIVE_NONE: u32 = 5;

    Ok(tokio::spawn(async move {
        loop {
            select! {
                _ = stop_rx.changed() => {
                    info!("Stop signal received for recv-merge user {}", user_id);
                    stream_manager.cleanup(); // Pulisci tutti i task
                    break;
                }

                // Messaggi dal canale utente (notifiche, controllo)
                user_msg = user_channel_stream.next() => {
                    match user_msg {
                        Some(Ok(val)) => {
                            if let Some(msg_type) = val.get("type").and_then(|t| t.as_str()) {
                                if msg_type == "conversation_created" {
                                    debug!("Received conversation_created notification for user {}", user_id);

                                    // Gestisci la notifica tramite helper
                                    if let Err(e) = handle_user_notification(&state, &val, user_id, &out_tx).await {
                                        warn!("Failed to handle user notification for user {}: {}", user_id, e);
                                    }

                                    // AGGIUNTA INCREMENTALE - No refresh completo!
                                    if let Some(conv_id_str) = val.get("conversation_id").and_then(|v| v.as_str()) {
                                        if let Ok(new_conv_id) = Uuid::parse_str(conv_id_str) {
                                            match add_single_conversation_stream(&state, user_id, new_conv_id, &mut stream_manager).await {
                                                Ok(()) => {
                                                    info!("Successfully added conversation {} to user {} (total: {})", 
                                                          new_conv_id, user_id, stream_manager.conversation_count());
                                                    empty_backoff_seconds = 30;
                                                    consecutive_none_count = 0;
                                                }
                                                Err(e) => {
                                                    error!("Failed to add conversation {} for user {}: {}", 
                                                           new_conv_id, user_id, e);
                                                }
                                            }
                                        }
                                    }
                                    continue;
                                }
                            }

                            // Altri messaggi dal canale utente (forward al client)
                            if let Ok(txt) = serde_json::to_string(&val) {
                                if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                                    warn!("Failed to send user notification to user {}, stopping receiver", user_id);
                                    let _ = stop_tx.send(true);
                                    break;
                                }
                            }
                        }
                        Some(Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped))) => {
                            warn!("User {} lagged behind on user channel, skipped {} messages", user_id, skipped);
                        }
                        None => {
                            warn!("User notification channel closed for user {}", user_id);
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

                            // Prevenzione echo al mittente
                            if let Some(author) = message.get("author_id")
                                .and_then(|x| x.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok())
                            {
                                if author == user_id {
                                    debug!("Skipping echo message from same user {} in conversation {}", user_id, conversation_id);
                                    continue;
                                }
                            }

                            // Forward messaggio al client
                            match serde_json::to_string(&message) {
                                Ok(txt) => {
                                    if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                                        warn!("Failed to send message to user {}, stopping receiver", user_id);
                                        let _ = stop_tx.send(true);
                                        break;
                                    }
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

                            if consecutive_none_count >= MAX_CONSECUTIVE_NONE {
                                warn!("Too many stream closures for user {}, backing off for {} seconds",
                                      user_id, empty_backoff_seconds);
                                sleep(Duration::from_secs(empty_backoff_seconds)).await;
                                empty_backoff_seconds = (empty_backoff_seconds * 2).min(MAX_BACKOFF_SECONDS);
                                consecutive_none_count = 0;
                            }
                        }
                        None => {
                            // Il canale interno è chiuso - probabilmente in shutdown
                            info!("Internal message channel closed for user {}", user_id);
                            break;
                        }
                    }
                }

                // Branch eseguito quando non ci sono conversazioni attive
                _ = sleep(Duration::from_secs(5)), if stream_manager.conversation_count() == 0 => {
                    debug!("No active conversations for user {}, waiting...", user_id);
                }
            }
        }

        // Cleanup finale
        stream_manager.cleanup();
        info!("recv-merge ended for user {}", user_id);
    }))
}

/// Aggiunge una singola conversazione al manager degli stream
/// QUESTA È LA FUNZIONE CHIAVE - Aggiunta incrementale senza refresh
async fn add_single_conversation_stream(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
    stream_manager: &mut DynamicStreamManager,
) -> Result<()> {

    // Verifica che l'utente sia partecipante della conversazione
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    let is_participant: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
    )
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if is_participant == 0 {
        return Err(AppError::Forbidden);
    }

    // Ottieni il broadcast channel e crea il receiver
    let conv_tx = state.get_or_create_broadcast_tx(conversation_id).await;
    let receiver = conv_tx.subscribe();
    let stream = BroadcastStream::new(receiver);

    let receiver_count = conv_tx.receiver_count();
    info!("User {} subscribing to conversation {} ({} receivers)", 
          user_id, conversation_id, receiver_count);

    // AGGIUNTA INCREMENTALE - questo è il punto chiave!
    stream_manager.add_conversation_stream(conversation_id, stream).await;

    Ok(())
}

/// Mantenuta per compatibilità ma non più utilizzata
pub async fn force_refresh_with_fetch(
    state: &AppState,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) {
    info!("Force refresh with fetch for user {} (legacy function)", user_id);
    warn!("force_refresh_with_fetch called but not needed with dynamic stream approach");
}

/// Invia fetch events per conversazioni specifiche - mantenuta per initial fetch
async fn send_fetch_events_for_conversations(
    state: &AppState,
    user_id: Uuid,
    conversation_ids: &[Uuid],
    out_tx: &mpsc::Sender<OutboundMsg>,
) {
    debug!("Checking {} conversations for messages to fetch", conversation_ids.len());

    let mut events_sent = 0;

    for &conversation_id in conversation_ids {
        let conversation_id_str = conversation_id.to_string();

        let message_count: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE conversation_id = ?"
        )
            .bind(&conversation_id_str)
            .fetch_one(&state.pool)
            .await {
            Ok(count) => count,
            Err(e) => {
                warn!("Failed to count messages for conversation {}: {}", conversation_id, e);
                continue;
            }
        };

        if message_count > 0 {
            let fetch_event = json!({
                "type": "fetch_conversation_messages",
                "conversation_id": conversation_id,
                "message_count": message_count,
                "reason": "initial_fetch",
                "timestamp": chrono::Utc::now().timestamp()
            });

            if let Ok(txt) = serde_json::to_string(&fetch_event) {
                if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                    warn!("Failed to send fetch event to user {}", user_id);
                    break;
                } else {
                    events_sent += 1;
                    info!("Sent fetch event to user {} for conversation {} ({} messages)",
                          user_id, conversation_id, message_count);
                }
            }
        } else {
            debug!("Skipping empty conversation {} for user {}", conversation_id, user_id);
        }
    }

    if events_sent > 0 {
        info!("Sent {} fetch events for user {}", events_sent, user_id);
    }
}