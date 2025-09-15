use futures::{StreamExt, stream::select_all};
use serde_json::{Value, json};
use tokio::{
    select,
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Duration, sleep},
};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn, debug};
use uuid::Uuid;

use super::{actor::OutboundMsg, helpers::handle_user_notification};
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

    // DIAGNOSTICA: Verifica stato canali iniziali
    info!("User {} found {} initial conversation channels", user_id, initial_channels.len());
    for (conv_id, tx) in &initial_channels {
        let receiver_count = tx.receiver_count();
        info!("Conversation {} has {} active receivers before subscribe", conv_id, receiver_count);
    }

    // SEMPRE includi il canale utente per le notifiche
    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    let user_channel_stream = BroadcastStream::new(user_tx.subscribe());
    info!("User {} subscribed to notification channel", user_id);

    let mut combined = if initial_channels.is_empty() {
        // Solo canale utente se non ci sono conversazioni
        info!("User {} starting with notification channel only (no conversations)", user_id);
        select_all(vec![user_channel_stream])
    } else {
        info!("User {} subscribing to {} conversation channels + notification channel",
               user_id, initial_channels.len());

        // CRITICO: Crea receiver attivi per ogni canale conversazione
        let mut all_streams: Vec<BroadcastStream<Value>> = Vec::new();

        for (conv_id, tx) in initial_channels {
            let receiver = tx.subscribe(); // Crea receiver attivo
            all_streams.push(BroadcastStream::new(receiver));

            // Verifica che il receiver sia stato creato
            let new_receiver_count = tx.receiver_count();
            info!("User {} subscribed to conversation {} (now {} receivers)",
                  user_id, conv_id, new_receiver_count);
        }

        // Aggiungi canale utente
        all_streams.push(user_channel_stream);
        select_all(all_streams)
    };

    // Variabile per tracking del backoff quando non ci sono conversazioni
    let mut empty_backoff_seconds = 30u64;
    const MAX_BACKOFF_SECONDS: u64 = 300;
    let mut consecutive_none_count = 0u32;
    const MAX_CONSECUTIVE_NONE: u32 = 5;

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
                            // Reset contatori quando riceviamo messaggi validi
                            empty_backoff_seconds = 30;
                            consecutive_none_count = 0;

                            // Gestione speciale per notifiche dal canale utente
                            if let Some(msg_type) = val.get("type").and_then(|t| t.as_str()) {
                                if msg_type == "conversation_created" {
                                    debug!("Received conversation_created notification for user {}", user_id);

                                    // Gestisci la notifica tramite helper
                                    if let Err(e) = handle_user_notification(&state, &val, user_id, &out_tx).await {
                                        warn!("Failed to handle user notification for user {}: {}", user_id, e);
                                    }

                                    // CRITICO: Refresh per includere il nuovo canale conversazione
                                    info!("Refreshing channels after conversation_created for user {}", user_id);
                                    match refresh_user_channels_conservative(&state, user_id, &mut combined, &out_tx).await {
                                        Ok(RefreshResult::HasChannels(count)) => {
                                            info!("User {} refreshed after conversation_created: {} channels", user_id, count);
                                            empty_backoff_seconds = 30;
                                            consecutive_none_count = 0;
                                        }
                                        Ok(RefreshResult::NoChannels) => {
                                            info!("User {} still has no conversations after refresh", user_id);
                                        }
                                        Err(e) => {
                                            error!("Failed to refresh after conversation_created for user {}: {}", user_id, e);
                                        }
                                    }
                                    continue;
                                }
                            }

                            // Prevenzione echo al mittente (per messaggi normali)
                            if let Some(author) = val.get("author_id")
                                .and_then(|x| x.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok())
                            {
                                if author == user_id {
                                    debug!("Skipping echo message from same user {}", user_id);
                                    continue;
                                }
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
                            consecutive_none_count = 0; // Reset perché abbiamo ricevuto qualcosa

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
                            consecutive_none_count += 1;
                            warn!("Stream returned None for user {} (consecutive: {})", user_id, consecutive_none_count);

                            // Se tutti i stream sono chiusi, tenta un refresh immediato
                            match refresh_user_channels_conservative(&state, user_id, &mut combined, &out_tx).await {
                                Ok(RefreshResult::HasChannels(count)) => {
                                    info!("User {} refreshed to {} channels after None", user_id, count);
                                    consecutive_none_count = 0;
                                    continue;
                                }
                                Ok(RefreshResult::NoChannels) => {
                                    info!("User {} has no conversations after None, backing off for {} seconds",
                                           user_id, empty_backoff_seconds);
                                    sleep(Duration::from_secs(empty_backoff_seconds)).await;
                                    empty_backoff_seconds = (empty_backoff_seconds * 2).min(MAX_BACKOFF_SECONDS);
                                    consecutive_none_count = 0;
                                    continue;
                                }
                                Err(e) => {
                                    error!("Failed to refresh streams for user {}: {}", user_id, e);
                                    let _ = stop_tx.send(true);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        info!("recv-merge ended for user {}", user_id);
    }))
}

