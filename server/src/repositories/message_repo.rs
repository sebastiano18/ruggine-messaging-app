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
        // Limita il numero massimo di messaggi
        let safe_limit = limit.min(200).max(1);

        let rows = sqlx::query(
            "SELECT m.id, m.author_id, m.conversation_id, u.username as author_username, 
                    m.content, m.created_at, m.sequence_num
             FROM messages m
             JOIN users u ON m.author_id = u.id
             WHERE m.conversation_id = ?
             ORDER BY COALESCE(m.sequence_num, m.created_at) DESC
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
                let sequence_num: Option<i64> = r.get("sequence_num");

                Message {
                    id: Uuid::parse_str(&id_str).unwrap(),
                    author_id: Uuid::parse_str(&author_str).unwrap(),
                    conversation_id: Uuid::parse_str(&conversation_id_str).unwrap(),
                    author_username,
                    content,
                    created_at,
                    sequence_num,
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
        // Validazione preliminare
        if content.trim().is_empty() {
            return Err(crate::error::AppError::BadRequest("Contenuto vuoto".into()));
        }

        let id = Uuid::new_v4();
        let timestamp = chrono::Utc::now().timestamp();

        // Ottieni la prossima sequence per questa conversazione
        let next_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence_num), 0) + 1 FROM messages WHERE conversation_id = ?"
        )
            .bind(conversation_id.to_string())
            .fetch_one(pool)
            .await?;

        sqlx::query(
            "INSERT INTO messages(id, conversation_id, author_id, content, created_at, sequence_num)
             VALUES(?, ?, ?, ?, ?, ?)",
        )
            .bind(id.to_string())
            .bind(conversation_id.to_string())
            .bind(author_id.to_string())
            .bind(content.trim())
            .bind(timestamp)
            .bind(next_sequence)
            .execute(pool)
            .await?;

        Ok(id)
    }

    pub async fn delete(
        pool: &SqlitePool,
        message_id: Uuid,
        author_id: Uuid,
    ) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM messages WHERE id = ? AND author_id = ?"
        )
            .bind(message_id.to_string())
            .bind(author_id.to_string())
            .execute(pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Ottieni metadata di un messaggio (author_id, conversation_id)
    pub async fn get_metadata(
        pool: &SqlitePool,
        message_id: Uuid,
    ) -> Result<Option<(Uuid, Uuid)>> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT author_id, conversation_id FROM messages WHERE id = ?"
        )
            .bind(message_id.to_string())
            .fetch_optional(pool)
            .await?;

        Ok(row.map(|(author_str, conv_str)| {
            let author_id = Uuid::parse_str(&author_str)
                .expect("DB must store valid UUIDs");
            let conversation_id = Uuid::parse_str(&conv_str)
                .expect("DB must store valid UUIDs");
            (author_id, conversation_id)
        }))
    }

    /// Ottieni la sequence_num di un messaggio
    pub async fn get_sequence(
        pool: &SqlitePool,
        message_id: Uuid,
    ) -> Result<Option<i64>> {
        // fetch_optional restituisce Option<T>, dove T = Option<i64> dalla query
        // Quindi otteniamo Option<Option<i64>>
        let sequence_num: Option<Option<i64>> = sqlx::query_scalar(
            "SELECT sequence_num FROM messages WHERE id = ?"
        )
            .bind(message_id.to_string())
            .fetch_optional(pool)
            .await?;

       
        Ok(sequence_num.and_then(|x| x))
    }
}