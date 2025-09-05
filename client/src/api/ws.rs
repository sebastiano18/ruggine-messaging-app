use anyhow::Result;
use futures::{SinkExt, StreamExt};
use std::borrow::Cow;
use tokio::sync::{oneshot, mpsc};
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use tokio_tungstenite::{
    connect_async,
    MaybeTlsStream,
    WebSocketStream,
    tungstenite::{
        client::IntoClientRequest,
        http::{Request, HeaderValue, header},
        protocol::Message,
    },
};
use tokio::net::TcpStream;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
pub struct WsControl {
    pub shutdown: oneshot::Sender<()>,
    pub outgoing_tx: mpsc::UnboundedSender<String>,
}

/// Connessione
pub async fn connect(base: &str, token: &str) -> Result<WsStream> {
    let ws_url = format!("{}/ws", base.trim_end_matches('/')).replacen("http", "ws", 1);
    let mut req: Request<()> = ws_url.as_str().into_client_request()?;
    req.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token))?,
    );
    let (ws, _resp) = connect_async(req).await?;
    Ok(ws)
}

/// Subscribe
pub async fn subscribe(ws: &mut WsStream) -> Result<()> {
    ws.send(Message::Text(r#"{"type":"subscribe"}"#.into())).await?;
    Ok(())
}

/// Versione bidirezionale: gestisce sia lettura che scrittura
pub fn spawn_bidirectional_handler(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
) -> WsControl {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                // Shutdown richiesto
                _ = &mut shutdown_rx => {
                    let _ = ws.send(Message::Close(Some(CloseFrame{
                        code: CloseCode::Normal,
                        reason: Cow::from("app_exit"),
                    }))).await;

                    let _ = tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        ws.next()
                    ).await;

                    break;
                }

                // Messaggio da inviare al server
                Some(msg) = outgoing_rx.recv() => {
                    if ws.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }

                // Messaggio ricevuto dal server
                Some(msg) = ws.next() => {
                    match msg {
                        Ok(Message::Text(t)) => {
                            on_text(t);
                        }
                        Ok(Message::Pong(_)) => {
                            // ok
                        }
                        Ok(Message::Ping(_)) => {
                            // tokio_tungstenite risponde automaticamente
                        }
                        Ok(Message::Close(_)) => {
                            break;
                        }
                        Ok(_) => {}
                        Err(_) => {
                            break;
                        }
                    }
                }

                // Ping periodico
                _ = interval.tick() => {
                    if ws.send(Message::Ping(vec![])).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    WsControl {
        shutdown: shutdown_tx,
        outgoing_tx,
    }
}

/// Versione precedente per compatibilità (deprecata)
pub fn spawn_reader_and_pinger(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
) -> WsControl {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let (outgoing_tx, _) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    let _ = ws.send(Message::Close(Some(CloseFrame{
                        code: CloseCode::Normal,
                        reason: Cow::from("app_exit"),
                    }))).await;

                    let _ = tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        ws.next()
                    ).await;

                    break;
                }

                Some(msg) = ws.next() => {
                    match msg {
                        Ok(Message::Text(t)) => {
                            on_text(t);
                        }
                        Ok(Message::Pong(_)) => {}
                        Ok(Message::Ping(_)) => {}
                        Ok(Message::Close(_)) => {
                            break;
                        }
                        Ok(_) => {}
                        Err(_) => {
                            break;
                        }
                    }
                }

                _ = interval.tick() => {
                    if ws.send(Message::Ping(vec![])).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    WsControl {
        shutdown: shutdown_tx,
        outgoing_tx,
    }
}