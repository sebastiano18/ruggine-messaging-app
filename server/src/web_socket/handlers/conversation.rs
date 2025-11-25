use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
    services::conversation_service::ConversationService,
    services::message_service::MessageService,
};
use crate::web_socket::actor::OutboundMsg;
use crate::web_socket::broadcast::{broadcast_to_conversation, send_conversation_created_events, send_message_confirmation};
use crate::web_socket::utils::{verify_conversation_exists, NewConversationData};


/// Router principale per gestire i messaggi in arrivo

/// Handlers for conversation operations (create, delete)

pub(super) async fn handle_message_with_new_conversation(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    info!("Creating new conversation with initial message");

    let content = value
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let client_msg_id = value
        .get("client_msg_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut create_request = json!({
        "type": "create_conversation",
        "target_username": value.get("target_username"),
        "conversation_type": "dm",
        "initial_message": content,
        "client_msg_id": client_msg_id
    });

    if let Some(temp_id) = value.get("client_temp_id") {
        create_request["client_temp_id"] = temp_id.clone();
    } else {
        if let Some(cid_str) = value
            .get("cid")
            .or_else(|| value.get("conversation_id"))
            .and_then(|v| v.as_str())
        {
            if let Ok(uuid) = Uuid::parse_str(cid_str) {
                if !verify_conversation_exists(state, uuid)
                    .await
                    .unwrap_or(false)
                {
                    create_request["client_temp_id"] = json!(cid_str);
                }
            } else {
                create_request["client_temp_id"] = json!(cid_str);
            }
        }
    }

    handle_create_conversation(state, &mut create_request, user_id, username).await
}

