use axum::Router;
use crate::state::AppState;

pub mod user_route;
pub mod group_route;
pub mod message_route;

/// Costruisce il Router principale e collega tutte le route
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        // I sub-router espongono path assoluti (/users/..., /groups/..., /conversations/...)
        // quindi qui usiamo `merge` invece di `nest` per evitare doppi prefissi.
        .merge(user_route::router())
        .merge(group_route::router())
        .merge(message_route::router())
        .with_state(state)
}


