use crate::{error::Result, repositories::message_repo::MessageRepo, state::AppState};
use crate::models::Message;
use serde_json::json;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;
use crate::web_socket::helpers::broadcast_to_conversation;

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

    /// Post di un messaggio con lazy registration gestita tramite WebSocket
    pub async fn post(
        pool: &SqlitePool,
        conversation_id: Uuid,
        author_id: Uuid,
        author_username: String,
        content: &str,
        state: &AppState,
    ) -> Result<Uuid> {
        // Validazione contenuto
        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            return Err(crate::error::AppError::BadRequest(
                "Contenuto messaggio vuoto".into(),
            ));
        }

        if trimmed_content.len() > 10000 {
            return Err(crate::error::AppError::BadRequest(
                "Messaggio troppo lungo".into(),
            ));
        }

        // Verifica che la conversazione esista
        let conversation_exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
                .bind(conversation_id.to_string())
                .fetch_one(pool)
                .await?;

        if conversation_exists == 0 {
            return Err(crate::error::AppError::NotFound);
        }

        // Verifica che l'utente sia autorizzato (partecipante della conversazione)
        let is_participant: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?",
        )
            .bind(conversation_id.to_string())
            .bind(author_id.to_string())
            .fetch_one(pool)
            .await?;

        if is_participant == 0 {
            return Err(crate::error::AppError::Forbidden);
        }

        let msg_id = MessageRepo::insert(pool, conversation_id, author_id, trimmed_content).await?;

        // Ottieni la sequence del messaggio appena inserito
        let sequence_num: Option<i64> = sqlx::query_scalar(
            "SELECT sequence_num FROM messages WHERE id = ?"
        )
            .bind(msg_id.to_string())
            .fetch_one(pool)
            .await?;

        // Broadcast del messaggio con sequence
        let event = json!({
            "type": "chat_message",
            "id": msg_id,
            "cid": conversation_id,
            "conversation_id": conversation_id,
            "author_id": author_id,
            "author_username": author_username,
            "content": trimmed_content,
            "created_at": chrono::Utc::now().timestamp(),
            "sequence": sequence_num,
        });

        match broadcast_to_conversation(state, conversation_id, event).await {
            Ok(delivered) => {
                tracing::info!(
                    "Message {} (seq: {:?}) broadcast to {} users in conversation {}",
                    msg_id,
                    sequence_num,
                    delivered,
                    conversation_id
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to broadcast message {} to conversation {}: {}",
                    msg_id,
                    conversation_id,
                    e
                );
            }
        }

        Ok(msg_id)
    }

    pub async fn delete_message(
        pool: &SqlitePool,
        message_id: Uuid,
        requester_id: Uuid,
    ) -> Result<Uuid> {
        // 1. Find the message to get conversation_id and verify the author
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT author_id, conversation_id FROM messages WHERE id = ?",
        )
        .bind(message_id.to_string())
        .fetch_optional(pool)
        .await?;

        let (author_id_str, conversation_id_str) = match row {
            Some((author, conv)) => (author, conv),
            None => return Err(crate::error::AppError::NotFound),
        };

        let author_id = Uuid::parse_str(&author_id_str)
            .map_err(|_| crate::error::AppError::Internal("Invalid author_id in DB".into()))?;
        
        let conversation_id = Uuid::parse_str(&conversation_id_str)
            .map_err(|_| crate::error::AppError::Internal("Invalid conversation_id in DB".into()))?;

        // 2. Authorization check: only the author can delete the message
        if author_id != requester_id {
            return Err(crate::error::AppError::Forbidden);
        }

        // 3. Delete the message
        let result = sqlx::query("DELETE FROM messages WHERE id = ?")
            .bind(message_id.to_string())
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            // This could happen in a race condition where the message was already deleted.
            // We can treat it as a success from the client's perspective.
            tracing::warn!("Attempted to delete message {} which was already deleted.", message_id);
        }

        // 4. Return the conversation_id for broadcasting purposes
        Ok(conversation_id)
    }

    pub async fn delete(
        pool: &SqlitePool,
        message_id: Uuid,
        author_id: Uuid,
        state: &AppState,
    ) -> Result<()> {
        // 1. Trova il messaggio per ottenere conversation_id e verificare l'autore
        let message: Option<(String, String, String)> = sqlx::query_as(
            "SELECT id, author_id, conversation_id FROM messages WHERE id = ?",
        )
        .bind(message_id.to_string())
        .fetch_optional(pool)
        .await?;

        let (message_id_str, author_id_str, conversation_id_str) = match message {
            Some((id, author, conv)) => (id, author, conv),
            None => return Err(crate::error::AppError::NotFound),
        };

        let db_author_id = Uuid::parse_str(&author_id_str).unwrap_or_default();
        let conversation_id = Uuid::parse_str(&conversation_id_str).unwrap_or_default();

        // 2. Verifica che l'utente che elimina sia l'autore del messaggio
        if db_author_id != author_id {
            return Err(crate::error::AppError::Forbidden);
        }

        // 3. Elimina il messaggio
        let rows_affected = MessageRepo::delete(pool, message_id, author_id).await?;

        if rows_affected > 0 {
            // 4. Broadcast dell'evento di eliminazione
            let event = json!({
                "type": "message_deleted",
                "message_id": message_id,
                "conversation_id": conversation_id,
            });

            match broadcast_to_conversation(state, conversation_id, event).await {
                Ok(delivered) => {
                    tracing::info!(
                        "Broadcast delete for message {} to {} users in conversation {}",
                        message_id,
                        delivered,
                        conversation_id
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to broadcast delete for message {} to conversation {}: {}",
                        message_id,
                        conversation_id,
                        e
                    );
                }
            }
        }

        Ok(())
    }
}