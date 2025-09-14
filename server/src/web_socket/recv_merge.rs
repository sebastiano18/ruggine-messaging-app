use futures::{StreamExt, stream::select_all};
use serde_json::{Value, json};
use tokio::{
    select,
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn};
use uuid::Uuid;

use super::actor::OutboundMsg;
use crate::{error::Result, state::AppState, error::AppError};

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

    let mut combined = select_all(
        initial_channels
            .into_iter()
            .map(|(_, tx)| BroadcastStream::new(tx.subscribe())),
    );



    Ok(tokio::spawn(async move {
        loop {
            select! {
                _ = stop_rx.changed() => {
                    info!("Stop signal received for recv-merge user {}", user_id);
                    break;
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
                            // Quando tutti gli stream finiscono, refresh E controlla per nuove conversazioni
                            warn!("All streams ended for user {}, refreshing", user_id);
                            match refresh_user_channels_with_fetch(&state, user_id, &mut combined, &out_tx).await {
                                Ok(true) => {
                                    info!("Successfully refreshed streams for user {}", user_id);
                                    continue;
                                }
                                Ok(false) => {
                                    info!("No conversations available for user {}", user_id);
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
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

/// Refresh canali E invia fetch events solo per NUOVE conversazioni con messaggi
async fn refresh_user_channels_with_fetch(
    state: &AppState,
    user_id: Uuid,
    combined: &mut futures::stream::SelectAll<BroadcastStream<Value>>,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<bool> {

    let user_channels = state
        .get_user_channels(user_id)
        .await
        .map_err(crate::error::AppError::from)?;

    if user_channels.is_empty() {
        return Ok(false);
    }

    let conversation_ids: Vec<Uuid> = user_channels.iter().map(|(conv_id, _)| *conv_id).collect();


    let new_receivers: Vec<BroadcastStream<Value>> = user_channels
        .into_iter()
        .map(|(_, tx)| BroadcastStream::new(tx.subscribe()))
        .collect();

    let receiver_count = new_receivers.len();
    *combined = select_all(new_receivers);

    info!("Refreshed channels for user {} - {} active channels", user_id, receiver_count);


    send_fetch_events_for_conversations(state, user_id, &conversation_ids, out_tx).await;

    Ok(true)
}

/// Ottieni IDs delle conversazioni attuali dell'utente
async fn get_user_conversation_ids(state: &AppState, user_id: Uuid) -> Result<Vec<Uuid>> {
    let user_id_str = user_id.to_string();

    let conversation_ids = sqlx::query_scalar::<_, String>(
        "SELECT conversation_id FROM participants WHERE user_id = ?"
    )
        .bind(&user_id_str)
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let parsed_ids: Vec<Uuid> = conversation_ids
        .into_iter()
        .filter_map(|id_str| Uuid::parse_str(&id_str).ok())
        .collect();

    Ok(parsed_ids)
}

/// Invia fetch events per conversazioni specifiche che hanno messaggi
async fn send_fetch_events_for_conversations(
    state: &AppState,
    user_id: Uuid,
    conversation_ids: &[Uuid],
    out_tx: &mpsc::Sender<OutboundMsg>,
) {
    info!("Checking {} conversations for messages to fetch", conversation_ids.len());

    for &conversation_id in conversation_ids {
        let conversation_id_str = conversation_id.to_string();

        // Conta messaggi nella conversazione
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

        // Invia fetch event solo se ci sono messaggi
        if message_count > 0 {
            let fetch_event = json!({
                "type": "fetch_conversation_messages",
                "conversation_id": conversation_id,
                "reason": "new_conversation_detected",
                "timestamp": chrono::Utc::now().timestamp()
            });

            if let Ok(txt) = serde_json::to_string(&fetch_event) {
                if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                    warn!("Failed to send fetch event to user {}", user_id);
                    break;
                } else {
                    info!("Sent fetch event to user {} for new conversation {} ({} messages)", 
                          user_id, conversation_id, message_count);
                }
            }
        } else {
            info!("Skipping empty conversation {} for user {}", conversation_id, user_id);
        }
    }
}