#[derive(Debug)]
enum RefreshResult {
    HasChannels(usize),
    NoChannels,
}

/// Refresh conservativo che preserva i receiver attivi quando possibile
async fn refresh_user_channels_conservative(
    state: &AppState,
    user_id: Uuid,
    combined: &mut futures::stream::SelectAll<BroadcastStream<Value>>,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<RefreshResult> {

    info!("Starting conservative refresh for user {}", user_id);

    let user_channels = state
        .get_user_channels(user_id)
        .await
        .map_err(crate::error::AppError::from)?;

    // SEMPRE includi il canale utente
    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    let user_channel_stream = BroadcastStream::new(user_tx.subscribe());
    info!("User {} re-subscribed to notification channel during conservative refresh", user_id);

    let mut new_receivers: Vec<BroadcastStream<Value>> = vec![user_channel_stream];

    if user_channels.is_empty() {
        info!("User {} has no conversations during refresh", user_id);
        *combined = select_all(new_receivers);
        return Ok(RefreshResult::NoChannels);
    }

    let conversation_ids: Vec<Uuid> = user_channels.iter().map(|(conv_id, _)| *conv_id).collect();

    // Crea receiver attivi per ogni canale conversazione
    for (conv_id, tx) in user_channels {
        let receiver = tx.subscribe(); // Crea nuovo receiver attivo
        new_receivers.push(BroadcastStream::new(receiver));

        let receiver_count = tx.receiver_count();
        info!("User {} re-subscribed to conversation {} during conservative refresh (now {} receivers)",
              user_id, conv_id, receiver_count);
    }

    let total_channels = new_receivers.len();
    *combined = select_all(new_receivers);

    info!("User {} conservative refresh completed: {} total channels", user_id, total_channels);

    // NON inviare fetch events durante refresh conservativo - mantieni stato esistente
    debug!("Skipping fetch events during conservative refresh for user {}", user_id);

    Ok(RefreshResult::HasChannels(total_channels))
}

/// Utilizzato solo per refresh espliciti (nuove conversazioni)
pub async fn force_refresh_with_fetch(
    state: &AppState,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) {
    info!("Force refresh with fetch for user {}", user_id);

    let user_channels = match state.get_user_channels(user_id).await {
        Ok(channels) => channels,
        Err(e) => {
            warn!("Failed to get channels for force refresh user {}: {}", user_id, e);
            return;
        }
    };

    let conversation_ids: Vec<Uuid> = user_channels.iter().map(|(conv_id, _)| *conv_id).collect();
    send_fetch_events_for_conversations(state, user_id, &conversation_ids, out_tx).await;
}

/// Invia fetch events per conversazioni specifiche che hanno messaggi
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
                "message_count": message_count,
                "reason": "explicit_refresh",
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
        info!("Sent {} fetch events during explicit refresh for user {}", events_sent, user_id);
    }
}