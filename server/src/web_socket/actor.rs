use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::{
    select,
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Duration, interval, timeout},
};
use tracing::{error, info, warn};
use uuid::Uuid;

use super::{
    helpers::{cleanup_empty_channels, send_initial_fetch_events},
    reader::spawn_reader,
    recv_merge::spawn_receiver
};
use crate::{error::Result, state::AppState};

#[derive(Debug)]
pub enum OutboundMsg {
    Text(String),
    Binary(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<axum::extract::ws::CloseFrame<'static>>),
}

pub struct ConnectionActor;

impl ConnectionActor {
    pub async fn start(
        socket: WebSocket,
        state: AppState,
        user_id: Uuid,
        username: String,
    ) -> Result<()> {
        let (mut ws_tx, ws_rx) = socket.split();

        // Coordinamento shutdown + coda bounded verso l'unico writer
        let (stop_tx, stop_rx) = watch::channel(false);
        let (out_tx, mut out_rx) = mpsc::channel::<OutboundMsg>(1024);

        // Clone dedicato del receiver per il writer
        let mut stop_rx_writer = stop_rx.clone();

        // NUOVO: Auto-subscribe al canale utente per notifiche
        // Questo assicura che l'utente riceva sempre le notifiche di nuove conversazioni
        let _user_tx = state.get_or_create_user_notification_channel(user_id).await;
        info!("Auto-subscribed user {} to their notification channel", user_id);

        // Invia conferma di iscrizione al canale utente
        let user_channel_ready = json!({
            "type": "user_channel_ready",
            "message": "Subscribed to user notification channel",
            "user_id": user_id,
            "timestamp": chrono::Utc::now().timestamp()
        });

        if let Ok(txt) = serde_json::to_string(&user_channel_ready) {
            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
        }

        // Writer: unico proprietario di ws_tx con gestione degli errori migliorata
        let mut writer: JoinHandle<()> = tokio::spawn(async move {
            // Heartbeat timer con jitter per evitare thundering herd
            let heartbeat_base_interval = Duration::from_secs(30);
            let jitter = Duration::from_millis(fastrand::u64(0..5000)); // 0-5s di jitter
            let mut heartbeat_interval = interval(heartbeat_base_interval + jitter);

            let mut consecutive_failures = 0u32;
            const MAX_CONSECUTIVE_FAILURES: u32 = 3;

            info!("Writer task started for user {}", user_id);

            loop {
                select! {
                    _ = stop_rx_writer.changed() => {
                        info!("Writer received stop signal for user {}", user_id);
                        // Graceful close con timeout
                        let close_result = timeout(
                            Duration::from_secs(5),
                            ws_tx.send(Message::Close(None))
                        ).await;

                        if close_result.is_err() {
                            warn!("Close message timeout for user {}", user_id);
                        }
                        break;
                    }
                    maybe_msg = out_rx.recv() => {
                        let Some(msg) = maybe_msg else {
                            info!("Output channel closed for user {}", user_id);
                            break;
                        };

                        let to_send = match msg {
                            OutboundMsg::Text(s)   => Message::Text(s),
                            OutboundMsg::Binary(b) => Message::Binary(b),
                            OutboundMsg::Pong(b)   => Message::Pong(b),
                            OutboundMsg::Close(f)  => Message::Close(f),
                        };

                        // Send con timeout per evitare blocchi
                        let send_result = timeout(
                            Duration::from_secs(10),
                            ws_tx.send(to_send)
                        ).await;

                        match send_result {
                            Ok(Ok(_)) => {
                                consecutive_failures = 0; // Reset counter su successo
                            }
                            Ok(Err(e)) => {
                                consecutive_failures += 1;
                                warn!("WebSocket send error for user {} (failure #{}: {})",
                                     user_id, consecutive_failures, e);

                                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                    error!("Too many consecutive failures for user {}, closing connection", user_id);
                                    break;
                                }
                            }
                            Err(_) => {
                                consecutive_failures += 1;
                                warn!("WebSocket send timeout for user {} (failure #{})",
                                     user_id, consecutive_failures);

                                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                    error!("Too many consecutive timeouts for user {}, closing connection", user_id);
                                    break;
                                }
                            }
                        }
                    }
                    _ = heartbeat_interval.tick() => {
                        let heartbeat = json!({
                            "type": "server_heartbeat",
                            "timestamp": chrono::Utc::now().timestamp(),
                            "user_id": user_id
                        });

                        if let Ok(txt) = serde_json::to_string(&heartbeat) {
                            let send_result = timeout(
                                Duration::from_secs(5),
                                ws_tx.send(Message::Text(txt))
                            ).await;

                            if send_result.is_err() {
                                warn!("Heartbeat send failed/timeout for user {}", user_id);
                                consecutive_failures += 1;

                                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                                    error!("Heartbeat failures exceeded limit for user {}", user_id);
                                    break;
                                }
                            } else {
                                consecutive_failures = 0;
                            }
                        }
                    }
                }
            }
            info!("Writer task ended for user {}", user_id);
        });

        // Invia fetch events iniziali per tutte le conversazioni con messaggi
        // Fallo PRIMA di avviare il receiver per evitare race conditions
        if let Err(e) = send_initial_fetch_events(&state, user_id, &out_tx).await {
            warn!("Failed to send initial fetch events for user {}: {}", user_id, e);
        } else {
            info!("Successfully sent initial fetch events for user {}", user_id);
        }

        // Receiver: merge dei broadcast delle conversazioni (server -> client)
        let recv_task_result = spawn_receiver(
            state.clone(),
            user_id,
            out_tx.clone(),
            stop_tx.clone(),
            stop_rx.clone(),
        )
            .await;

        let mut recv_task = match recv_task_result {
            Ok(task) => task,
            Err(e) => {
                error!("Failed to spawn receiver for user {}: {}", user_id, e);
                let _ = stop_tx.send(true);
                writer.abort();
                return Err(e);
            }
        };

        // Reader: input client -> valida/salva/broadcast -> risposte via out_tx
        let mut reader_task = spawn_reader(
            ws_rx,
            state.clone(),
            user_id,
            username,
            out_tx.clone(),
            stop_tx.clone(),
            stop_rx.clone(),
        );

        // Orchestrazione con gestione migliorata degli errori
        let connection_result = select! {
            reader_result = &mut reader_task => {
                info!("Reader task completed for user {}", user_id);
                let _ = stop_tx.send(true);
                recv_task.abort();
                writer.abort();
                reader_result
            }
            recv_result = &mut recv_task => {
                info!("Receiver task completed for user {}", user_id);
                let _ = stop_tx.send(true);
                reader_task.abort();
                writer.abort();
                recv_result
            }
            writer_result = &mut writer => {
                info!("Writer task completed for user {}", user_id);
                reader_task.abort();
                recv_task.abort();
                writer_result
            }
        };

        // Cleanup finale con timeout
        let cleanup_future = cleanup_empty_channels(&state, user_id);
        if timeout(Duration::from_secs(5), cleanup_future)
            .await
            .is_err()
        {
            warn!("Cleanup timeout for user {}", user_id);
        }

        // Log delle statistiche finali
        let (total_channels, total_receivers) = state.get_channel_stats().await;
        info!(
            "Connection fully closed for user {} (system: {} channels, {} receivers)",
            user_id, total_channels, total_receivers
        );

        // Propaga eventuali errori dai task
        if let Err(e) = connection_result {
            warn!("Connection ended with error for user {}: {:?}", user_id, e);
        }

        Ok(())
    }
}