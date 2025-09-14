use anyhow::Result;
use futures::{SinkExt, StreamExt};
use std::borrow::Cow;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        http::{header, HeaderValue, Request},
        protocol::Message,
    },
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, warn};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
pub struct WsControl {
    pub shutdown: oneshot::Sender<()>,
    pub outgoing_tx: mpsc::UnboundedSender<String>,
}

/// Connessione WebSocket con validazione URL migliorata
pub async fn connect(base: &str, token: &str) -> Result<WsStream> {
    let ws_url = format!("{}/ws", base.trim_end_matches('/')).replacen("http", "ws", 1);
    debug!("Connecting to WebSocket: {}", ws_url);

    // Validazione token
    if token.trim().is_empty() {
        return Err(anyhow::anyhow!("Token di autenticazione vuoto"));
    }

    let mut req: Request<()> = ws_url.as_str().into_client_request()?;
    req.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token.trim()))?, // CORREZIONE: trim del token
    );

    let (ws, resp) = connect_async(req).await?;
    debug!(
        "WebSocket connection established with status: {}",
        resp.status()
    );
    Ok(ws)
}

/// Subscribe con timeout
pub async fn subscribe(ws: &mut WsStream) -> Result<()> {
    debug!("Sending subscribe message");

    let subscribe_msg = Message::Text(r#"{"type":"subscribe"}"#.into());

    // Timeout per il subscribe
    tokio::time::timeout(std::time::Duration::from_secs(10), ws.send(subscribe_msg)).await??;

    Ok(())
}

/// Versione bidirezionale con correzioni essenziali
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
        const MAX_FAILURES: u32 = 5;
        const PING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

        loop {
            tokio::select! {
                // Shutdown richiesto
                _ = &mut shutdown_rx => {
                    debug!("WebSocket shutdown requested");
                    let close_frame = CloseFrame {
                        code: CloseCode::Normal,
                        reason: Cow::from("app_exit"),
                    };

                    // Tentativo graceful close
                    if let Err(e) = ws.send(Message::Close(Some(close_frame))).await {
                        debug!("Failed to send close frame: {}", e);
                    }

                    // Attendi conferma chiusura per massimo 1 secondo
                    let _ = tokio::time::timeout(
                        std::time::Duration::from_millis(1000),
                        ws.next()
                    ).await;

                    debug!("WebSocket connection closed gracefully");
                    break;
                }

                // Messaggio da inviare al server
                Some(msg) = outgoing_rx.recv() => {
                    debug!("Sending message to server: {}",
                           msg.chars().take(100).collect::<String>());

                    // CORREZIONE: Validazione dimensione messaggio
                    if msg.len() > 100_000 {
                        error!("Message too large ({} bytes), dropping", msg.len());
                        continue;
                    }

                    match ws.send(Message::Text(msg)).await {
                        Ok(_) => {
                            consecutive_failures = 0;
                        }
                        Err(e) => {
                            error!("Failed to send message to server: {}", e);
                            consecutive_failures += 1;
                            if consecutive_failures >= MAX_FAILURES {
                                error!("Too many consecutive send failures ({}), closing connection",
                                       consecutive_failures);
                                break;
                            }
                        }
                    }
                }

                // Messaggio ricevuto dal server
                Some(msg_result) = ws.next() => {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            debug!("Received text message: {}",
                                   text.chars().take(100).collect::<String>());

                            // CORREZIONE: Validazione dimensione messaggio ricevuto
                            if text.len() > 1_000_000 {
                                error!("Received message too large ({} bytes), ignoring", text.len());
                                continue;
                            }

                            consecutive_failures = 0;
                            on_text(text);
                        }
                        Ok(Message::Pong(payload)) => {
                            debug!("Received pong from server (payload: {} bytes)", payload.len());
                            last_pong = std::time::Instant::now();
                            consecutive_failures = 0;
                        }
                        Ok(Message::Ping(payload)) => {
                            debug!("Received ping from server, sending pong");
                            if let Err(e) = ws.send(Message::Pong(payload)).await {
                                warn!("Failed to send pong response: {}", e);
                                consecutive_failures += 1;
                            } else {
                                consecutive_failures = 0;
                            }
                        }
                        Ok(Message::Close(frame)) => {
                            debug!("Received close frame: {:?}", frame);
                            let _ = ws.send(Message::Close(None)).await;
                            break;
                        }
                        Ok(Message::Binary(data)) => {
                            debug!("Received binary message ({} bytes), ignoring", data.len());
                        }
                        Ok(Message::Frame(_)) => {
                            debug!("Received raw frame, ignoring");
                        }
                        Err(e) => {
                            error!("WebSocket receive error: {}", e);
                            consecutive_failures += 1;

                            // CORREZIONE: Distingui errori fatali
                            let error_str = e.to_string().to_lowercase();
                            if error_str.contains("connection closed") ||
                               error_str.contains("broken pipe") {
                                error!("Connection terminated by peer");
                                break;
                            }

                            if consecutive_failures >= MAX_FAILURES {
                                error!("Too many consecutive receive failures ({}), closing connection",
                                       consecutive_failures);
                                break;
                            }

                            // Pausa breve prima del prossimo tentativo
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }

                // Ping periodico con controllo timeout migliorato
                _ = ping_interval.tick() => {
                    let elapsed_since_pong = last_pong.elapsed();

                    if elapsed_since_pong > PING_TIMEOUT {
                        error!("No pong received for {:?}, connection appears dead", elapsed_since_pong);
                        break;
                    }

                    debug!("Sending periodic ping (last pong: {:?} ago)", elapsed_since_pong);

                    // Invia ping WebSocket nativo
                    match ws.send(Message::Ping(vec![])).await {
                        Ok(_) => debug!("WebSocket ping sent successfully"),
                        Err(e) => {
                            error!("Failed to send WebSocket ping: {}", e);
                            consecutive_failures += 1;
                        }
                    }

                    // Invia anche ping JSON per compatibilità con il server
                    match ws.send(Message::Text(r#"{"type":"ping"}"#.to_string())).await {
                        Ok(_) => debug!("JSON ping sent successfully"),
                        Err(e) => {
                            error!("Failed to send JSON ping: {}", e);
                            consecutive_failures += 1;
                        }
                    }

                    if consecutive_failures >= MAX_FAILURES {
                        error!("Too many ping failures ({}), closing connection", consecutive_failures);
                        break;
                    }
                }
            }
        }

        debug!(
            "WebSocket handler loop ended (failures: {})",
            consecutive_failures
        );
    });

    WsControl {
        shutdown: shutdown_tx,
        outgoing_tx,
    }
}

