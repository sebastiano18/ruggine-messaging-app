use axum::{routing::post, Router};
use crate::state::AppState;
use crate::controllers::user_controller as c;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users/register", post(c::register))
        .route("/users/login", post(c::login))
        .route("/users/logout", post(c::logout))
}
