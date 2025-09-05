use tokio::{select, task::JoinHandle, sync::{mpsc, watch}};
use tokio_stream::wrappers::BroadcastStream;
use futures::{StreamExt, stream::select_all};
use serde_json::Value;
use tracing::{error, info};
use uuid::Uuid;

use crate::{state::AppState, error::Result};
use super::{actor::OutboundMsg, helpers::get_user_conversation_receivers};

pub async fn spawn_receiver(
    state: AppState,
    user_id: Uuid,
    out_tx: mpsc::Sender<OutboundMsg>,
    stop_tx: watch::Sender<bool>,
    mut stop_rx: watch::Receiver<bool>,
) -> Result<JoinHandle<()>> {
    let receivers = get_user_conversation_receivers(&state, user_id).await?;
    let mut combined = select_all(receivers.into_iter().map(BroadcastStream::new));

    Ok(tokio::spawn(async move {
        loop {
            select! {
                _ = stop_rx.changed() => break,
                item = combined.next() => {
                    match item {
                        Some(Ok(val)) => {
                            // niente echo al mittente
                            if let Some(author) = val.get("author_id")
                                .and_then(|x| x.as_str())
                                .and_then(|s| Uuid::parse_str(s).ok())
                            {
                                if author == user_id { continue; }
                            }

                            match serde_json::to_string(&val) {
                                Ok(txt) => {
                                    if out_tx.send(OutboundMsg::Text(txt)).await.is_err() {
                                        let _ = stop_tx.send(true); break;
                                    }
                                }
                                Err(e) => {
                                    error!("serialize err: {e}");
                                    let _ = out_tx.send(OutboundMsg::Text(
                                        r#"{"type":"error","message":"serialization_failed"}"#.into()
                                    )).await;
                                }
                            }
                        }
                        Some(Err(_)) => continue,
                        None => break,
                    }
                }
            }
        }
        info!("recv-merge end for {}", user_id);
    }))
}