pub async fn handle_create_conversation(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    info!("HANDLING NEW CONVERSATION REQUEST: {}", value);

    let client_temp_id = value
        .get("client_temp_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let target_username = value
        .get("target_username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            AppError::BadRequest("Target username required for new conversation".into())
        })?
        .trim();

    let conversation_type = value
        .get("conversation_type")
        .and_then(|v| v.as_str())
        .unwrap_or("dm");

    let initial_message = value
        .get("initial_message")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty());

    let conversation_id = if let Some(ref temp_id) = client_temp_id {
        if let Some(existing_id) = state
            .conversation_confirmation_cache
            .get_by_temp_id(temp_id)
            .await
        {
            info!(
                "Conversation with temp_id {} already exists: {}",
                temp_id, existing_id
            );

            if let Some(content) = initial_message {
                let conversation_id_str = existing_id.to_string();
                let user_id_str = user_id.to_string();
                let message_sequence = state.get_next_message_sequence(existing_id).await?;
                let msg_id = Uuid::new_v4();
                let msg_id_str = msg_id.to_string();
                let ts = Utc::now().timestamp();

                sqlx::query(
                    "INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES (?, ?, ?, ?, ?, ?)"
                )
                    .bind(&msg_id_str)
                    .bind(&conversation_id_str)
                    .bind(&user_id_str)
                    .bind(&content)
                    .bind(ts)
                    .bind(message_sequence as i64)
                    .execute(&state.pool)
                    .await
                    .map_err(AppError::from)?;

                info!(
                    "Added message {} to existing conversation {}",
                    msg_id, existing_id
                );

                if let Some(client_msg_id) = value.get("client_msg_id").and_then(|v| v.as_str()) {
                    state
                        .message_confirmation_cache
                        .insert(msg_id, client_msg_id.to_string())
                        .await;
                }

                send_message_confirmation(
                    state,
                    msg_id,
                    existing_id,
                    message_sequence,
                    value
                        .get("client_msg_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    ts,
                    user_id,
                )
                    .await?;

                let broadcast_msg = json!({
                    "type": "chat_message",
                    "id": msg_id,
                    "conversation_id": existing_id,
                    "author_id": user_id,
                    "author_username": username,
                    "content": content,
                    "created_at": ts,
                    "sequence_num": message_sequence
                });

                broadcast_to_conversation(state, existing_id, broadcast_msg).await?;
            }

            return Ok(());
        } else {
            let new_id = Uuid::new_v4();
            state
                .conversation_confirmation_cache
                .insert(new_id, temp_id.clone())
                .await;
            info!(
                "Cached new conversation {} with client_temp_id {}",
                new_id, temp_id
            );
            new_id
        }
    } else {
        Uuid::new_v4()
    };

    let conversation_id_str = conversation_id.to_string();
    let user_id_str = user_id.to_string();

    let target_row = sqlx::query("SELECT id, username FROM users WHERE username = ?")
        .bind(target_username)
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound)?;

    let target_user_id_str: String = target_row.try_get("id").map_err(AppError::from)?;
    let target_user_id = Uuid::parse_str(&target_user_id_str).map_err(|_| {
        AppError::Internal("Invalid UUID format in database for target user".into())
    })?;
    let target_username_actual: String = target_row.try_get("username").map_err(AppError::from)?;

    if target_user_id == user_id {
        return Err(AppError::BadRequest(
            "Cannot create conversation with yourself".into(),
        ));
    }

    if conversation_type == "dm" {
        let existing_dm = sqlx::query(
            "SELECT c.id FROM conversations c
             JOIN participants p1 ON c.id = p1.conversation_id
             JOIN participants p2 ON c.id = p2.conversation_id
             WHERE c.kind = 'dm'
             AND p1.user_id = ? AND p2.user_id = ?
             LIMIT 1",
        )
            .bind(&user_id_str)
            .bind(&target_user_id_str)
            .fetch_optional(&state.pool)
            .await
            .map_err(AppError::from)?;

        if let Some(row) = existing_dm {
            let existing_id: String = row.try_get("id").map_err(AppError::from)?;
            return Err(AppError::BadRequest(format!(
                "DM conversation already exists: {}",
                existing_id
            )));
        }
    }

    let mut tx = state.pool.begin().await.map_err(AppError::from)?;

    let ts = Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO conversations (id, kind, title, owner_id, created_at) VALUES (?, ?, ?, ?, ?)"
    )
        .bind(&conversation_id_str)
        .bind(conversation_type)
        .bind(None::<String>)
        .bind(&user_id_str)
        .bind(ts)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from)?;

    for (participant_id_str, role) in &[(user_id_str.clone(), "member"), (target_user_id_str.clone(), "member")] {
        sqlx::query(
            "INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, ?)"
        )
            .bind(&conversation_id_str)
            .bind(participant_id_str)
            .bind(role)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;
    }

    let mut initial_msg_data: Option<(Uuid, String, u64, Option<String>)> = None;

    if let Some(content) = initial_message {
        let msg_id = Uuid::new_v4();
        let msg_id_str = msg_id.to_string();

        let sequence_result = sqlx::query(
            "INSERT INTO message_sequences (conversation_id, current_sequence, last_updated)
             VALUES (?, 1, ?)
             ON CONFLICT(conversation_id) DO UPDATE
             SET current_sequence = current_sequence + 1,
                 last_updated = ?
             RETURNING current_sequence"
        )
            .bind(&conversation_id_str)
            .bind(ts)
            .bind(ts)
            .fetch_one(&mut *tx)
            .await;

        let sequence = if sequence_result.is_err() {
            let _insert_result = sqlx::query(
                "INSERT OR IGNORE INTO message_sequences (conversation_id, current_sequence, last_updated)
                 VALUES (?, 0, ?)"
            )
                .bind(&conversation_id_str)
                .bind(ts)
                .execute(&mut *tx)
                .await;

            sqlx::query(
                "UPDATE message_sequences
                 SET current_sequence = current_sequence + 1,
                     last_updated = ?
                 WHERE conversation_id = ?"
            )
                .bind(ts)
                .bind(&conversation_id_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::from)?;

            let row = sqlx::query(
                "SELECT current_sequence FROM message_sequences WHERE conversation_id = ?"
            )
                .bind(&conversation_id_str)
                .fetch_one(&mut *tx)
                .await
                .map_err(AppError::from)?;

            row.try_get::<i64, _>("current_sequence").map_err(AppError::from)?
        } else {
            sequence_result.unwrap().try_get::<i64, _>("current_sequence").map_err(AppError::from)?
        };

        sqlx::query(
            "INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES (?, ?, ?, ?, ?, ?)"
        )
            .bind(&msg_id_str)
            .bind(&conversation_id_str)
            .bind(&user_id_str)
            .bind(&content)
            .bind(ts)
            .bind(sequence)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from)?;

        let client_msg_id = value
            .get("client_msg_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(ref client_id) = client_msg_id {
            state
                .message_confirmation_cache
                .insert(msg_id, client_id.clone())
                .await;
            info!(
                "Cached client_msg_id {} for initial message {}",
                client_id, msg_id
            );
        }

        initial_msg_data = Some((msg_id, content.to_string(), sequence as u64, client_msg_id));
    }

    tx.commit().await.map_err(AppError::from)?;

    info!(
        "Created conversation {} between {} and {}",
        conversation_id, username, target_username_actual
    );

    if let Some((msg_id, _, sequence, client_msg_id)) = &initial_msg_data {
        send_message_confirmation(
            state,
            *msg_id,
            conversation_id,
            *sequence,
            client_msg_id.clone(),
            ts,
            user_id,
        )
            .await?;
        info!("Sent message confirmation for initial message {}", msg_id);
    }

    let new_conv_data = NewConversationData {
        conversation_id,
        client_temp_id,
        creator_id: user_id,
        creator_username: username.to_string(),
        other_participant_id: target_user_id,
        other_participant_username: target_username_actual,
        initial_message_id: initial_msg_data
            .as_ref()
            .map(|(id, _, _, _)| *id)
            .unwrap_or(Uuid::nil()),
        initial_message_content: initial_msg_data
            .as_ref()
            .map(|(_, c, _, _)| c.clone())
            .unwrap_or_default(),
        initial_message_sequence: initial_msg_data
            .as_ref()
            .map(|(_, _, s, _)| *s)
            .unwrap_or(0),
        client_msg_id: initial_msg_data.as_ref().and_then(|(_, _, _, cid)| cid.clone()),
        created_at: ts,
    };

    send_conversation_created_events(state, new_conv_data).await?;

    if let Some((msg_id, content, sequence, client_msg_id)) = initial_msg_data {
        info!("Creating new_message events for initial message {} in conversation {}",
              msg_id, conversation_id);

        let participants = vec![user_id, target_user_id];

        let mut message_event_data = json!({
            "id": msg_id,
            "conversation_id": conversation_id,
            "author_id": user_id,
            "author_username": username,
            "content": content,
            "created_at": ts,
            "sequence_num": sequence,
        });

        if let Some(ref client_id) = client_msg_id {
            message_event_data["client_msg_id"] = json!(client_id);
        }

        for participant_id in participants {
            let event_payload = json!({
                "type": "new_message",
                "conversation_id": conversation_id,
                "conversation_sequence": sequence,
                "message": message_event_data.clone()
            });

            match state
                .send_sequenced_event_to_user(
                    participant_id,
                    "new_message",
                    event_payload,
                    Some(conversation_id),
                )
                .await
            {
                Ok(seq) => {
                    info!(
                        "Sent new_message event (seq={}) for initial message to user {}",
                        seq, participant_id
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to send new_message event to user {}: {}",
                        participant_id, e
                    );
                }
            }
        }

        info!("Initial message events created for all participants");
    }

    Ok(())
}

