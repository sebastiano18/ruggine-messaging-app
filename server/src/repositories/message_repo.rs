use crate::error::Result;
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone)]
pub struct MessageRepo;

impl MessageRepo {
    /// Ritorna gli ultimi `limit` messaggi di una conversazione (ordinati DESC per id)
    pub async fn list_by_conversation(
        pool: &SqlitePool,
        cid: i64,
        limit: i64,
    ) -> Result<Vec<(i64, i64, String, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT id, author_id, content, created_at
            FROM messages
            WHERE conversation_id = ?
            ORDER BY id DESC
            LIMIT ?
            "#,
        )
            .bind(cid)
            .bind(limit)
            .fetch_all(pool)
            .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let id: i64 = r.get("id");
                let author_id: i64 = r.get("author_id");
                let content: String = r.get("content");
                let created_at: i64 = r.get("created_at");
                (id, author_id, content, created_at)
            })
            .collect();

        Ok(items)
    }

    pub async fn insert(
        pool: &SqlitePool,
        cid: i64,
        author_id: i64,
        content: &str,
    ) -> Result<i64> {
        let res = sqlx::query(
            "INSERT INTO messages(conversation_id, author_id, content, created_at)
             VALUES(?, ?, ?, strftime('%s','now'))",
        )
            .bind(cid)
            .bind(author_id)
            .bind(content)
            .execute(pool)
            .await?;

        Ok(res.last_insert_rowid())
    }
}
