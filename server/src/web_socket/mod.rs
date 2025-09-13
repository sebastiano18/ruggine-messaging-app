use axum::{
    extract::State,
    extract::ws::{WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use uuid::Uuid;

use self::actor::ConnectionActor;
use crate::{auth::AuthUser, error::Result, state::AppState};

pub mod actor;
pub mod helpers;
pub mod reader;
pub mod recv_merge;

pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    user: AuthUser,
) -> impl IntoResponse {
    let uid = user.id;
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
