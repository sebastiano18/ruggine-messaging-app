use axum::{routing::{get, post}, Router};
use crate::state::AppState;
use crate::controllers::conversation_controller as c;

pub fn router() -> Router<AppState> {
    Router::new()
        // Gestione conversazioni unificate
        .route("/conversations", get(c::mine))

        // Singola conversazione per fetch mirata e cancellazione
        .route("/conversations/:id", get(c::get_conversation).delete(c::delete_conversation))

        // NUOVO: Conversazione con messaggi in una sola chiamata
        .route("/conversations/:id/with-messages", get(c::get_conversation_with_messages))

        // Creazione gruppi
        .route("/conversations/groups", post(c::create_group))

        // Creazione/ricerca DM
        .route("/conversations/dm", post(c::create_dm))

        // Aggiunta membri ai gruppi
        .route("/conversations/:id/members", post(c::add_member))
}