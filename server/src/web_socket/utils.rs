use std::collections::HashMap;
use chrono::Utc;
use uuid::Uuid;
use serde_json::{json, Value};
use tracing::{debug, info};

use crate::{
    error::{AppError, Result},
    repositories::conversation_repo::ConversationRepo,
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
    Ok(ConversationRepo::get_conversation_kind(&state.pool, conversation_id)
        .await?
        .is_some())
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

pub async fn send_event_to_multiple_users(
    state: &AppState,
    user_ids: &[Uuid],
    event_type: &str,
    event_data: &Value,
    conversation_id: Option<Uuid>,
) -> Result<Vec<u64>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    const BATCH_THRESHOLD: usize = 10;

    if user_ids.len() >= BATCH_THRESHOLD {
        // ✅ BATCH INSERT per gruppi grandi (>= 10 utenti)
        info!(
            "Batch processing {} '{}' events for {} users",
            user_ids.len(),
            event_type,
            user_ids.len()
        );
        
        let user_sequences = batch_insert_user_events(
            &state.pool,
            user_ids,
            event_type,
            event_data,
            conversation_id,
        ).await?;

        // Broadcast agli utenti online via user_notification_channel
        let channels = state.user_notification_channels.read().await;
        for (user_id, user_seq) in user_ids.iter().zip(&user_sequences) {
            if let Some(tx) = channels.get(user_id) {
                let mut event = event_data.clone();
                event["type"] = json!(event_type);
                event["sequence"] = json!(user_seq);
                event["event_type"] = json!(event_type);
                if let Some(cid) = conversation_id {
                    event["conversation_id"] = json!(cid);
                }
                let _ = tx.send(event); // Best effort
            }
        }

        Ok(user_sequences)
    } else {
        // ✅ LOOP INDIVIDUALE per gruppi piccoli (< 10 utenti)
        debug!(
            "Individual processing {} '{}' events for {} users",
            user_ids.len(),
            event_type,
            user_ids.len()
        );

        let mut sequences = Vec::new();
        for user_id in user_ids {
            let seq = state
                .send_sequenced_event_to_user(
                    *user_id,
                    event_type,
                    event_data.clone(),
                    conversation_id,
                )
                .await?;
            sequences.push(seq);
        }
        Ok(sequences)
    }
}

/// Helper function: Batch INSERT user_events
/// Salva notification LEGGERA (solo riferimenti, non contenuto)

pub(crate) async fn batch_insert_user_events(
    pool: &sqlx::SqlitePool,
    user_ids: &[Uuid],
    event_type: &str,
    payload: &Value,
    conversation_id: Option<Uuid>,
) -> Result<Vec<u64>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut tx = pool.begin().await.map_err(AppError::from)?;
    let ts = Utc::now().timestamp();

    // 1️⃣ Pre-alloca sequenze (questo rimane un loop, ma è relativamente veloce)
    let mut user_sequences: HashMap<Uuid, u64> = HashMap::new();

    for user_id in user_ids {
        let user_id_str = user_id.to_string();

        let current: Option<i64> = sqlx::query_scalar(
            "SELECT current_sequence FROM user_sequences WHERE user_id = ?"
        )
            .bind(&user_id_str)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::from)?;

        if let Some(seq) = current {
            user_sequences.insert(*user_id, (seq + 1) as u64);
        } else {
            sqlx::query(
                "INSERT INTO user_sequences (user_id, current_sequence) VALUES (?, 0)"
            )
                .bind(&user_id_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;

            user_sequences.insert(*user_id, 1);
        }
    }

    // 2️⃣ Serializza payload una volta sola
    let payload_str = serde_json::to_string(payload)
        .map_err(|e| AppError::Internal(format!("JSON serialize: {}", e)))?;

    let conversation_id_str = conversation_id.map(|c| c.to_string());

    // 3️⃣ 🚀 VERO BATCH INSERT - Costruisci una query con VALUES multipli
    // INSERT INTO user_events (...) VALUES (?,?,?,...), (?,?,?,...), (?,?,?,...)

    let placeholders = user_ids
        .iter()
        .map(|_| "(?, ?, ?, ?, ?, ?)")
        .collect::<Vec<_>>()
        .join(", ");

    let query_str = format!(
        "INSERT INTO user_events (user_id, event_type, event_data, sequence_num, conversation_id, created_at) 
         VALUES {}",
        placeholders
    );

    let mut query = sqlx::query(&query_str);
    let mut assigned_sequences = Vec::new();

    // Bind tutti i parametri in ordine
    for user_id in user_ids {
        let seq = *user_sequences.get(user_id).unwrap();
        assigned_sequences.push(seq);

        query = query
            .bind(user_id.to_string())
            .bind(event_type)
            .bind(&payload_str)
            .bind(seq as i64)
            .bind(&conversation_id_str)
            .bind(ts);
    }

    // Esegui la query (1 SOLO comando SQL per 1000 inserimenti!)
    query.execute(&mut *tx).await.map_err(AppError::from)?;

    info!(
        "✅ Batch inserted {} events with 1 SQL command",
        user_ids.len()
    );

    // 4️⃣ 🚀 BATCH UPDATE sequenze - 1 comando con CASE/WHEN
    if !user_sequences.is_empty() {
        // UPDATE user_sequences 
        // SET current_sequence = CASE user_id 
        //   WHEN 'uuid1' THEN seq1 
        //   WHEN 'uuid2' THEN seq2 
        //   ... 
        // END 
        // WHERE user_id IN ('uuid1', 'uuid2', ...)

        let when_clauses = user_sequences
            .iter()
            .map(|(user_id, seq)| format!("WHEN '{}' THEN {}", user_id, seq))
            .collect::<Vec<_>>()
            .join(" ");

        let user_list = user_sequences
            .keys()
            .map(|id| format!("'{}'", id))
            .collect::<Vec<_>>()
            .join(", ");

        let update_query = format!(
            "UPDATE user_sequences 
             SET current_sequence = CASE user_id {} END 
             WHERE user_id IN ({})",
            when_clauses, user_list
        );

        sqlx::query(&update_query)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        info!(
            "✅ Batch updated {} sequences with 1 SQL command",
            user_sequences.len()
        );
    }

    tx.commit().await.map_err(AppError::from)?;

    Ok(assigned_sequences)
}