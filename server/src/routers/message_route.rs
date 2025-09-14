use crate::{controllers::message_controller as c, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Endpoint esistenti per messaggi
        .route("/conversations/:cid/messages", get(c::list).post(c::post))

        // NUOVO: Endpoint per fetch messaggi (fetch-on-subscribe)
        .route("/conversations/:cid/messages/fetch", get(c::fetch_messages))
}