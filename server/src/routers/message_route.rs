use crate::{controllers::message_controller as c, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Endpoint unificato per messaggi con paginazione opzionale
        .route("/conversations/:cid/messages", get(c::list).post(c::post))
    
}