use crate::models::Message;
use crate::{error::Result, repositories::message_repo::MessageRepo, state::AppState, ws};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct MessageService;

impl MessageService {
    pub async fn list(
        pool: &SqlitePool,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<(Uuid, Uuid, String, String, i64)>> {
        // Ora restituisce 5 elementi
        let rows = sqlx::query(
            "SELECT m.id, m.author_id, u.username, m.content, m.created_at 
         FROM messages m 
         JOIN users u ON m.author_id = u.id
         WHERE m.conversation_id = ? 
         ORDER BY m.created_at DESC 
         LIMIT ?",
        )
        .bind(conversation_id.to_string())
        .bind(limit)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let id_str: String = r.get("id");
                let author_str: String = r.get("author_id");
                let username: String = r.get("username");
                let content: String = r.get("content");
                let created_at: i64 = r.get("created_at");
                (
                    Uuid::parse_str(&id_str).unwrap(),
                    Uuid::parse_str(&author_str).unwrap(),
                    username,
                    content,
                    created_at,
                )
            })
            .collect())
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
