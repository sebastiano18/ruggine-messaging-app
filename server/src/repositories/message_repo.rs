use crate::error::Result;
use sqlx::{Row, SqlitePool};

pub struct MessageRepo;

impl MessageRepo {
    // Ora prende direttamente conversation_id invece di group_id
    pub async fn insert(
        pool: &SqlitePool,
        conversation_id: i64,  // Cambiato da group_id
        author_id: i64,
        content: &str,
    ) -> Result<i64> {
        let res = sqlx::query(
            "INSERT INTO messages(conversation_id, author_id, content, created_at)
             VALUES(?, ?, ?, strftime('%s','now'))",
        )
            .bind(conversation_id)
            .bind(author_id)
            .bind(content)
            .execute(pool)
            .await?;

        Ok(res.last_insert_rowid())
    }

    // Ora prende direttamente conversation_id
    pub async fn list(
        pool: &SqlitePool,
        conversation_id: i64,  // Cambiato da group_id
        limit: i32,
    ) -> Result<Vec<(i64, i64, String, i64)>> {
        let rows = sqlx::query(
            "SELECT id, author_id, content, created_at 
             FROM messages 
             WHERE conversation_id = ? 
             ORDER BY created_at ASC 
             LIMIT ?",
        )
            .bind(conversation_id)
            .bind(limit)
            .fetch_all(pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get("id"),
                    r.get("author_id"),
                    r.get("content"),  // Cambiato da "body"
                    r.get("created_at"),
                )
            })
            .collect())
    }

    // Nuova funzione per verificare se un utente può accedere a una conversazione
    pub async fn can_user_access_conversation(
        pool: &SqlitePool,
        conversation_id: i64,
        user_id: i64
    ) -> Result<bool> {
        let row = sqlx::query(
            "SELECT 1 FROM participants WHERE conversation_id = ? AND user_id = ?"
        )
            .bind(conversation_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;

        Ok(row.is_some())
    }

    // Funzione helper per ottenere info sulla conversazione
    pub async fn get_conversation_info(
        pool: &SqlitePool,
        conversation_id: i64
    ) -> Result<Option<(String, String)>> {  // (kind, title)
        let row = sqlx::query(
            "SELECT kind, title FROM conversations WHERE id = ?"
        )
            .bind(conversation_id)
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|r| (
            r.get("kind"),
            r.get::<Option<String>, _>("title").unwrap_or_else(|| "Untitled".to_string())
        )))
    }
}