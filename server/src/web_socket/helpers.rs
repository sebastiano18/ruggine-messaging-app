use chrono::Utc;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, broadcast};
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

pub async fn get_user_conversation_receivers(
    state: &AppState,
    user_id: Uuid,
) -> Result<Vec<broadcast::Receiver<Value>>> {
    let conv_ids: Vec<String> =
        sqlx::query_scalar(r#"SELECT conversation_id FROM participants WHERE user_id = ?"#)
            .bind(user_id.to_string())
            .fetch_all(&state.pool)
            .await
            .map_err(AppError::from)?;

    let mut receivers = Vec::new();

    // Converti gli ID e filtra quelli validi
    let valid_conv_ids: Vec<Uuid> = conv_ids
        .into_iter()
        .filter_map(|s| Uuid::parse_str(&s).ok())
        .collect();

    // Crea i receiver senza tenere il lock troppo a lungo
    for conv_id in valid_conv_ids {
        let tx = state.get_or_create_broadcast_tx(conv_id).await;
        receivers.push(tx.subscribe());
    }

    Ok(receivers)
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

    let conversation_id =
        Uuid::parse_str(cid).map_err(|_| AppError::BadRequest("invalid conversation id".into()))?;

    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("missing content".into()))?;

    // Autorizzazione
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"#,
    )
    .bind(conversation_id.to_string())
    .bind(user_id.to_string())
    .fetch_one(&state.pool)
    .await
    .map_err(AppError::from)?;

    if count == 0 {
        return Err(AppError::Forbidden);
    }

    // Salvataggio nel database
    let id = Uuid::new_v4();
    let ts = Utc::now().timestamp();

    let save_result = sqlx::query(
        r#"INSERT INTO messages (id, conversation_id, author_id, content, created_at)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(id.to_string())
    .bind(conversation_id.to_string())
    .bind(user_id.to_string())
    .bind(content)
    .bind(ts)
    .execute(&state.pool)
    .await;

    // Se il salvataggio fallisce, non fare il broadcast
    if let Err(e) = save_result {
        warn!("Failed to save message {}: {}", id, e);
        return Err(AppError::from(e));
    }

    // Evento per il broadcast
    let event = json!({
        "type": "chat_message",
        "id": id,
        "cid": conversation_id,
        "author_id": user_id,
        "author_username": username,
        "content": content,
        "created_at": ts
    });

    // Broadcast - se fallisce, logga ma non fallire la richiesta
    // visto che il messaggio è già salvato nel DB
    if let Err(e) = broadcast_to_conversation(state, conversation_id, event).await {
        warn!(
            "Broadcast failed for message {} but message was saved: {}",
            id, e
        );
        // Considera l'implementazione di un retry mechanism o queue per i broadcast falliti
    }

    Ok(())
}

// Cleanup intelligente e mirato che usa i metodi thread-safe di AppState
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    // Ottieni le conversazioni dell'utente che potrebbero essere diventate vuote
    // quando si disconnette
    let user_conversations_result = state.get_user_channels(user_id).await;

    match user_conversations_result {
        Ok(user_conversations) => {
            let mut removed_count = 0;

            // Controlla solo i canali delle conversazioni dell'utente
            for (conv_id, _) in user_conversations {
                if state.try_remove_empty_channel(conv_id).await {
                    removed_count += 1;
                }
            }

            if removed_count > 0 {
                info!(
                    "Cleaned up {} empty channels for user {}",
                    removed_count, user_id
                );
            }

            // Log delle statistiche finali
            let (total_channels, total_receivers) = state.get_channel_stats().await;
            info!(
                "Cleanup complete for user {} - System: {} channels, {} receivers",
                user_id, total_channels, total_receivers
            );
        }
        Err(e) => {
            warn!(
                "Failed to get user conversations for cleanup of user {}: {}",
                user_id, e
            );

            // Fallback: cleanup generale (meno efficiente ma funziona)
            cleanup_empty_channels_fallback(&state.channels, user_id).await;
        }
    }
}

// Fallback cleanup per quando non riusciamo a ottenere le conversazioni dell'utente
async fn cleanup_empty_channels_fallback(
    channels: &Arc<RwLock<HashMap<Uuid, broadcast::Sender<Value>>>>,
    user_id: Uuid,
) {
    let candidates_for_removal: Vec<Uuid> = {
        let map = channels.read().await;
        map.iter()
            .filter_map(|(&conv_id, tx)| {
                if tx.receiver_count() == 0 {
                    Some(conv_id)
                } else {
                    None
                }
            })
            .collect()
    };

    if candidates_for_removal.is_empty() {
        return;
    }

    let mut removed_count = 0;
    {
        let mut map = channels.write().await;
        for conv_id in candidates_for_removal {
            if let Some(tx) = map.get(&conv_id) {
                if tx.receiver_count() == 0 {
                    map.remove(&conv_id);
                    removed_count += 1;
                    info!(
                        "Removed empty channel for conversation {} (fallback cleanup)",
                        conv_id
                    );
                }
            }
        }
    }

    if removed_count > 0 {
        info!(
            "Fallback cleanup: {} empty channels removed for user {}",
            removed_count, user_id
        );
    }
}
