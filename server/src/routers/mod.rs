use axum::Router;
use crate::state::AppState;

mod user_route;
mod group_route;
mod message_route;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .merge(user_route::router())
                .merge(group_route::router())
                .merge(message_route::router()),
        )
        .with_state(state)
}