pub async fn handle_delete_conversation(
    state: &AppState,
    value: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    let cid_opt = value
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok());

    if cid_opt.is_none() {
        let err = json!({
            "type":"error",
            "error_code":"INVALID_REQUEST",
            "message":"Missing or invalid conversation_id",
            "op":"delete_conversation"
        });
        if let Ok(txt) = serde_json::to_string(&err) {
            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
        }
        return Err(AppError::BadRequest("Missing conversation_id".into()));
    }

    let conversation_id = cid_opt.unwrap();

    let participants_res = ConversationService::list_participant_ids(&state.pool, conversation_id).await;

    let delete_res = ConversationService::delete_conversation(
        &state.pool,
        conversation_id,
        user_id
    ).await;

    if let Err(e) = delete_res {
        let (code, message) = match &e {
            AppError::Unauthorized => {
                ("FORBIDDEN", "User not authorized".to_string())
            }
            AppError::NotFound => {
                ("NOT_FOUND", "Conversation not found".to_string())
            }
            _ => ("DELETE_FAILED", format!("Delete failed: {}", e)),
        };
        let err = json!({
            "type":"error",
            "error_code": code,
            "message": message,
            "op":"delete_conversation",
            "conversation_id": conversation_id
        });
        if let Ok(txt) = serde_json::to_string(&err) {
            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
        }
        return Err(e);
    }

    if let Ok(participants) = participants_res {
        ConversationService::broadcast_conversation_deleted(
            &state,
            conversation_id,
            user_id,
            participants.clone(),
            true,
        )
            .await;
    }

    let ack = json!({
        "type":"delete_conversation_ack",
        "conversation_id": conversation_id,
        "status":"ok"
    });
    if let Ok(txt) = serde_json::to_string(&ack) {
        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
    }

    Ok(())
}