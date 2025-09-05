use axum::{
    extract::State,
    extract::ws::{WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use uuid::Uuid;

use crate::{auth::AuthUser, state::AppState, error::Result};
use self::actor::ConnectionActor;

pub mod actor;
pub mod reader;
pub mod recv_merge;
pub mod helpers;

pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    user: AuthUser,
) -> impl IntoResponse {
    let uid = user.id;           // se la tua struct si chiama user_id, cambia qui
    let uname = user.username;
    ws.on_upgrade(move |socket| async move {
        let _ = ws_entrypoint(socket, state, uid, uname).await;
    })
}

async fn ws_entrypoint(
    socket: WebSocket,
    state: AppState,
    user_id: Uuid,
    username: String,
) -> Result<()> {
    ConnectionActor::start(socket, state, user_id, username).await
}
