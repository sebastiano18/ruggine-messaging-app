use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
};
use super::utils::NewConversationData;
use super::actor::OutboundMsg;

/// Broadcast di un messaggio a tutti i partecipanti di una conversazione
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
        Err(_e) => {
            // Non è un errore critico: i messaggi sono già salvati nel DB e arrivano via user_events
            // Ma logghiamo come warn per monitorare questi casi
            warn!("broadcast {}: no active receivers (message delivered via user_events)", conversation_id);
            Ok(0)
        }
    }
}

/// Invia conferma di un messaggio al mittente
pub async fn send_message_confirmation(
    state: &AppState,
    msg_id: Uuid,
    conversation_id: Uuid,
    sequence: u64,
    client_msg_id: Option<String>,
    created_at: i64,
    user_id: Uuid,
) -> Result<()> {
    let confirmation = json!({
        "type": "message_confirmation",
        "client_msg_id": client_msg_id,
        "server_msg_id": msg_id.to_string(),
        "conversation_id": conversation_id,
        "sequence": sequence,
        "created_at": created_at,
        "status": "saved"
    });

    let user_tx = state.get_or_create_user_notification_channel(user_id).await;
    match user_tx.send(confirmation) {
        Ok(receiver_count) => {
            debug!(
                "Sent message confirmation to user {} ({} receivers)",
                user_id, receiver_count
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                "Failed to send message confirmation to user {}: {}",
                user_id, e
            );
            Err(AppError::Internal(format!(
                "Failed to send confirmation: {}",
                e
            )))
        }
    }
}

/// Helper privato: invia un singolo evento di conversazione a un utente
async fn send_conversation_event(
    state: &AppState,
    user_id: Uuid,
    event_type: &str,
    event_payload: Value,
    conversation_id: Uuid,
) -> Result<()> {
    match state
        .send_sequenced_event_to_user(user_id, event_type, event_payload, Some(conversation_id))
        .await
    {
        Ok(seq) => {
            info!("Sent {} (seq={}) to user {}", event_type, seq, user_id);
            Ok(())
        }
        Err(e) => {
            warn!("Failed to send {} to user {}: {}", event_type, user_id, e);
            Err(e)
        }
    }
}

/// Invia eventi di creazione DM: conferma al creatore, notifica all'altro partecipante
pub async fn send_conversation_created_events(
    state: &AppState,
    data: NewConversationData,
) -> Result<()> {
    // Costruisci il messaggio iniziale se presente
    let last_message = if data.initial_message_id != Uuid::nil() {
        let mut last_msg = json!({
            "id": data.initial_message_id,
            "author_id": data.creator_id,
            "author_username": data.creator_username.clone(),
            "content": data.initial_message_content.clone(),
            "created_at": data.created_at,
            "sequence_num": data.initial_message_sequence
        });

        if let Some(ref client_id) = data.client_msg_id {
            last_msg["client_msg_id"] = json!(client_id);
        }

        Some(last_msg)
    } else {
        None
    };

    // === Invia conferma al creatore ===
    let mut creator_conversation = json!({
        "id": data.conversation_id,
        "kind": "dm",
        "title": null,
        "display_title": data.other_participant_username.clone(),
        "owner_id": data.creator_id,
        "created_at": data.created_at,
        "message_count": if data.initial_message_id != Uuid::nil() { 1 } else { 0 },
        "participants": [
            {
                "user_id": data.creator_id.to_string(),
                "username": data.creator_username.clone(),
                "role": "member"
            },
            {
                "user_id": data.other_participant_id.to_string(),
                "username": data.other_participant_username.clone(),
                "role": "member"
            }
        ]
    });

    if let Some(ref last_msg) = last_message {
        creator_conversation["last_message"] = last_msg.clone();
    }

    // Aggiungi client_temp_id per il creatore
    if let Some(ref temp_id) = data.client_temp_id {
        creator_conversation["client_temp_id"] = json!(temp_id);
    } else {
        warn!("DM creator {} missing client_temp_id", data.creator_id);
    }

    let creator_event = json!({
        "conversation": creator_conversation
    });

    send_conversation_event(
        state,
        data.creator_id,
        "conversation_confirmation",
        creator_event,
        data.conversation_id,
    )
        .await?;

    // === Notifica l'altro partecipante ===
    let mut other_conversation = json!({
        "id": data.conversation_id,
        "kind": "dm",
        "title": null,
        "display_title": data.creator_username.clone(),
        "owner_id": data.creator_id,
        "created_at": data.created_at,
        "message_count": if data.initial_message_id != Uuid::nil() { 1 } else { 0 },
        "participants": [
            {
                "user_id": data.creator_id.to_string(),
                "username": data.creator_username.clone(),
                "role": "member"
            },
            {
                "user_id": data.other_participant_id.to_string(),
                "username": data.other_participant_username.clone(),
                "role": "member"
            }
        ]
    });

    if let Some(ref last_msg) = last_message {
        other_conversation["last_message"] = last_msg.clone();
    }

    let other_event = json!({
        "conversation": other_conversation
    });

    send_conversation_event(
        state,
        data.other_participant_id,
        "new_conversation",
        other_event,
        data.conversation_id,
    )
        .await?;

    Ok(())
}

