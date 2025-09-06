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
use tracing::{debug, warn, error};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
pub struct WsControl {
    pub shutdown: oneshot::Sender<()>,
    pub outgoing_tx: mpsc::UnboundedSender<String>,
}

/// Connessione
pub async fn connect(base: &str, token: &str) -> Result<WsStream> {
    let ws_url = format!("{}/ws", base.trim_end_matches('/')).replacen("http", "ws", 1);
    debug!("Connecting to WebSocket: {}", ws_url);

    let mut req: Request<()> = ws_url.as_str().into_client_request()?;
    req.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token))?,
    );
    let (ws, _resp) = connect_async(req).await?;
    debug!("WebSocket connection established");
    Ok(ws)
}

/// Subscribe
pub async fn subscribe(ws: &mut WsStream) -> Result<()> {
    debug!("Sending subscribe message");
    ws.send(Message::Text(r#"{"type":"subscribe"}"#.into())).await?;
    Ok(())
}

/// Versione bidirezionale migliorata con gestione errori e heartbeat
pub fn spawn_bidirectional_handler(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
) -> WsControl {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut last_pong = std::time::Instant::now();
        let mut consecutive_failures = 0u32;
        const MAX_FAILURES: u32 = 3;

        loop {
            tokio::select! {
                // Shutdown richiesto
                _ = &mut shutdown_rx => {
                    debug!("WebSocket shutdown requested");
                    let close_frame = CloseFrame {
                        code: CloseCode::Normal,
                        reason: Cow::from("app_exit"),
                    };
                    let _ = ws.send(Message::Close(Some(close_frame))).await;

                    // Attendi conferma chiusura per massimo 500ms
                    let _ = tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        ws.next()
                    ).await;

                    debug!("WebSocket connection closed");
                    break;
                }

                // Messaggio da inviare al server
                Some(msg) = outgoing_rx.recv() => {
                    debug!("Sending message to server: {}", msg);
                    if let Err(e) = ws.send(Message::Text(msg)).await {
                        error!("Failed to send message to server: {}", e);
                        consecutive_failures += 1;
                        if consecutive_failures >= MAX_FAILURES {
                            error!("Too many consecutive send failures, closing connection");
                            break;
                        }
                    } else {
                        consecutive_failures = 0; // Reset su successo
                    }
                }

                // Messaggio ricevuto dal server
                Some(msg_result) = ws.next() => {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            debug!("Received text message: {}", text);
                            consecutive_failures = 0; // Reset su ricezione riuscita
                            on_text(text);
                        }
                        Ok(Message::Pong(_)) => {
                            debug!("Received pong from server");
                            last_pong = std::time::Instant::now();
                            consecutive_failures = 0;
                        }
                        Ok(Message::Ping(payload)) => {
                            debug!("Received ping from server, sending pong");
                            if let Err(e) = ws.send(Message::Pong(payload)).await {
                                warn!("Failed to send pong: {}", e);
                                consecutive_failures += 1;
                            }
                        }
                        Ok(Message::Close(frame)) => {
                            debug!("Received close frame: {:?}", frame);
                            break;
                        }
                        Ok(Message::Binary(_)) => {
                            debug!("Received binary message (ignored)");
                        }
                        Ok(Message::Frame(_)) => {
                            debug!("Received raw frame (ignored)");
                        }
                        Err(e) => {
                            error!("WebSocket error: {}", e);
                            consecutive_failures += 1;
                            if consecutive_failures >= MAX_FAILURES {
                                error!("Too many consecutive receive failures, closing connection");
                                break;
                            }
                        }
                    }
                }

                // Ping periodico con controllo timeout
                _ = ping_interval.tick() => {
                    // Controlla se è passato troppo tempo dall'ultimo pong
                    if last_pong.elapsed() > std::time::Duration::from_secs(90) {
                        error!("No pong received for 90 seconds, connection may be dead");
                        break;
                    }

                    // Invia ping al server (sia JSON che WebSocket ping)
                    debug!("Sending ping to server");
                    
                    // Invia sia ping WebSocket nativo
                    if let Err(e) = ws.send(Message::Ping(vec![])).await {
                        error!("Failed to send WebSocket ping: {}", e);
                        consecutive_failures += 1;
                    }
                    
                    // Sia ping JSON per compatibilità con il server
                    if let Err(e) = ws.send(Message::Text(r#"{"type":"ping"}"#.to_string())).await {
                        error!("Failed to send JSON ping: {}", e);
                        consecutive_failures += 1;
                    }

                    if consecutive_failures >= MAX_FAILURES {
                        error!("Too many ping failures, closing connection");
                        break;
                    }
                }
            }
        }

        debug!("WebSocket handler loop ended");
    });

    WsControl {
        shutdown: shutdown_tx,
        outgoing_tx,
    }
}

/// Versione semplificata per test/debugging
pub fn spawn_simple_handler(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
) -> WsControl {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                
                Some(msg) = outgoing_rx.recv() => {
                    if ws.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }
                
                Some(msg_result) = ws.next() => {
                    match msg_result {
                        Ok(Message::Text(text)) => on_text(text),
                        Ok(Message::Close(_)) => break,
                        Ok(Message::Ping(payload)) => {
                            let _ = ws.send(Message::Pong(payload)).await;
                        }
                        Ok(Message::Pong(_)) => {
                            // Pong ricevuto, tutto ok
                        }
                        Ok(Message::Binary(_)) => {
                            // Ignora messaggi binari
                        }
                        Ok(Message::Frame(_)) => {
                            // Ignora frame grezzi
                        }
                        Err(_) => break,
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