use std::{collections::HashMap, sync::Arc};
use axum::{
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::state::AppState;

// Chiave fissa per il canale globale
const GLOBAL_CH_KEY: i64 = 0;

#[axum::debug_handler]
pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_loop(socket, state))
}

async fn ws_loop(socket: WebSocket, state: AppState) {
    info!("WebSocket connection established (global channel)");

    let tx = get_or_init_global_channel(&state.channels).await;
    let mut rx = tx.subscribe();
    let (mut ws_tx, mut ws_rx) = socket.split();

    // canale di coordinamento "stop"
    use tokio::sync::watch;
    let (stop_tx, mut stop_rx) = watch::channel(false);

    // Task: dal broadcast -> al WebSocket (si ferma se stop=true)
    let mut recv_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_rx.changed() => {
                    // l'altro task ha segnalato chiusura
                    break;
                }
                val = rx.recv() => {
                    match val {
                        Ok(val) => {
                            match serde_json::to_string(&val) {
                                Ok(msg) => {
                                    if let Err(e) = ws_tx.send(Message::Text(msg)).await {
                                        // se fallisce l'invio, chiudi
                                        warn!("Failed to send WebSocket message: {e}");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to serialize message: {e}");
                                    let error_msg = r#"{"type":"error","message":"serialization_failed"}"#;
                                    if ws_tx.send(Message::Text(error_msg.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            // broadcast chiuso
                            break;
                        }
                    }
                }
            }
        }
        info!("Recv task ended (global channel)");
    });

    // Task: dal WebSocket -> al broadcast
    let mut send_task = {
        let tx = tx.clone();
        let stop_tx = stop_tx.clone();
        tokio::spawn(async move {
            while let Some(msg_result) = ws_rx.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let mut value = match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(v) => v,
                            Err(_) => {
                                warn!("Invalid JSON received, treating as raw text");
                                serde_json::json!({ "type": "text", "content": text })
                            }
                        };

                        // non ribroadcastare messaggi di controllo
                        let is_control = value.get("type")
                            .and_then(|t| t.as_str())
                            .map(|t| matches!(t, "subscribe" | "ping" | "pong"))
                            .unwrap_or(false);
                        if is_control {
                            continue;
                        }

                        if let Err(e) = tx.send(value) {
                            warn!("Failed to broadcast message: {e}");
                            // segnala stop all'altro task
                            let _ = stop_tx.send(true);
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket connection closed by client");
                        // segnala stop all'altro task
                        let _ = stop_tx.send(true);
                        break;
                    }
                    Ok(Message::Ping(_)) => {
                        info!("Received ping"); // axum risponde col pong
                    }
                    Ok(Message::Pong(_)) => {
                        info!("Received pong");
                    }
                    Ok(_) => {
                        warn!("Received unsupported message type");
                    }
                    Err(e) => {
                        // axum::Error: considera disconnect
                        info!("WS read error (client disconnected): {}", e);
                        let _ = stop_tx.send(true);
                        break;
                    }
                }
            }
            info!("Send task ended (global channel)");
        })
    };

    // Attendi il primo che termina e interrompi l'altro per evitare write post-close
    tokio::select! {
        _ = &mut recv_task => {
            info!("WebSocket recv task completed (global)");
            send_task.abort();
        }
        _ = &mut send_task => {
            info!("WebSocket send task completed (global)");
            recv_task.abort();
        }
    }

    cleanup_global_if_empty(&state.channels).await;
}


async fn get_or_init_global_channel(
    channels: &Arc<RwLock<HashMap<i64, broadcast::Sender<serde_json::Value>>>>,
) -> broadcast::Sender<serde_json::Value> {
    // Prova read-lock
    {
        let map = channels.read().await;
        if let Some(tx) = map.get(&GLOBAL_CH_KEY) {
            return tx.clone();
        }
    }

    // Write-lock e double-check
    let mut map = channels.write().await;
    if let Some(tx) = map.get(&GLOBAL_CH_KEY) {
        return tx.clone();
    }

    let (tx, _) = broadcast::channel(256);
    map.insert(GLOBAL_CH_KEY, tx.clone());
    info!("Created global broadcast channel");
    tx
}

async fn cleanup_global_if_empty(
    channels: &Arc<RwLock<HashMap<i64, broadcast::Sender<serde_json::Value>>>>,
) {
    let should_remove = {
        let map = channels.read().await;
        map.get(&GLOBAL_CH_KEY)
            .map(|tx| tx.receiver_count() == 0)
            .unwrap_or(false)
    };

    if should_remove {
        let mut map = channels.write().await;
        if let Some(tx) = map.get(&GLOBAL_CH_KEY) {
            if tx.receiver_count() == 0 {
                map.remove(&GLOBAL_CH_KEY);
                info!("Cleaned up global broadcast channel (no subscribers)");
            }
        }
    }
}
pub async fn ws_broadcast(state: &crate::state::AppState, value: serde_json::Value) {
    let tx = get_or_init_global_channel(&state.channels).await;

    // (facoltativo) per log leggibile
    let payload = match serde_json::to_string(&value) {
        Ok(s) => s,
        Err(_) => "<non-serializzabile>".into(),
    };

    let receivers_now = tx.receiver_count();
    tracing::info!("WS broadcast -> {} subscribers | payload={}", receivers_now, payload);

    match tx.send(value) {
        Ok(delivered) => {
            // delivered = numero di subscriber attivi che riceveranno questo messaggio
            tracing::info!("WS broadcast delivered to {} subscribers", delivered);
        }
        Err(e) => {
            // errore tipicamente quando non ci sono subscriber
            tracing::warn!("WS broadcast dropped (no subscribers?): {}", e);
        }
    }
}

