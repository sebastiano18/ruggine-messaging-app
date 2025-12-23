use crate::{controllers::message_controller as c, state::AppState};
use axum::{
    routing::{delete, get},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Endpoint unificato per messaggi con paginazione opzionale
        .route("/conversations/:cid/messages", get(c::list))

}