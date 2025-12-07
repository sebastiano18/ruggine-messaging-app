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
    pub user_sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

/// Connessione WebSocket con validazione URL migliorata
pub async fn connect(base: &str, token: &str, session_id: Option<uuid::Uuid>) -> Result<WsStream> {
    let ws_url = if let Some(sid) = session_id {
        format!("{}/ws?session_id={}", base.trim_end_matches('/'), sid)
            .replacen("http", "ws", 1)
    } else {
        format!("{}/ws", base.trim_end_matches('/')).replacen("http", "ws", 1)
    };

    debug!("Connecting to WebSocket: {}", ws_url);

    // Validazione token
    if token.trim().is_empty() {
        return Err(anyhow::anyhow!("Token di autenticazione vuoto"));
    }

    let mut req: Request<()> = ws_url.as_str().into_client_request()?;
    req.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token.trim()))?,
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

    tokio::time::timeout(std::time::Duration::from_secs(10), ws.send(subscribe_msg)).await??;

    Ok(())
}

/// Versione bidirezionale con correzioni essenziali
pub fn spawn_bidirectional_handler(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
    disconnect_notifier: Option<mpsc::UnboundedSender<crate::models::UiEvent>>,
    user_sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
) -> WsControl {

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<String>();

    // Clone per vari task
    let ping_tx = outgoing_tx.clone();
    let ping_user_seq = user_sequence.clone();

    // ========================================================================
    // TASK INDIPENDENTE: Invia ping automatici ogni 30 secondi
    // Questo task gira su tokio e NON dipende dal loop UI di egui
    // ========================================================================
    tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Skip first immediate tick
        ping_interval.tick().await;

        loop {
            ping_interval.tick().await;

            let user_seq = ping_user_seq.load(std::sync::atomic::Ordering::SeqCst);

            let ping_json = serde_json::json!({
                "type": "ping",
                "timestamp": chrono::Utc::now().timestamp(),
                "user_sequence": user_seq,
            });

            match serde_json::to_string(&ping_json) {
                Ok(msg) => {
                    // Se il send fallisce, il WebSocket è chiuso -> termina task
                    if ping_tx.send(msg).is_err() {
                        debug!("Ping task: WebSocket closed, stopping ping task");
                        break;
                    }
                    debug!("🔔 Automatic ping sent from independent tokio task (user_seq: {})", user_seq);
                }
                Err(e) => {
                    error!("Failed to serialize ping JSON: {}", e);
                }
            }
        }

        debug!("Independent ping task terminated");
    });

    tokio::spawn(async move {
        // Ping WebSocket base ogni 60 secondi (solo per keepalive)
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(60));
        let mut last_pong = std::time::Instant::now();
        let mut consecutive_failures = 0u32;
        const MAX_FAILURES: u32 = 5;
        const PING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
        let mut graceful_shutdown = false;

        loop {
            tokio::select! {
                // Shutdown richiesto
                _ = &mut shutdown_rx => {
                    debug!("WebSocket shutdown requested");
                    graceful_shutdown = true;
                    let close_frame = CloseFrame {
                        code: CloseCode::Normal,
                        reason: Cow::from("app_exit"),
                    };

                    if let Err(e) = ws.send(Message::Close(Some(close_frame))).await {
                        debug!("Failed to send close frame: {}", e);
                    }

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

                            let error_str = e.to_string().to_lowercase();
                            debug!("Error string (lowercase) for matching: '{}'", error_str);

                            if error_str.contains("connection closed") ||
                               error_str.contains("broken pipe") ||
                               error_str.contains("interrotta") ||  // "Connessione in corso interrotta"
                               error_str.contains("10054") {        // Windows error code
                                error!("Connection terminated by peer - error matched: {}", error_str);
                                break;
                            }

                            if consecutive_failures >= MAX_FAILURES {
                                error!("Too many consecutive receive failures ({}), closing connection",
                                       consecutive_failures);
                                break;
                            }

                            warn!("WebSocket receive error (failure {}/{}), retrying after delay",
                                  consecutive_failures, MAX_FAILURES);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }

                // Ping periodico SOLO per keepalive (ogni 60 secondi)
                _ = ping_interval.tick() => {
                    let elapsed_since_pong = last_pong.elapsed();

                    if elapsed_since_pong > PING_TIMEOUT {
                        error!("No pong received for {:?}, connection appears dead", elapsed_since_pong);
                        break;
                    }

                    debug!("Sending WebSocket keepalive ping (last pong: {:?} ago)", elapsed_since_pong);

                    // SOLO ping WebSocket nativo per keepalive
                    match ws.send(Message::Ping(vec![])).await {
                        Ok(_) => debug!("WebSocket keepalive ping sent"),
                        Err(e) => {
                            error!("Failed to send WebSocket ping: {}", e);
                            consecutive_failures += 1;
                            if consecutive_failures >= MAX_FAILURES {
                                error!("Too many ping failures ({}), closing connection", consecutive_failures);
                                break;
                            }
                        }
                    }

                    // RIMOSSO il ping JSON - il sistema enhanced lo gestisce
                }
            }
        }

        // Notifica disconnessione se non è shutdown graceful
        if !graceful_shutdown {
            if let Some(notifier) = disconnect_notifier {
                let _ = notifier.send(crate::models::UiEvent::WsDisconnected);
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
        user_sequence,
    }
}

/// Versione semplificata con correzioni basilari
pub fn spawn_simple_handler(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
    user_sequence: std::sync::Arc<std::sync::atomic::AtomicU64>,
) -> WsControl {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    debug!("Simple WebSocket handler shutdown");
                    let _ = ws.send(Message::Close(None)).await;
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
        user_sequence,
    }
}