/// Invia eventi di creazione gruppo a tutti i partecipanti
pub async fn send_conversation_created_group_complete(
    state: &AppState,
    conversation_id: Uuid,
    client_temp_id: Option<String>,
    creator_id: Uuid,
    group_name: String,
    created_at: i64,
    participant_ids: Vec<Uuid>,
) -> Result<()> {
    info!(
        "Sending group creation events for {} to {} participants",
        conversation_id, participant_ids.len()
    );

    // Recupera la lista dei membri con dettagli (username, role)
    let members_query = r#"
        SELECT p.user_id, u.username, p.role
        FROM participants p
        INNER JOIN users u ON p.user_id = u.id
        WHERE p.conversation_id = ?
        ORDER BY CASE WHEN LOWER(p.role) = 'owner' THEN 0 ELSE 1 END,
                 LOWER(u.username) ASC
    "#;

    let member_rows = sqlx::query(members_query)
        .bind(conversation_id.to_string())
        .fetch_all(&state.pool)
        .await
        .map_err(AppError::from)?;

    let mut members = Vec::new();
    for row in member_rows {
        let member_user_id: String = row.try_get("user_id").map_err(AppError::from)?;
        let username: String = row.try_get("username").map_err(AppError::from)?;
        let role: String = row.try_get("role").map_err(AppError::from)?;

        members.push(json!({
            "user_id": member_user_id,
            "username": username,
            "role": role
        }));
    }

    // Invia eventi personalizzati per ogni partecipante
    for participant_id in participant_ids {
        let last_read_sequence: i64 = sqlx::query_scalar(
            "SELECT last_read_sequence FROM participants WHERE conversation_id = ? AND user_id = ?"
        )
            .bind(conversation_id.to_string())
            .bind(participant_id.to_string())
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(0);

        let mut conversation_obj = json!({
            "id": conversation_id,
            "kind": "group",
            "title": group_name,
            "owner_id": creator_id,
            "created_at": created_at,
            "last_read_sequence": last_read_sequence,
            "last_msg_seq": 0,
            "message_count": 0,
            "members": members.clone()
        });

        // Determina il tipo di evento: creatore vs altri membri
        let event_type = if participant_id == creator_id {
            // Creatore → conversation_created_complete (con temp_id se presente)
            if let Some(ref temp_id) = client_temp_id {
                conversation_obj["client_temp_id"] = json!(temp_id);
            }
            "conversation_created_complete"
        } else {
            // Altri membri → new_conversation
            "new_conversation"
        };

        let event = json!({
            "conversation": conversation_obj
        });

        send_conversation_event(
            state,
            participant_id,
            event_type,
            event,
            conversation_id,
        )
            .await?;
    }

    Ok(())
}

