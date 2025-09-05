use axum::Router;
use axum::routing::get;
use crate::state::AppState;
use crate::web_socket::ws_handler;

mod user_route;
mod conversation_route;
mod message_route;
mod invite_route;

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
        .route("/ws", get(ws_handler)) // Moved here
        .with_state(state)
}

