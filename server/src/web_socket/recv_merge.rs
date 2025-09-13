use futures::{StreamExt, stream::select_all};
use serde_json::Value;
use std::collections::HashSet;
use tokio::{
    select,
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Duration, interval},
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::actor::OutboundMsg;
use crate::{error::Result, state::AppState};

pub async fn spawn_receiver(
    state: AppState,
    user_id: Uuid,
    out_tx: mpsc::Sender<OutboundMsg>,
    stop_tx: watch::Sender<bool>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<JoinHandle<()>> {
    // Setup iniziale con il nuovo metodo thread-safe
    let initial_channels = state
        .get_user_channels(user_id)
        .await
        .map_err(|e| crate::error::AppError::from(e))?;

    let mut combined = select_all(
        initial_channels
            .into_iter()
            .map(|(_, tx)| BroadcastStream::new(tx.subscribe())),
    );

    // Track delle conversazioni correnti per rilevare cambiamenti
    let mut current_conversations = HashSet::new();

    Ok(tokio::spawn(async move {
        // Timer più frequente per rilevare velocemente nuove conversazioni
        let mut refresh_interval = interval(Duration::from_secs(10));

        loop {
            select! {
                _ = stop_rx.changed() => {
                    info!("Stop signal received for recv-merge user {}", user_id);
                    break;
                }

                // Refresh più frequente per rilevare cambiamenti nelle conversazioni
                _ = refresh_interval.tick() => {
                    match refresh_user_conversations_improved(&state, user_id, &mut combined, &mut current_conversations).await {
                        Ok(changed) => {
                            if changed {
                                info!("Updated conversation list for user {} ({} conversations)",
                                     user_id, current_conversations.len());
                            }
                        }
                        Err(e) => {
                            error!("Failed to refresh conversations for user {}: {}", user_id, e);
                        }
                    }
                }

                item = combined.next() => {
                    match item {
                        Some(Ok(val)) => {
                            // Prevenzione echo al mittente
                            if let Some(author) = val.get("author_id")
                                .and_then(|x| x.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok())
                            {
                                if author == user_id { continue; }
                            }

                            match serde_json::to_string(&val) {
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
                        Some(Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped))) => {
                            warn!("User {} lagged behind, skipped {} messages", user_id, skipped);
                            let lag_notice = format!(
                                r#"{{"type":"system","message":"Missed {} messages due to slow connection","skipped":{}}}"#,
                                skipped, skipped
                            );
                            if out_tx.send(OutboundMsg::Text(lag_notice)).await.is_err() {
                                warn!("Failed to send lag notice to user {}", user_id);
                                let _ = stop_tx.send(true);
                                break;
                            }
                            continue;
                        }
                        None => {
                            // Tutti gli stream sono terminati
                            warn!("All streams ended for user {}, attempting refresh", user_id);
                            match refresh_user_conversations_improved(&state, user_id, &mut combined, &mut current_conversations).await {
                                Ok(true) => {
                                    info!("Successfully refreshed streams for user {}", user_id);
                                    continue;
                                }
                                Ok(false) => {
                                    info!("No conversations available for user {}", user_id);
                                    tokio::time::sleep(Duration::from_secs(2)).await;
                                    continue;
                                }
                                Err(e) => {
                                    error!("Failed to recover streams for user {}: {}", user_id, e);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        info!("recv-merge end for {}", user_id);
    }))
}

/// Versione migliorata che rileva automaticamente nuove conversazioni
async fn refresh_user_conversations_improved(
    state: &AppState,
    user_id: Uuid,
    combined: &mut futures::stream::SelectAll<BroadcastStream<Value>>,
    current_conversations: &mut HashSet<Uuid>,
) -> Result<bool> {
    // Usa il metodo thread-safe per ottenere i canali
    let user_channels = state
        .get_user_channels(user_id)
        .await
        .map_err(crate::error::AppError::from)?;

    let new_conversations: HashSet<Uuid> =
        user_channels.iter().map(|(conv_id, _)| *conv_id).collect();

    // Controlla se ci sono cambiamenti
    if new_conversations == *current_conversations {
        return Ok(false); // Nessun cambiamento
    }

    // Log dettagliato delle conversazioni aggiunte/rimosse
    let added: Vec<_> = new_conversations.difference(current_conversations).collect();
    let removed: Vec<_> = current_conversations.difference(&new_conversations).collect();

    if !added.is_empty() {
        info!("User {} automatically joined new conversations: {:?}", user_id, added);

        // Invia notifica per ogni nuova conversazione
        for &conv_id in &added {
            let notification = serde_json::json!({
                "type": "new_conversation_available",
                "conversation_id": conv_id,
                "message": "Nuova conversazione disponibile",
                "timestamp": chrono::Utc::now().timestamp(),
            });

            // Trova il canale per questa conversazione e invia la notifica
            if let Some((_, tx)) = user_channels.iter().find(|(id, _)| *id == *conv_id) {
                let _ = tx.send(notification); // Best effort, non bloccare se fallisce
            }
        }
    }

    if !removed.is_empty() {
        info!("User {} left conversations: {:?}", user_id, removed);
    }

    info!(
        "Conversation list changed for user {}: {} -> {} conversations",
        user_id,
        current_conversations.len(),
        new_conversations.len()
    );

    // Crea i nuovi receiver - i canali sono già stati creati/ottenuti in modo thread-safe
    let new_receivers: Vec<BroadcastStream<Value>> = user_channels
        .into_iter()
        .map(|(_, tx)| BroadcastStream::new(tx.subscribe()))
        .collect();

    // Sostituisci atomicamente il select_all con i nuovi stream
    *combined = select_all(new_receivers);
    *current_conversations = new_conversations;

    Ok(true) // Cambiamento avvenuto
}

/// Funzione helper per il monitoring dello stato dei receiver
#[allow(dead_code)]
async fn log_receiver_stats(
    state: &AppState,
    user_id: Uuid,
    current_conversations: &HashSet<Uuid>,
) {
    let (total_channels, total_receivers) = state.get_channel_stats().await;
    info!(
        "User {} stats: tracking {} conversations, system has {} channels with {} total receivers",
        user_id,
        current_conversations.len(),
        total_channels,
        total_receivers
    );
}