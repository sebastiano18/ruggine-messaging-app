use std::{collections::HashMap, sync::Arc};
use axum::{
    extract::{Query, State},
    response::IntoResponse,
};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct WsParams {
    pub conversation_id: i64,
}

#[axum::debug_handler]
pub async fn ws_handler(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_loop(socket, state, params.conversation_id))
}

async fn ws_loop(socket: WebSocket, state: AppState, conversation_id: i64) {
    info!("WebSocket connection established for conversation {}", conversation_id);

    let tx = get_or_init_channel(&state.channels, conversation_id).await;
    let mut rx = tx.subscribe();
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Task per ricevere messaggi dal broadcast channel e inviarli al WebSocket
    let recv_task = tokio::spawn(async move {
        while let Ok(val) = rx.recv().await {
            match serde_json::to_string(&val) {
                Ok(msg) => {
                    if let Err(e) = ws_tx.send(Message::Text(msg)).await {
                        warn!("Failed to send WebSocket message: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    let error_msg = r#"{"type":"error","message":"serialization_failed"}"#;
                    if ws_tx.send(Message::Text(error_msg.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
        info!("Recv task ended for conversation {}", conversation_id);
    });

    // Task per ricevere messaggi dal WebSocket e inviarli al broadcast channel
    let send_task = {
        let tx = tx.clone();
        let conversation_id = conversation_id;
        tokio::spawn(async move {
            while let Some(msg_result) = ws_rx.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let value = match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(v) => v,
                            Err(_) => {
                                warn!("Invalid JSON received, treating as raw text");
                                serde_json::json!({
                                    "type": "text",
                                    "content": text
                                })
                            }
                        };

                        if let Err(e) = tx.send(value) {
                            warn!("Failed to broadcast message: {}", e);
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket connection closed gracefully");
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        // Echo back pong - questo dovrebbe essere gestito automaticamente da axum
                        info!("Received ping");
                    }
                    Ok(Message::Pong(_)) => {
                        // Pong ricevuto
                        info!("Received pong");
                    }
                    Ok(_) => {
                        // Altri tipi di messaggi (Binary, etc.)
                        warn!("Received unsupported message type");
                    }
                    Err(e) => {
                        warn!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
            info!("Send task ended for conversation {}", conversation_id);
        })
    };

    // Aspetta che uno dei due task finisca
    tokio::select! {
        _ = recv_task => {
            info!("WebSocket recv task completed for conversation {}", conversation_id);
        },
        _ = send_task => {
            info!("WebSocket send task completed for conversation {}", conversation_id);
        },
    }

    // Opzionale: cleanup della channel se non ci sono più subscriber
    cleanup_channel_if_empty(&state.channels, conversation_id).await;
}

async fn get_or_init_channel(
    channels: &Arc<RwLock<HashMap<i64, broadcast::Sender<serde_json::Value>>>>,
    conversation_id: i64,
) -> broadcast::Sender<serde_json::Value> {
    // Prima prova a leggere
    {
        let map = channels.read().await;
        if let Some(tx) = map.get(&conversation_id) {
            return tx.clone();
        }
    }

    // Se non esiste, acquisisci il write lock
    let mut map = channels.write().await;
    // Double-check pattern: qualcun altro potrebbe aver creato la channel nel frattempo
    if let Some(tx) = map.get(&conversation_id) {
        return tx.clone();
    }

    // Crea una nuova channel
    let (tx, _) = broadcast::channel(256);
    map.insert(conversation_id, tx.clone());
    info!("Created new broadcast channel for conversation {}", conversation_id);
    tx
}

// Funzione opzionale per fare cleanup delle channel inutilizzate
async fn cleanup_channel_if_empty(
    channels: &Arc<RwLock<HashMap<i64, broadcast::Sender<serde_json::Value>>>>,
    conversation_id: i64,
) {
    let should_remove = {
        let map = channels.read().await;
        if let Some(tx) = map.get(&conversation_id) {
            tx.receiver_count() == 0
        } else {
            false
        }
    };

    if should_remove {
        let mut map = channels.write().await;
        // Double-check: la situazione potrebbe essere cambiata
        if let Some(tx) = map.get(&conversation_id) {
            if tx.receiver_count() == 0 {
                map.remove(&conversation_id);
                info!("Cleaned up unused broadcast channel for conversation {}", conversation_id);
            }
        }
    }
}