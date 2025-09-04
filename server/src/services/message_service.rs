use crate::{error::Result, repositories::message_repo::MessageRepo, state::AppState, ws};
use serde_json::json;
use uuid::Uuid;

pub struct MessageService;

impl MessageService {
    pub async fn list(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<(Uuid, Uuid, String, i64)>> {
        MessageRepo::list(pool, conversation_id, limit).await
    }

    pub async fn post(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        author_id: Uuid,
        author_username: String,
        content: &str,
        state: &AppState, // broadcast
    ) -> Result<Uuid> {
        let msg_id = MessageRepo::insert(pool, conversation_id, author_id, content).await?;

        let event = json!({
            "type": "message",
            "conversation_id": conversation_id,
            "id": msg_id,
            "author_id": author_id,
            "author_username":  author_username,
            "content": content,
        });

        ws::ws_broadcast(state, event).await;
        Ok(msg_id)
    }
}
