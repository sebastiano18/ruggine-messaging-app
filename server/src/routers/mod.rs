use axum::Router;
use crate::state::AppState;

pub mod user_route;
pub mod conversation_route;
pub mod message_route;
pub mod invite_route;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .merge(user_route::router())
                .merge(conversation_route::router())
                .merge(message_route::router())
                .merge(invite_route::router()),
        )
        .with_state(state)
}

