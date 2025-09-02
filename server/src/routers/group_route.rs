use axum::{routing::{get, post}, Router};
use crate::state::AppState;
use crate::controllers::group_controller as c;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/groups", post(c::create).get(c::mine))
        .route("/groups/:id/members", post(c::add_member))
}
