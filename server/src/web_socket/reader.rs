// imports necessari in cima al file:
use axum::extract::ws::{Message, WebSocket};
use futures::{StreamExt, stream::SplitStream};
use serde_json::{json, Value};
use tokio::sync::{mpsc, watch};
use tracing::{error, info};
use uuid::Uuid;

use crate::{state::AppState};
use super::{actor::OutboundMsg, helpers::handle_chat_message};

pub fn spawn_reader(
    mut ws_rx: SplitStream<WebSocket>,         // <- mut perché .next() richiede &mut self
    state: AppState,
    user_id: Uuid,
    username: String,
    out_tx: mpsc::Sender<OutboundMsg>,
    stop_tx: watch::Sender<bool>,
    mut stop_rx: watch::Receiver<bool>,        // <- mut perché .has_changed() richiede &mut self
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let mut value = match serde_json::from_str::<Value>(&text) {
                        Ok(mut v) => {
                            v["author_id"] = Value::String(user_id.to_string());
                            v["author_username"] = Value::String(username.clone());
                            v
                        }
                        Err(_) => json!({
                            "type": "text",
                            "content": text,
                            "author_id": user_id.to_string(),
                            "author_username": username.clone()
                        }),
                    };

                    let kind = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if kind == "ping" {
                        let _ = out_tx.send(OutboundMsg::Pong(Vec::new())).await;
                        continue;
                    }
                    if matches!(kind, "pong" | "subscribe") {
                        continue;
                    }
                    if kind == "chat_message" {
                        if let Err(e) = handle_chat_message(&state, &mut value, user_id, &username).await {
                            error!("save chat error: {e}");
                            if let Ok(txt) = serde_json::to_string(&json!({"type":"error","message":"Failed to save message"})) {
                                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                            }
                            let _ = stop_tx.send(true);
                            break;
                        }
                    }
                    info!("inbound {:?} from {}", value, user_id);
                }
                Ok(Message::Close(frame)) => {
                    let _ = out_tx.send(OutboundMsg::Close(frame.map(|f| axum::extract::ws::CloseFrame {
                        code: f.code,
                        reason: f.reason.into_owned().into(),
                    }))).await;
                    let _ = stop_tx.send(true);
                    break;
                }
                Ok(Message::Ping(p))  => { let _ = out_tx.send(OutboundMsg::Pong(p)).await; }
                Ok(Message::Pong(_))  => {}
                Ok(Message::Binary(b))=> { let _ = out_tx.send(OutboundMsg::Binary(b)).await; }
                Err(e) => {
                    info!("client read err {}: {}", user_id, e);
                    let _ = stop_tx.send(true);
                    break;
                }
            }

            // uscita rapida se è arrivato lo stop
            if stop_rx.has_changed().unwrap_or(false) {
                break;
            }
        }
    })
}
