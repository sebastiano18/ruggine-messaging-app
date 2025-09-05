use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt}; // StreamExt non serve in questo file
use tokio::{
    select,
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tracing::info;
use uuid::Uuid;

use crate::{state::AppState, error::Result};
use super::{reader::spawn_reader, recv_merge::spawn_receiver, helpers::cleanup_empty_channels};

#[derive(Debug)]
pub enum OutboundMsg {
    Text(String),
    Binary(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<axum::extract::ws::CloseFrame<'static>>),
}

pub struct ConnectionActor;

impl ConnectionActor {
    pub async fn start(socket: WebSocket, state: AppState, user_id: Uuid, username: String) -> Result<()> {
        let (mut ws_tx, ws_rx) = socket.split();

        // Coordinamento shutdown + coda bounded verso l’unico writer
        let (stop_tx, stop_rx) = watch::channel(false);
        let (out_tx, mut out_rx) = mpsc::channel::<OutboundMsg>(1024);

        // Clone dedicato del receiver per il writer
        let mut stop_rx_writer = stop_rx.clone();

        // Writer: unico proprietario di ws_tx
        let mut writer: JoinHandle<()> = tokio::spawn(async move {
            loop {
                select! {
                    _ = stop_rx_writer.changed() => {
                        let _ = ws_tx.send(Message::Close(None)).await;
                        break;
                    }
                    maybe_msg = out_rx.recv() => {
                        let Some(msg) = maybe_msg else { break; };
                        let to_send = match msg {
                            OutboundMsg::Text(s)   => Message::Text(s),
                            OutboundMsg::Binary(b) => Message::Binary(b),
                            OutboundMsg::Pong(b)   => Message::Pong(b),
                            OutboundMsg::Close(f)  => Message::Close(f),
                        };
                        if ws_tx.send(to_send).await.is_err() {
                            break;
                        }
                    }
                }
            }
            info!("writer end for {}", user_id);
        });

        // Receiver: merge dei broadcast delle conversazioni (server -> client)
        let mut recv_task = spawn_receiver(
            state.clone(),
            user_id,
            out_tx.clone(),
            stop_tx.clone(),
            stop_rx.clone(),
        ).await?;

        // Reader: input client -> valida/salva/broadcast -> risposte via out_tx
        let mut reader_task = spawn_reader(
            ws_rx,
            state.clone(),
            user_id,
            username,
            out_tx.clone(),
            stop_tx.clone(),
            stop_rx.clone(),
        );

        // Orchestrazione (nota: &mut su JoinHandle)
        select! {
            _ = &mut reader_task => {
                let _ = stop_tx.send(true);
                recv_task.abort();
                writer.abort();
            }
            _ = &mut recv_task => {
                let _ = stop_tx.send(true);
                reader_task.abort();
                writer.abort();
            }
            _ = &mut writer => {
                reader_task.abort();
                recv_task.abort();
            }
        }

        cleanup_empty_channels(&state.channels, user_id).await;
        Ok(())
    }
}
