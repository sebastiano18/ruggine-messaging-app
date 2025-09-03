use serde_json::json;
use crate::{error::Result, state::AppState, repositories::message_repo::MessageRepo, ws};
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
        content: &str,
        state: &AppState, // broadcast
    ) -> Result<Uuid> {
        let msg_id = MessageRepo::insert(pool, conversation_id, author_id, content).await?;

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
