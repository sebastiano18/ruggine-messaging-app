use tokio::{select, task::JoinHandle, sync::{mpsc, watch}, time::{interval, Duration}};
use tokio_stream::wrappers::BroadcastStream;
use futures::{StreamExt, stream::select_all};
use serde_json::Value;
use tracing::{error, info, warn};
use uuid::Uuid;
use std::collections::HashSet;

use crate::{state::AppState, error::Result};
use super::{actor::OutboundMsg, helpers::get_user_conversation_receivers};

pub async fn spawn_receiver(
    state: AppState,
    user_id: Uuid,
    out_tx: mpsc::Sender<OutboundMsg>,
    stop_tx: watch::Sender<bool>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<JoinHandle<()>> {
    // Setup iniziale
    let receivers = get_user_conversation_receivers(&state, user_id).await?;
    let mut combined = select_all(receivers.into_iter().map(BroadcastStream::new));

    // Track delle conversazioni correnti per rilevare cambiamenti
    let mut current_conversations = HashSet::new();

    Ok(tokio::spawn(async move {
        // Timer per refresh periodico delle conversazioni
        let mut refresh_interval = interval(Duration::from_secs(60));

        loop {
            select! {
                _ = stop_rx.changed() => {
                    info!("Stop signal received for recv-merge user {}", user_id);
                    break;
                }
                
                // Refresh periodico delle conversazioni dell'utente
                _ = refresh_interval.tick() => {
                    match refresh_user_conversations(&state, user_id, &mut combined, &mut current_conversations).await {
                        Ok(changed) => {
                            if changed {
                                info!("Updated conversation list for user {}", user_id);
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
                            // Invia notifica al client che ha perso messaggi
                            let lag_notice = format!(
                                r#"{{"type":"system","message":"Missed {} messages due to slow connection"}}"#, 
                                skipped
                            );
                            let _ = out_tx.send(OutboundMsg::Text(lag_notice)).await;
                            continue;
                        }
                        None => {
                            // Uno stream è terminato, ma potremmo averne altri
                            // Invece di terminare, facciamo un refresh
                            warn!("Stream ended for user {}, attempting refresh", user_id);
                            match refresh_user_conversations(&state, user_id, &mut combined, &mut current_conversations).await {
                                Ok(_) => continue,
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

async fn refresh_user_conversations(
    state: &AppState,
    user_id: Uuid,
    combined: &mut futures::stream::SelectAll<BroadcastStream<Value>>,
    current_conversations: &mut HashSet<Uuid>,
) -> Result<bool> {
    // Ottieni le conversazioni correnti dell'utente
    let conv_ids: Vec<String> = sqlx::query_scalar(
        r#"SELECT conversation_id FROM participants WHERE user_id = ?"#,
    )
        .bind(user_id.to_string())
        .fetch_all(&state.pool)
        .await
        .map_err(crate::error::AppError::from)?;

    let new_conversations: HashSet<Uuid> = conv_ids
        .into_iter()
        .filter_map(|s| Uuid::parse_str(&s).ok())
        .collect();

    // Controlla se ci sono cambiamenti
    if new_conversations == *current_conversations {
        return Ok(false); // Nessun cambiamento
    }

    info!("Conversation list changed for user {}: {:?} -> {:?}", 
          user_id, current_conversations, new_conversations);

    // Ricostruisci completamente il select_all con le nuove conversazioni
    let mut new_receivers = Vec::new();
    {
        let mut channels = state.channels.write().await;
        for conv_id in &new_conversations {
            let tx = channels.entry(*conv_id)
                .or_insert_with(|| {
                    let (tx, _rx) = tokio::sync::broadcast::channel::<Value>(1024);
                    tx
                })
                .clone();
            new_receivers.push(BroadcastStream::new(tx.subscribe()));
        }
    }

    // Sostituisci il select_all esistente
    *combined = select_all(new_receivers);
    *current_conversations = new_conversations;

    Ok(true) // Cambiamento avvenuto
}