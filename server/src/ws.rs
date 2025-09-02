use std::sync::Arc;

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use axum_extra::TypedHeader;
use futures::{SinkExt, StreamExt};
use headers::Authorization;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc, RwLock},
    task::JoinHandle,
};

use crate::{api::AppState, auth};

pub async fn ws_handler(
    State(state): State<Arc<AppState>>,
    // Richiede axum-extra = { version = "0.9", features = ["typed-header"] }
    TypedHeader(Authorization(bearer)):
    TypedHeader<headers::authorization::Authorization<headers::authorization::Bearer>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let claims = auth::decode_jwt(bearer.token(), &state.jwt_secret)
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;
    Ok::<_, axum::http::StatusCode>(ws.on_upgrade(move |socket| handle_socket(socket, state, claims.sub)))
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum InMsg {
    #[serde(rename = "subscribe")]
    Subscribe { conversations: Vec<i64> },
    #[serde(rename = "send")]
    Send { cid: i64, tmp_id: Option<String>, body: String },
    #[serde(rename = "read")]
    Read { cid: i64, msg_id: i64 },
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, user_id: i64) {
    // Canale centrale su cui convergono tutti gli eventi dei broadcast (dalle conversazioni sottoscritte)
    let (evt_tx, mut evt_rx) = mpsc::unbounded_channel::<serde_json::Value>();

    // Tracciamo i task che inoltrano dai broadcast channel -> mpsc; così possiamo abortirli al resubscribe
    let mut sub_tasks: Vec<JoinHandle<()>> = Vec::new();

    // Utility: avvia forwarding da un broadcast::Receiver verso evt_tx
    let spawn_forwarder = |mut rx: broadcast::Receiver<serde_json::Value>, tx: mpsc::UnboundedSender<serde_json::Value>| {
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(evt) => { let _ = tx.send(evt); }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    };

    // Loop principale: ascolta sia i messaggi del client sia gli eventi delle conversazioni
    loop {
        tokio::select! {
            // Messaggi in arrivo dal client WS
            next = socket.next() => {
                let Some(Ok(Message::Text(txt))) = next else {
                    // socket chiuso o errore
                    break;
                };

                match serde_json::from_str::<InMsg>(&txt) {
                    Ok(InMsg::Subscribe { conversations }) => {
                        // Annulla i forwarder precedenti
                        for h in sub_tasks.drain(..) { h.abort(); }

                        // Per ogni conversazione, ottieni/crea il canale broadcast e sottoscriviti
                        for cid in conversations {
                            let ch = {
                                let mut map = state.channels.write().await;
                                map.entry(cid).or_insert_with(|| broadcast::channel(256).0).clone()
                            };
                            let rx = ch.subscribe();
                            // Avvia forwarder broadcast -> mpsc centralizzato
                            let handle = spawn_forwarder(rx, evt_tx.clone());
                            sub_tasks.push(handle);
                        }
                    }

                    Ok(InMsg::Send { cid, tmp_id, body }) => {
                        // Controllo partecipazione (opzionale, se già gestita via REST puoi ometterla)
                        let allowed: Option<i64> = sqlx::query_scalar(
                            "SELECT 1 FROM participants WHERE conversation_id=? AND user_id=?"
                        )
                        .bind(cid).bind(user_id)
                        .fetch_optional(&state.pool).await.ok().flatten();

                        if allowed.is_none() {
                            // Notifica errore al mittente (facoltativo)
                            let _ = socket.send(Message::Text(
                                serde_json::json!({"type":"error","cid":cid,"msg":"forbidden"}).to_string()
                            )).await;
                            continue;
                        }

                        // Persisti messaggio
                        let id = match sqlx::query(
                            "INSERT INTO messages(conversation_id, author_id, body, created_at) \
                             VALUES(?,?,?, strftime('%s','now'))"
                        )
                        .bind(cid).bind(user_id).bind(&body)
                        .execute(&state.pool).await
                        {
                            Ok(r) => r.last_insert_rowid(),
                            Err(_) => {
                                // Notifica errore al mittente (facoltativo)
                                let _ = socket.send(Message::Text(
                                    serde_json::json!({"type":"error","cid":cid,"msg":"persist_fail"}).to_string()
                                )).await;
                                continue;
                            }
                        };

                        // Evento da broadcastare
                        let evt = serde_json::json!({
                            "type":"message",
                            "cid": cid,
                            "msg": { "id": id, "author_id": user_id, "body": body }
                        });

                        // Pubblica sul canale della conversazione
                        let ch = {
                            let mut m = state.channels.write().await;
                            m.entry(cid).or_insert_with(|| broadcast::channel(256).0).clone()
                        };
                        let _ = ch.send(evt);

                        // Ack al mittente, se presente tmp_id
                        if let Some(t) = tmp_id {
                            let _ = socket.send(Message::Text(
                                serde_json::json!({"type":"ack","cid":cid,"tmp_id":t,"msg_id":id}).to_string()
                            )).await;
                        }
                    }

                    Ok(InMsg::Read { cid, msg_id }) => {
                        // Aggiorna last_read_msg
                        let _ = sqlx::query(
                            "UPDATE participants SET last_read_msg=? WHERE conversation_id=? AND user_id=?"
                        )
                        .bind(msg_id).bind(cid).bind(user_id)
                        .execute(&state.pool).await;

                        // Notifica di read agli altri partecipanti
                        let evt = serde_json::json!({
                            "type":"read",
                            "cid": cid,
                            "user": user_id,
                            "msg_id": msg_id
                        });
                        let ch = {
                            let mut m = state.channels.write().await;
                            m.entry(cid).or_insert_with(|| broadcast::channel(256).0).clone()
                        };
                        let _ = ch.send(evt);
                    }

                    Err(_) => {
                        // Messaggio malformato: opzionale, rispondi con errore
                        let _ = socket.send(Message::Text(
                            serde_json::json!({"type":"error","msg":"bad_payload"}).to_string()
                        )).await;
                    }
                }
            }

            // Eventi dalle conversazioni sottoscritte -> inviali al client
            Some(evt) = evt_rx.recv() => {
                let _ = socket.send(Message::Text(evt.to_string())).await;
            }
        }
    }

    // cleanup: abortisci eventuali task rimasti
    for h in sub_tasks { h.abort(); }
}
