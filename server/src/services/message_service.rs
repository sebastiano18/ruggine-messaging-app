use crate::{error::Result, repositories::message_repo::MessageRepo, state::AppState};
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
    ) -> Result<Vec<(Uuid, Uuid, String, String, i64)>> {
        let safe_limit = limit.min(200).max(1);

        let rows = sqlx::query(
            "SELECT m.id, m.author_id, u.username, m.content, m.created_at 
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
        state: &AppState,
    ) -> Result<Uuid> {
        // Validazione contenuto
        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            return Err(crate::error::AppError::BadRequest("Contenuto messaggio vuoto".into()));
        }

        if trimmed_content.len() > 10000 {
            return Err(crate::error::AppError::BadRequest("Messaggio troppo lungo".into()));
        }

        // Modello WhatsApp: auto-join dei partecipanti nelle conversazioni DM
        Self::ensure_dm_participants(pool, conversation_id, author_id).await?;

        let msg_id = MessageRepo::insert(pool, conversation_id, author_id, trimmed_content).await?;

        // Broadcast del messaggio
        let event = json!({
            "type": "chat_message",
            "id": msg_id,
            "cid": conversation_id,
            "conversation_id": conversation_id,
            "author_id": author_id,
            "author_username": author_username,
            "content": trimmed_content,
            "created_at": chrono::Utc::now().timestamp(),
        });

        match broadcast_to_conversation(state, conversation_id, event).await {
            Ok(delivered) => {
                tracing::info!(
                    "Message {} broadcast to {} users in conversation {}",
                    msg_id,
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

    // Funzione per auto-join stile WhatsApp nelle conversazioni DM
    async fn ensure_dm_participants(
        pool: &SqlitePool,
        conversation_id: Uuid,
        author_id: Uuid,
    ) -> Result<()> {
        // Controlla se è una conversazione DM
        let conversation_kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM conversations WHERE id = ?"
        )
            .bind(conversation_id.to_string())
            .fetch_optional(pool)
            .await?;

        let kind = match conversation_kind {
            Some(k) => k,
            None => return Err(crate::error::AppError::NotFound),
        };

        // Solo per conversazioni DM
        if kind != "dm" {
            return Ok(());
        }

        // Trova tutti gli utenti che hanno mai partecipato a questa conversazione
        let all_involved_users: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT author_id FROM messages WHERE conversation_id = ?
             UNION
             SELECT DISTINCT user_id FROM participants WHERE conversation_id = ?"
        )
            .bind(conversation_id.to_string())
            .bind(conversation_id.to_string())
            .fetch_all(pool)
            .await?;

        // Converti in UUID e includi l'autore corrente
        let mut user_ids: std::collections::HashSet<Uuid> = all_involved_users
            .into_iter()
            .filter_map(|id| Uuid::parse_str(&id).ok())
            .collect();

        user_ids.insert(author_id);

        // Assicurati che tutti siano partecipanti
        for user_id in user_ids {
            let is_participant: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
            )
                .bind(conversation_id.to_string())
                .bind(user_id.to_string())
                .fetch_one(pool)
                .await?;

            if is_participant == 0 {
                sqlx::query(
                    "INSERT INTO participants (conversation_id, user_id, role) 
                     VALUES (?, ?, 'member')"
                )
                    .bind(conversation_id.to_string())
                    .bind(user_id.to_string())
                    .execute(pool)
                    .await?;

                tracing::info!(
                    "auto-joined user {} to DM conversation {}",
                    user_id,
                    conversation_id
                );
            }
        }

        Ok(())
    }

    // Broadcast di eventi di sistema
    pub async fn broadcast_system_event(
        state: &AppState,
        conversation_id: Uuid,
        event_type: &str,
        message: &str,
    ) -> Result<()> {
        let event = json!({
            "type": event_type,
            "conversation_id": conversation_id,
            "message": message,
            "timestamp": chrono::Utc::now().timestamp(),
        });

        broadcast_to_conversation(state, conversation_id, event)
            .await
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        Ok(())
    }

    // Indicatori di scrittura
    pub async fn broadcast_typing_indicator(
        state: &AppState,
        conversation_id: Uuid,
        user_id: Uuid,
        username: &str,
        is_typing: bool,
    ) -> Result<()> {
        let event = json!({
            "type": "typing",
            "conversation_id": conversation_id,
            "user_id": user_id,
            "username": username,
            "is_typing": is_typing,
            "timestamp": chrono::Utc::now().timestamp(),
        });

        broadcast_to_conversation(state, conversation_id, event)
            .await
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        Ok(())
    }

    // Notifica utente entrato
    pub async fn broadcast_user_joined(
        state: &AppState,
        conversation_id: Uuid,
        joined_user_id: Uuid,
        joined_username: &str,
    ) -> Result<()> {
        let event = json!({
            "type": "user_joined",
            "conversation_id": conversation_id,
            "user_id": joined_user_id,
            "username": joined_username,
            "message": format!("{} si è unito alla conversazione", joined_username),
            "timestamp": chrono::Utc::now().timestamp(),
        });

        broadcast_to_conversation(state, conversation_id, event)
            .await
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        Ok(())
    }

    // Notifica utente uscito
    pub async fn broadcast_user_left(
        state: &AppState,
        conversation_id: Uuid,
        left_user_id: Uuid,
        left_username: &str,
    ) -> Result<()> {
        let event = json!({
            "type": "user_left",
            "conversation_id": conversation_id,
            "user_id": left_user_id,
            "username": left_username,
            "message": format!("{} ha lasciato la conversazione", left_username),
            "timestamp": chrono::Utc::now().timestamp(),
        });

        broadcast_to_conversation(state, conversation_id, event)
            .await
            .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

        Ok(())
    }
}