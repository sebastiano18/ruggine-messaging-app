use crate::{error::Result, repositories::message_repo::MessageRepo, state::AppState, ws};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct MessageService;

impl MessageService {
    pub async fn list(
        pool: &sqlx::SqlitePool,
        conversation_id: i64,
        limit: i64,
    ) -> Result<Vec<(i64, i64, String, i64)>> {
        MessageRepo::list(pool, conversation_id, limit as i32).await
    }

    /// Salva su DB e invia evento via WebSocket globale
    pub async fn post(
        pool: &sqlx::SqlitePool,
        conversation_id: i64,
        author_id: i64,
        content: &str,
        state: &AppState, // serve per il broadcast
    ) -> Result<i64> {
        let msg_id = MessageRepo::insert(pool, conversation_id, author_id, content).await?;

        // Evento autocontenuto per il client (un solo WS per utente → serve conversation_id)
        let event = json!({
            "type": "message",
            "conversation_id": conversation_id,
            "id": msg_id,
            "author_id": author_id,
            "content": content,
        });

        ws::ws_broadcast(state, event).await;

        Ok(msg_id)
    }
}
