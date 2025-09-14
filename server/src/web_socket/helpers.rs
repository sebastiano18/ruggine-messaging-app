// helpers.rs - Versione semplificata

use chrono::Utc;
use serde_json::{Value, json};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

pub async fn broadcast_to_conversation(
    state: &AppState,
    conversation_id: Uuid,
    payload: Value,
) -> Result<usize> {
    let tx = state.get_or_create_broadcast_tx(conversation_id).await;
    match tx.send(payload) {
        Ok(n) => {
            info!("broadcast {} subs for {}", n, conversation_id);
            Ok(n)
        }
        Err(e) => {
            warn!("broadcast fail {}: {}", conversation_id, e);
            Err(AppError::Internal(format!("broadcast error: {e}")))
        }
    }
}

/// SEMPLIFICATO: Non notificare durante creazione conversazione
pub async fn handle_chat_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    let cid = value
        .get("cid")
        .or_else(|| value.get("conversation_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing conversation id (cid)".into()))?;

    let conversation_id =
        Uuid::parse_str(cid).map_err(|_| AppError::BadRequest("invalid conversation id".into()))?;

    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing content".into()))?;

    let target_username = value
        .get("target_username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let conversation_id_str = conversation_id.to_string();
    let conversation_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ?",
    )
        .bind(&conversation_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    let mut is_new_conversation = false;

    if conversation_exists == 0 {
        info!("Creating new DM conversation {} for first message from user {}", conversation_id, user_id);
        is_new_conversation = true;

        if let Some(ref target_user) = target_username {
            let target_id: Option<String> = sqlx::query_scalar(
                "SELECT id FROM users WHERE username = ?"
            )
                .bind(target_user)
                .fetch_optional(&state.pool)
                .await
                .map_err(AppError::from)?;

            match target_id {
                Some(id_str) => {
                    let target_user_id = Uuid::parse_str(&id_str)
                        .map_err(|_| AppError::BadRequest("Invalid target user ID".into()))?;

                    // Crea la conversazione DM
                    let user_id_str = user_id.to_string();
                    sqlx::query(
                        "INSERT INTO conversations(id, kind, title, owner_id, created_at)
                         VALUES(?, 'dm', NULL, ?, strftime('%s','now'))",
                    )
                        .bind(&conversation_id_str)
                        .bind(&user_id_str)
                        .execute(&state.pool)
                        .await
                        .map_err(AppError::from)?;

                    // Aggiungi entrambi i partecipanti
                    sqlx::query(
                        "INSERT INTO participants(conversation_id, user_id, role)
                         VALUES(?, ?, 'member')",
                    )
                        .bind(&conversation_id_str)
                        .bind(&user_id_str)
                        .execute(&state.pool)
                        .await
                        .map_err(AppError::from)?;

                    let target_id_str = target_user_id.to_string();
                    sqlx::query(
                        "INSERT INTO participants(conversation_id, user_id, role)
                         VALUES(?, ?, 'member')",
                    )
                        .bind(&conversation_id_str)
                        .bind(&target_id_str)
                        .execute(&state.pool)
                        .await
                        .map_err(AppError::from)?;

                    info!("Created DM conversation {} and added both users {} and {}", 
                          conversation_id, user_id, target_user_id);
                }
                None => {
                    return Err(AppError::BadRequest(
                        format!("Target user '{}' not found", target_user)
                    ));
                }
            }
        } else {
            return Err(AppError::BadRequest("Target username required for new DM".into()));
        }
    } else {
        // Conversazione esistente - controllo autorizzazione normale
        let user_id_str = user_id.to_string();
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"#,
        )
            .bind(&conversation_id_str)
            .bind(&user_id_str)
            .fetch_one(&state.pool)
            .await
            .map_err(AppError::from)?;

        if count == 0 {
            return Err(AppError::Forbidden);
        }
    }

    // Salvataggio del messaggio nel database - SEMPRE
    let id = Uuid::new_v4();
    let ts = Utc::now().timestamp();
    let id_str = id.to_string();
    let user_id_str = user_id.to_string();

    sqlx::query(
        r#"INSERT INTO messages (id, conversation_id, author_id, content, created_at)
           VALUES (?, ?, ?, ?, ?)"#,
    )
        .bind(&id_str)
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .bind(content)
        .bind(ts)
        .execute(&state.pool)
        .await
        .map_err(AppError::from)?;

    info!("Message {} saved to DB", id);

    // IMPORTANTE: Solo broadcast se NON è una nuova conversazione
    if !is_new_conversation {
        let event = json!({
            "type": "chat_message",
            "id": id,
            "cid": conversation_id,
            "author_id": user_id,
            "author_username": username,
            "content": content,
            "created_at": ts
        });

        match broadcast_to_conversation(state, conversation_id, event).await {
            Ok(delivered) if delivered > 0 => {
                info!("Message {} delivered immediately to {} receivers", id, delivered);
            }
            _ => {
                info!("Message {} stored - no active receivers", id);
            }
        }
    } else {
        info!("New conversation created - message {} will be fetched when users subscribe", id);
    }

    Ok(())
}

/// Response struct per l'API di fetch messaggi
#[derive(serde::Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub author_id: String,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
}

/// Endpoint API per fetch messaggi conversazione
pub async fn get_conversation_messages_api(
    state: &AppState,
    conversation_id: Uuid,
    user_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<MessageResponse>> {
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    // Verifica autorizzazione
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
    )
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if count == 0 {
        return Err(AppError::Forbidden);
    }

    let limit = limit.unwrap_or(50).min(100);

    let messages = sqlx::query!(
        "SELECT m.id, m.author_id, u.username as author_username, m.content, m.created_at
         FROM messages m
         JOIN users u ON m.author_id = u.id
         WHERE m.conversation_id = ?
         ORDER BY m.created_at ASC
         LIMIT ?",
        conversation_id_str,
        limit
    )
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let response: Vec<MessageResponse> = messages
        .into_iter()
        .map(|row| MessageResponse {
            id: row.id.expect("REASON"),
            author_id: row.author_id,
            author_username: row.author_username,
            content: row.content,
            created_at: row.created_at,
        })
        .collect();

    info!("Fetched {} messages for conversation {} (user {})",
          response.len(), conversation_id, user_id);

    Ok(response)
}

// Cleanup (rimane uguale)
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    let user_conversations_result = state.get_user_channels(user_id).await;

    match user_conversations_result {
        Ok(user_conversations) => {
            let mut removed_count = 0;

            for (conv_id, _) in user_conversations {
                if state.try_remove_empty_channel(conv_id).await {
                    removed_count += 1;
                }
            }

            if removed_count > 0 {
                info!("Cleaned up {} empty channels for user {}", removed_count, user_id);
            }
        }
        Err(e) => {
            warn!("Failed to get user conversations for cleanup of user {}: {}", user_id, e);
        }
    }
}

//===========================================
// recv_merge.rs - Aggiornato per triggare fetch su join

