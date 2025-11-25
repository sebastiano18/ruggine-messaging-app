use uuid::Uuid;
use serde_json::Value;
use sqlx::Row;
use tracing::debug;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

/// Struttura per i dati di una nuova conversazione
#[derive(Debug)]
pub struct NewConversationData {
    pub conversation_id: Uuid,
    pub client_temp_id: Option<String>,
    pub creator_id: Uuid,
    pub creator_username: String,
    pub other_participant_id: Uuid,
    pub other_participant_username: String,
    pub initial_message_id: Uuid,
    pub initial_message_content: String,
    pub initial_message_sequence: u64,
    pub client_msg_id: Option<String>,
    pub created_at: i64,
}

/// Verifica se una conversazione esiste nel database
pub async fn verify_conversation_exists(state: &AppState, conversation_id: Uuid) -> Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE id = ?")
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    Ok(count > 0)
}

/// Estrae l'ID della conversazione dal valore, gestendo anche client_temp_id
pub async fn extract_conversation_id(state: &AppState, value: &Value) -> Result<Uuid> {
    // PRIORITÀ 1: Se c'è un client_temp_id esplicito, usa quello
    if let Some(temp_id) = value.get("client_temp_id").and_then(|v| v.as_str()) {
        if let Some(real_id) = state
            .conversation_confirmation_cache
            .get_by_temp_id(temp_id)
            .await
        {
            debug!(
                "Resolved client_temp_id {} to conversation {}",
                temp_id, real_id
            );
            return Ok(real_id);
        }
        // client_temp_id non trovato nella cache - NON è un errore
        // Potrebbe essere il primo messaggio
        debug!(
            "client_temp_id {} not found in cache, might be first message",
            temp_id
        );
    }

    // PRIORITÀ 2: Usa conversation_id/cid normale
    if let Some(cid_str) = value
        .get("conversation_id")
        .or_else(|| value.get("cid"))
        .and_then(|v| v.as_str())
    {
        // Prova a parsare come UUID
        if let Ok(uuid) = Uuid::parse_str(cid_str) {
            // IMPORTANTE: Verifica che la conversazione esista nel DB
            if verify_conversation_exists(state, uuid).await? {
                return Ok(uuid);
            }

            // UUID non esistente - potrebbe essere nella cache come temp_id
            if let Some(real_id) = state
                .conversation_confirmation_cache
                .get_by_temp_id(cid_str)
                .await
            {
                debug!(
                    "Found UUID {} in temp_id cache, resolved to {}",
                    cid_str, real_id
                );
                return Ok(real_id);
            }

            return Err(AppError::BadRequest(format!(
                "Conversation {} does not exist",
                uuid
            )));
        }

        // Non è un UUID valido, prova come client_temp_id nella cache
        if let Some(real_id) = state
            .conversation_confirmation_cache
            .get_by_temp_id(cid_str)
            .await
        {
            debug!(
                "Resolved non-UUID client_temp_id {} to conversation {}",
                cid_str, real_id
            );
            return Ok(real_id);
        }

        return Err(AppError::BadRequest(format!(
            "Invalid conversation ID: {}",
            cid_str
        )));
    }

    Err(AppError::BadRequest("Missing conversation ID".into()))
}