use crate::error::Result;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct MessageRepo;

impl MessageRepo {
    pub async fn list(
        pool: &SqlitePool,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<(Uuid, Uuid, String, i64)>> {
        let rows = sqlx::query(
            "SELECT id, author_id, content, created_at FROM messages
             WHERE conversation_id = ? ORDER BY created_at DESC LIMIT ?",
        )
            .bind(conversation_id.to_string())
            .bind(limit)
            .fetch_all(pool)
            .await?;

        Ok(rows.into_iter().map(|r| {
            let id_str: String = r.get("id");
            let author_str: String = r.get("author_id");
            let content: String = r.get("content");
            let created_at: i64 = r.get("created_at");
            (
                Uuid::parse_str(&id_str).unwrap(),
                Uuid::parse_str(&author_str).unwrap(),
                content,
                created_at,
            )
        }).collect())
    }

    pub async fn insert(
        pool: &SqlitePool,
        conversation_id: Uuid,
        author_id: Uuid,
        content: &str,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO messages(id, conversation_id, author_id, content, created_at)
             VALUES(?, ?, ?, ?, strftime('%s','now'))",
        )
            .bind(id.to_string())
            .bind(conversation_id.to_string())
            .bind(author_id.to_string())
            .bind(content)
            .execute(pool)
            .await?;

        Ok(id)
    }
}
