use crate::{error::Result, repositories::message_repo::MessageRepo, repositories::conversation_repo::ConversationRepo, state::AppState};
use crate::models::Message;
use serde_json::json;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use crate::web_socket::broadcast::broadcast_to_conversation;

pub struct MessageService;

impl MessageService {
    pub async fn list(
        pool: &SqlitePool,
        conversation_id: Uuid,
        limit: i64,
    ) -> Result<Vec<(Uuid, Uuid, String, String, i64, Option<i64>)>> {
        let safe_limit = limit.min(200).max(1);

        let rows = sqlx::query(
            "SELECT m.id, m.author_id, u.username, m.content, m.created_at, m.sequence_num
             FROM messages m
             JOIN users u ON m.author_id = u.id
             WHERE m.conversation_id = ?
             ORDER BY COALESCE(m.sequence_num, m.created_at) ASC
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
                let username: String = r.get("username");
                let content: String = r.get("content");
                let created_at: i64 = r.get("created_at");
                let sequence_num: Option<i64> = r.get("sequence_num");
                (
                    Uuid::parse_str(&id_str).unwrap(),
                    Uuid::parse_str(&author_str).unwrap(),
                    username,
                    content,
                    created_at,
                    sequence_num,
                )
            })
            .collect())
    }

    /// Lista paginata semplice con before_sequence
    pub async fn list_with_pagination(
        pool: &SqlitePool,
        conversation_id: Uuid,
        limit: i64,
        before_sequence: Option<i64>,
    ) -> Result<Vec<Message>> {
        let safe_limit = limit.min(100).max(1);

        let rows = if let Some(before_seq) = before_sequence {
            // Carica messaggi più vecchi
            sqlx::query(
                "SELECT m.id, m.author_id, u.username, m.content, m.created_at, m.sequence_num
                 FROM messages m
                 JOIN users u ON m.author_id = u.id
                 WHERE m.conversation_id = ? AND m.sequence_num < ?
                 ORDER BY m.sequence_num DESC
                 LIMIT ?"
            )
                .bind(conversation_id.to_string())
                .bind(before_seq)
                .bind(safe_limit)
                .fetch_all(pool)
                .await?
        } else {
            // Carica ultimi messaggi
            sqlx::query(
                "SELECT m.id, m.author_id, u.username, m.content, m.created_at, m.sequence_num
                 FROM messages m
                 JOIN users u ON m.author_id = u.id
                 WHERE m.conversation_id = ?
                 ORDER BY m.sequence_num DESC
                 LIMIT ?"
            )
                .bind(conversation_id.to_string())
                .bind(safe_limit)
                .fetch_all(pool)
                .await?
        };

        Ok(rows
            .into_iter()
            .map(|r| {
                let id_str: String = r.get("id");
                let author_str: String = r.get("author_id");
                Message {
                    id: Uuid::parse_str(&id_str).unwrap(),
                    author_id: Uuid::parse_str(&author_str).unwrap(),
                    conversation_id,
                    author_username: r.get("username"),
                    content: r.get("content"),
                    created_at: r.get("created_at"),
                    sequence_num: r.get("sequence_num"),
                }
            })
            .rev() // Inverti per ordine cronologico
            .collect())
    }


    pub async fn delete_message(
        pool: &SqlitePool,
        message_id: Uuid,
        requester_id: Uuid,
    ) -> Result<Uuid> {
        // 1. Find the message to get conversation_id and verify the author
        let (author_id, conversation_id) = MessageRepo::get_metadata(pool, message_id)
            .await?
            .ok_or(crate::error::AppError::NotFound)?;

        // 2. Authorization check: only the author can delete the message
        if author_id != requester_id {
            return Err(crate::error::AppError::Forbidden);
        }

        // 3. Delete the message using repo
        let rows_affected = MessageRepo::delete(pool, message_id, author_id).await?;

        if rows_affected == 0 {
            // This could happen in a race condition where the message was already deleted.
            // We can treat it as a success from the client's perspective.
            tracing::warn!("Attempted to delete message {} which was already deleted.", message_id);
        }

        // 4. Return the conversation_id for broadcasting purposes
        Ok(conversation_id)
    }
    
}