use axum::{
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{auth::AuthUser, state::AppState};

// Chiave fissa per il canale globale
const GLOBAL_CH_KEY: i64 = 0;

#[axum::debug_handler]
pub async fn ws_handler(
    auth_user: AuthUser,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    info!(
        "WebSocket connection authenticated for user: {} ({})",
        auth_user.username, auth_user.id
    );
    ws.on_upgrade(move |socket| ws_loop(socket, state, auth_user.id))
}

async fn ws_loop(socket: WebSocket, state: AppState, user_id: Uuid) {
    info!("WebSocket connection established for user {}", user_id);

    let tx = get_or_init_global_channel(&state.channels).await;
    let mut rx = tx.subscribe();
    let (mut ws_tx, mut ws_rx) = socket.split();

    // canale di coordinamento "stop"
    use tokio::sync::watch;
    let (stop_tx, mut stop_rx) = watch::channel(false);

    // Task: dal broadcast -> al WebSocket (si ferma se stop=true)
    let user_id_for_recv = user_id;
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
                            // FILTRO: Non inviare messaggi che ho mandato io stesso
                            if let Some(author_id) = val.get("author_id").and_then(|id| id.as_str()) {
                                if let Ok(author_uuid) = Uuid::parse_str(author_id) {
                                    if author_uuid == user_id_for_recv {
                                        continue; // Skip i miei messaggi
                                    }
                                }
                            }

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
        info!("Recv task ended for user {}", user_id_for_recv);
    });

    // Task: dal WebSocket -> al broadcast
    let mut send_task = {
        let tx = tx.clone();
        let stop_tx = stop_tx.clone();
        let user_id_for_send = user_id;
        tokio::spawn(async move {
            while let Some(msg_result) = ws_rx.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let mut value = match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(mut v) => {
                                // Aggiungi automaticamente author_id
                                v["author_id"] =
                                    serde_json::Value::String(user_id_for_send.to_string());
                                v
                            }
                            Err(_) => {
                                warn!("Invalid JSON received, treating as raw text");
                                serde_json::json!({
                                    "type": "text",
                                    "content": text,
                                    "author_id": user_id_for_send.to_string()
                                })
                            }
                        };

                        // non ribroadcastare messaggi di controllo
                        let is_control = value
                            .get("type")
                            .and_then(|t| t.as_str())
                            .map(|t| matches!(t, "subscribe" | "ping" | "pong"))
                            .unwrap_or(false);
                        if is_control {
                            continue;
                        }

                        info!(
                            "Broadcasting message from user {}: {}",
                            user_id_for_send, value
                        );

                        if let Err(e) = tx.send(value) {
                            warn!("Failed to broadcast message: {e}");
                            // segnala stop all'altro task
                            let _ = stop_tx.send(true);
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket connection closed by user {}", user_id_for_send);
                        // segnala stop all'altro task
                        let _ = stop_tx.send(true);
                        break;
                    }
                    Ok(Message::Ping(_)) => {
                        info!("Received ping from user {}", user_id_for_send); // axum risponde col pong
                    }
                    Ok(Message::Pong(_)) => {
                        info!("Received pong from user {}", user_id_for_send);
                    }
                    Ok(_) => {
                        warn!(
                            "Received unsupported message type from user {}",
                            user_id_for_send
                        );
                    }
                    Err(e) => {
                        // axum::Error: considera disconnect
                        info!(
                            "WS read error from user {} (client disconnected): {}",
                            user_id_for_send, e
                        );
                        let _ = stop_tx.send(true);
                        break;
                    }
                }
            }
            info!("Send task ended for user {}", user_id_for_send);
        })
    };

    // Attendi il primo che termina e interrompi l'altro per evitare write post-close
    tokio::select! {
        _ = &mut recv_task => {
            info!("WebSocket recv task completed for user {}", user_id);
            send_task.abort();
        }
        _ = &mut send_task => {
            info!("WebSocket send task completed for user {}", user_id);
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
    tracing::info!(
        "WS broadcast -> {} subscribers | payload={}",
        receivers_now,
        payload
    );

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