/// Configura l'iscrizione dell'utente al broadcast channel della conversazione
pub async fn setup_conversation_subscription(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<()> {
    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    let is_participant: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?",
    )
        .bind(&conversation_id_str)
        .bind(&user_id_str)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    if is_participant == 0 {
        return Err(AppError::Forbidden);
    }

    let conv_tx = state.get_or_create_broadcast_tx(conversation_id).await;
    let receiver_count = conv_tx.receiver_count();

    info!(
        "Conversation {} broadcast channel ready for user {} ({} current receivers)",
        conversation_id, user_id, receiver_count
    );

    Ok(())
}

/// Cleanup canali vuoti quando un utente si disconnette
pub async fn cleanup_empty_channels(state: &AppState, user_id: Uuid) {
    // CRITICAL: Non fare cleanup se l'utente ha una connessione attiva
    // Questo previene race condition dove cleanup task della vecchia sessione
    // rimuove canali broadcast che la NUOVA sessione sta usando
    if state.is_user_connected(user_id).await {
        info!("User {} has active connection, skipping cleanup (scheduled by old session)", user_id);
        return;
    }

    info!("Starting cleanup for user {} (no active connections)", user_id);
    state.cleanup_empty_channels(user_id).await;
}

/// Gestisce notifiche utente e setup subscription conversazioni
pub async fn handle_user_notification(
    state: &AppState,
    notification: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<Option<Uuid>> {
    // Prima controlla event_type (per user_events), poi type (per altri messaggi)
    let notification_type = notification
        .get("event_type")
        .and_then(|v| v.as_str())
        .or_else(|| notification.get("type").and_then(|v| v.as_str()))
        .unwrap_or("unknown");

    match notification_type {
        "conversation_created_complete" | "conversation_confirmation" | "new_conversation" => {
            let conversation_id = notification
                .get("conversation")
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| AppError::BadRequest("invalid conversation_id".into()))?;

            info!(
                "Processing {} for user {} - conversation {}",
                notification_type, user_id, conversation_id
            );

            // Setup subscription al broadcast channel
            if let Err(e) = setup_conversation_subscription(state, user_id, conversation_id).await {
                warn!(
                    "Failed to setup conversation subscription for user {}: {}",
                    user_id, e
                );
            }

            // Forward dell'evento al client
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx
                    .send(OutboundMsg::Text(notification_txt))
                    .await
                    .is_err()
                {
                    warn!("Failed to send {} to user {}", notification_type, user_id);
                } else {
                    info!("Forwarded {} to user {}", notification_type, user_id);
                }
            }

            // Ritorna il conversation_id per permettere setup aggiuntivo (es. stream manager)
            Ok(Some(conversation_id))
        }
        "conversation_deleted" => {
            // Forward dell'evento al client
            if let Ok(notification_txt) = serde_json::to_string(&notification) {
                if out_tx
                    .send(OutboundMsg::Text(notification_txt))
                    .await
                    .is_err()
                {
                    warn!("Failed to send {} to user {}", notification_type, user_id);
                } else {
                    debug!("Forwarded {} to user {}", notification_type, user_id);
                }
            }
            Ok(None)
        }
        _ => {
            // Forward altre notifiche
            if notification.get("sequence").is_some() {
                if let Ok(notification_txt) = serde_json::to_string(&notification) {
                    if out_tx
                        .send(OutboundMsg::Text(notification_txt))
                        .await
                        .is_err()
                    {
                        warn!("Failed to send sequenced event to user {}", user_id);
                    } else {
                        debug!("Forwarded sequenced event to user {}", user_id);
                    }
                }
            } else {
                debug!(
                    "Received notification type '{}' for user {}",
                    notification_type, user_id
                );
                if let Ok(txt) = serde_json::to_string(&notification) {
                    let _ = out_tx.send(OutboundMsg::Text(txt)).await;
                }
            }
            Ok(None)
        }
    }
}