/// Versione semplificata con correzioni basilari
pub fn spawn_simple_handler(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
) -> WsControl {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    debug!("Simple WebSocket handler shutdown");
                    let _ = ws.send(Message::Close(None)).await; // CORREZIONE: Graceful close
                    break;
                }

                Some(msg) = outgoing_rx.recv() => {
                    if let Err(e) = ws.send(Message::Text(msg)).await {
                        error!("Simple handler send error: {}", e);
                        break;
                    }
                }

                Some(msg_result) = ws.next() => {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            on_text(text);
                        }
                        Ok(Message::Close(_)) => {
                            debug!("Simple handler received close");
                            break;
                        }
                        Ok(Message::Ping(payload)) => {
                            if ws.send(Message::Pong(payload)).await.is_err() {
                                break;
                            }
                        }
                        Ok(Message::Pong(_)) => {
                            debug!("Simple handler received pong");
                        }
                        Ok(Message::Binary(_)) => {
                            debug!("Simple handler ignoring binary message");
                        }
                        Ok(Message::Frame(_)) => {
                            debug!("Simple handler ignoring raw frame");
                        }
                        Err(e) => {
                            error!("Simple handler receive error: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        debug!("Simple WebSocket handler ended");
    });

    WsControl {
        shutdown: shutdown_tx,
        outgoing_tx,
    }
}
