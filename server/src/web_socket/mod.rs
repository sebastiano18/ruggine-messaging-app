use axum::{
    extract::State,
    extract::ws::{WebSocket, WebSocketUpgrade},
    extract::Query,
    response::IntoResponse,
};
use std::collections::HashMap;
use uuid::Uuid;

use self::actor::ConnectionActor;
use crate::{auth::AuthUser, error::Result, state::AppState};

pub mod actor;
pub mod reader;
pub mod recv_merge;

// Moduli interni (non esportati pubblicamente)
pub(crate) mod utils;
pub(crate) mod broadcast;
mod initial_state;
pub mod handlers;


pub async fn ws_handler(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    user: AuthUser,
) -> impl IntoResponse {
    let uid = user.id;
    let uname = user.username;

    // Estrai session_id opzionale dalla query
    let client_session_id = params.get("session_id")
        .and_then(|s| Uuid::parse_str(s).ok());

    ws.on_upgrade(move |socket| async move {
        let _ = ws_entrypoint(socket, state, uid, uname, client_session_id).await;
    })
}

async fn ws_entrypoint(
    socket: WebSocket,
    state: AppState,
    user_id: Uuid,
    username: String,
    client_session_id: Option<Uuid>,
) -> Result<()> {
    ConnectionActor::start(socket, state, user_id, username, client_session_id).await
}