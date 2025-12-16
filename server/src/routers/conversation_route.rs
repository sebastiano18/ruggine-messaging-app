use axum::{routing::{get}, Router};
use crate::state::AppState;
use crate::controllers::conversation_controller as c;

pub fn router() -> Router<AppState> {
    Router::new()
        
        // Ottieni conversazioni con paginazione
        .route("/conversations", get(c::get_conversations))
        
        // Ottieni conversazione
        .route("/conversations/:id", get(c::get_conversation))
}