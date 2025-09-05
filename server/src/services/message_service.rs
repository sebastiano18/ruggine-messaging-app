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
        state: &AppState,
    ) -> Result<Uuid> {
        let msg_id = MessageRepo::insert(pool, conversation_id, author_id, content).await?;

        // CAMBIAMENTO: Usa broadcast_to_conversation invece di ws_broadcast globale
        let event = json!({
            "type": "message",
            "conversation_id": conversation_id,
            "id": msg_id,
            "author_id": author_id,
            "author_username": author_username,
            "content": content,
            "created_at": chrono::Utc::now().timestamp(), // Aggiungi timestamp per consistenza
        });

        // Invia solo ai partecipanti di questa conversazione
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
                // Non fallire l'operazione anche se il broadcast fallisce
                // Il messaggio è comunque salvato nel database
            }
        }

        Ok(msg_id)
    }

    // NUOVA FUNZIONE: Broadcast di eventi di sistema a una conversazione
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

    // NUOVA FUNZIONE: Notifica quando un utente inizia a digitare
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

    // NUOVA FUNZIONE: Notifica quando un nuovo utente si unisce alla conversazione
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

    // NUOVA FUNZIONE: Notifica quando un utente lascia la conversazione
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
