use axum::{routing::{get, post}, Router};
use crate::state::AppState;
use crate::controllers::conversation_controller as c;

pub fn router() -> Router<AppState> {
    Router::new()
        // Gestione conversazioni unificate
        .route("/conversations", get(c::mine))

        // NUOVO: Conversazione con messaggi in una sola chiamata
        .route("/conversations/:id/with-messages", get(c::get_conversation_with_messages))

        // Creazione gruppi
        .route("/conversations/groups", post(c::create_group))

        // Creazione/ricerca DM
        .route("/conversations/dm", post(c::create_dm))

        // Aggiunta membri ai gruppi
        .route("/conversations/:id/members", post(c::add_member))
        
        // Ottieni membri di una conversazione
        .route("/conversations/:id/members", get(c::get_members))
}