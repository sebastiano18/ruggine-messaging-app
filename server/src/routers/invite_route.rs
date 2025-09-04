use axum::{routing::{get, post, delete}, Router};
use crate::state::AppState;
use crate::controllers::invite_controller as ic;

pub fn router() -> Router<AppState> {
    Router::new()
        // Crea un nuovo invito
        .route("/invites", post(ic::create_invite))

        // Usa un invito per unirsi a una conversazione
        .route("/invites/join", post(ic::use_invite))

        // Ottieni tutti gli inviti per una conversazione
        .route("/invites/conversation/:conversation_id", get(ic::get_invites))

        // Revoca un invito
        .route("/invites/:token", delete(ic::delete_invite))
}