use crate::error::Result;
use crate::models::Message;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub struct MessageRepo;

impl MessageRepo {
    pub async fn list(
        pool: &SqlitePool,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Message>> {
        // CORREZIONE: Limita il numero massimo di messaggi
        let safe_limit = limit.min(200).max(1);

        let rows = sqlx::query(
            "SELECT m.id, m.author_id, m.conversation_id, u.username as author_username, m.content, m.created_at 
         FROM messages m 
         JOIN users u ON m.author_id = u.id
         WHERE m.conversation_id = ? 
         ORDER BY m.created_at DESC 
         LIMIT ?",
        )
            .bind(conversation_id.to_string())
            .bind(safe_limit)
            .fetch_all(pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let id_str: String = r.get("id");
                let author_str: String = r.get("author_id");
                let conversation_id_str: String = r.get("conversation_id");
                let author_username: String = r.get("author_username");
                let content: String = r.get("content");
                let created_at: i64 = r.get("created_at");

                Message {
                    id: Uuid::parse_str(&id_str).unwrap(),
                    author_id: Uuid::parse_str(&author_str).unwrap(),
                    conversation_id: Uuid::parse_str(&conversation_id_str).unwrap(), // CORREZIONE: Usa il valore dal DB, non il parametro
                    author_username,
                    content,
                    created_at,
                }
            })
            .collect())
    }

    pub async fn insert(
        pool: &SqlitePool,
        conversation_id: Uuid,
        author_id: Uuid,
        content: &str,
    ) -> Result<Uuid> {
        // CORREZIONE: Validazione preliminare
        if content.trim().is_empty() {
            return Err(crate::error::AppError::BadRequest("Contenuto vuoto".into()));
        }

        let id = Uuid::new_v4();
        let timestamp = chrono::Utc::now().timestamp(); // CORREZIONE: Usa timestamp consistente

        sqlx::query(
            "INSERT INTO messages(id, conversation_id, author_id, content, created_at)
             VALUES(?, ?, ?, ?, ?)",
        )
            .bind(id.to_string())
            .bind(conversation_id.to_string())
            .bind(author_id.to_string())
            .bind(content.trim()) // CORREZIONE: Trim del contenuto
            .bind(timestamp) // CORREZIONE: Usa timestamp esplicito invece di strftime
            .execute(pool)
            .await?;

        Ok(id)
    }
}