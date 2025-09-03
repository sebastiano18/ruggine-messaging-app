use crate::{controllers::message_controller as c, state::AppState};
use axum::{
    Router,
    routing::{get, post},
};
pub fn router() -> Router<AppState> {
    Router::new().route("/conversations/:cid/messages", get(c::list).post(c::post))
}
