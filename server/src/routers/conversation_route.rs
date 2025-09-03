use axum::{routing::{get, post}, Router};
use crate::state::AppState;
use crate::controllers::conversation_controller as c;

pub fn router() -> Router<AppState> {
    Router::new()
        // Gestione conversazioni unificate
        .route("/conversations", get(c::mine))

        // Creazione gruppi
        .route("/conversations/groups", post(c::create_group))

        // Creazione/ricerca DM
        .route("/conversations/dm", post(c::create_dm))

        // Aggiunta membri ai gruppi
        .route("/conversations/:id/members", post(c::add_member))
}