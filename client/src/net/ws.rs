use anyhow::Result;
use futures::{SinkExt, StreamExt};
use std::borrow::Cow;
use tokio::sync::oneshot;
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

/// (opzionale) subscribe
pub async fn subscribe(ws: &mut WsStream) -> Result<()> {
    ws.send(Message::Text(r#"{"type":"subscribe"}"#.into())).await?;
    Ok(())
}

/// Avvia reader+pinger e restituisce un handle per spegnerlo con Close
pub fn spawn_reader_and_pinger(
    mut ws: WsStream,
    mut on_text: impl FnMut(String) + Send + 'static,
) -> WsControl {
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                // ✅ richiesta di spegnimento: invia Close e attendi un attimo
                _ = &mut shutdown_rx => {
                    let _ = ws.send(Message::Close(Some(CloseFrame{
                        code: CloseCode::Normal,
                        reason: Cow::from("app_exit"),
                    }))).await;

                    // Attendi eventuale risposta/flush per poco
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
                        Ok(Message::Pong(_)) => {
                            // ok
                        }
                        Ok(Message::Ping(_)) => {
                            // tokio_tungstenite risponde in automatico di solito, ma se vuoi:
                            // let _ = ws.send(Message::Pong(vec![])).await;
                        }
                        Ok(Message::Close(_)) => {
                            // peer ha chiuso: usciamo dal loop
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            // errore di lettura: considera la connessione chiusa
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

    WsControl { shutdown: shutdown_tx }
}
