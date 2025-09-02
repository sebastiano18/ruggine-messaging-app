use anyhow::Result;
use tokio::net::TcpStream;
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
use futures::{SinkExt, StreamExt};
use serde_json::json;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect(base:&str, token:&str) -> Result<WsStream> {
    let ws_url = format!("{}/ws", base.replace("http", "ws"));
    let mut req: Request<()> = ws_url.as_str().into_client_request()?;
    req.headers_mut().insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
    let (ws, _resp) = connect_async(req).await?;
    Ok(ws)
}

pub async fn subscribe(ws:&mut WsStream, cids:&[i64]) -> Result<()> {
    let msg = json!({ "type":"subscribe", "conversations": cids }).to_string();
    ws.send(Message::Text(msg)).await?;
    Ok(())
}

pub async fn spawn_reader(mut ws: WsStream, mut on_text: impl FnMut(String) + Send + 'static) {
    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws.next().await {
            if let Message::Text(t) = msg {
                on_text(t);
            }
        }
    });
}
