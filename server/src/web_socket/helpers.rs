use serde_json::{json, Value};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;
use std::{collections::HashMap, sync::Arc};
use tracing::{info, warn};
use chrono::Utc;

use crate::{state::AppState, error::{AppError, Result}};

pub async fn broadcast_to_conversation(
    state: &AppState,
    conversation_id: Uuid,
    payload: Value,
) -> Result<usize> {
    let tx = {
        let mut map = state.channels.write().await;
        // Miglioramento: usa entry() per evitare race conditions
        map.entry(conversation_id)
            .or_insert_with(|| {
                let (tx, _rx) = broadcast::channel::<Value>(1024);
                tx
            })
            .clone()
    };
    match tx.send(payload) {
        Ok(n) => { info!("broadcast {} subs for {}", n, conversation_id); Ok(n) }
        Err(e) => { warn!("broadcast fail {}: {}", conversation_id, e); Err(AppError::Internal(format!("broadcast error: {e}"))) }
    }
}

pub async fn get_user_conversation_receivers(
    state: &AppState,
    user_id: Uuid,
) -> Result<Vec<broadcast::Receiver<Value>>> {
    let conv_ids: Vec<String> = sqlx::query_scalar(
        r#"SELECT conversation_id FROM participants WHERE user_id = ?"#,
    )
        .bind(user_id.to_string())
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let mut res = Vec::new();
    let mut guard = state.channels.write().await;
    for s in conv_ids {
        if let Ok(cid) = Uuid::parse_str(&s) {
            // Miglioramento: usa entry() anche qui
            let tx = guard.entry(cid)
                .or_insert_with(|| {
                    let (tx, _rx) = broadcast::channel::<Value>(1024);
                    tx
                })
                .clone();
            res.push(tx.subscribe());
        }
    }
    Ok(res)
}

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

    let conversation_id = Uuid::parse_str(cid)
        .map_err(|_| AppError::BadRequest("invalid conversation id".into()))?;

    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing content".into()))?;

    // autorizzazione
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"#,
    )
        .bind(conversation_id.to_string())
        .bind(user_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;
    if count == 0 { return Err(AppError::Forbidden); }

    // salvataggio
    let id = Uuid::new_v4();
    let ts = Utc::now().timestamp();

    sqlx::query(
        r#"INSERT INTO messages (id, conversation_id, author_id, content, created_at)
           VALUES (?, ?, ?, ?, ?)"#,
    )
        .bind(id.to_string())
        .bind(conversation_id.to_string())
        .bind(user_id.to_string())
        .bind(content)
        .bind(ts)
        .execute(&state.pool)
        .await
        .map_err(AppError::from)?;

    // evento
    let event = json!({
        "type": "chat_message",
        "id": id,
        "cid": conversation_id,
        "author_id": user_id,
        "author_username": username,
        "content": content,
        "created_at": ts
    });

    broadcast_to_conversation(state, conversation_id, event).await?;
    Ok(())
}

// Miglioramento: cleanup più completo con logging
pub async fn cleanup_empty_channels(
    channels: &Arc<RwLock<HashMap<Uuid, broadcast::Sender<Value>>>>,
    user_id: Uuid,
) {
    let mut map = channels.write().await;
    let initial_count = map.len();
    map.retain(|conv_id, tx| {
        let keep = tx.receiver_count() > 0;
        if !keep {
            info!("Removing empty channel for conversation {} (user {} disconnected)", conv_id, user_id);
        }
        keep
    });
    let removed_count = initial_count - map.len();
    if removed_count > 0 {
        info!("Cleaned up {} empty channels for user {}", removed_count, user_id);
    }
}