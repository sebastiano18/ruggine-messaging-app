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
use crate::web_socket::broadcast::broadcast_to_conversation;

/// Router principale per gestire i messaggi in arrivo

/// Handlers for group operations (create, invite, leave)

pub async fn handle_create_group_with_participants(
    state: &AppState,
    value: &mut serde_json::Value,
    creator_id: Uuid,
    creator_username: &str,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> crate::error::Result<()> {
    use crate::services::conversation_service::ConversationService;

    // Estrai parametri
    let group_name = value
        .get("group_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::error::AppError::BadRequest("Missing group_name".into()))?;

    let participant_usernames = value
        .get("participant_usernames")
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::error::AppError::BadRequest("Missing participant_usernames".into()))?;

    // Estrai client_temp_id se presente
    let client_temp_id = value
        .get("client_temp_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    info!(
        "Creating group '{}' by {} with {} participants (client_temp_id: {:?})",
        group_name,
        creator_username,
        participant_usernames.len(),
        client_temp_id
    );

    // 1. Crea il gruppo
    let group_id = ConversationService::create_group(&state.pool, group_name, creator_id).await?;
    info!("Group created with ID: {}", group_id);

    // 2. Raccogli tutti i participant IDs (creatore + invitati)
    let mut all_participant_ids = vec![creator_id];

    for username_value in participant_usernames {
        if let Some(username) = username_value.as_str() {
            // Cerca l'utente per username
            match sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&state.pool)
                .await?
            {
                Some(user_id_str) => {
                    match Uuid::parse_str(&user_id_str) {
                        Ok(user_id) => {
                            if user_id != creator_id {
                                all_participant_ids.push(user_id);
                            }
                        }
                        Err(e) => {
                            warn!("Invalid UUID for user {}: {}", username, e);
                        }
                    }
                }
                None => {
                    warn!("User {} not found, skipping", username);
                }
            }
        }
    }

    info!(
        "Adding {} participants to group {}",
        all_participant_ids.len(),
        group_id
    );

    // 3. Aggiungi tutti i partecipanti al gruppo (usa add_member con requester_id = creator)
    for participant_id in &all_participant_ids {
        if *participant_id != creator_id {
            ConversationService::add_member(&state.pool, group_id, *participant_id, creator_id).await?;
        }
    }

    // 4. Carica la conversazione completa per broadcast (usa get_conversation)
    let conversation_opt = ConversationService::get_conversation(&state.pool, group_id, creator_id).await?;

    let (id, kind, title, owner_id, created_at, last_read_seq, last_activity, last_msg_seq) = match conversation_opt {
        Some(data) => data,
        None => {
            return Err(crate::error::AppError::Internal("Failed to load created group".into()));
        }
    };

    let mut conversation = json!({
        "id": id,
        "kind": kind,
        "title": title,
        "owner_id": owner_id,
        "created_at": created_at,
        "last_read_sequence": last_read_seq,
        "last_activity": last_activity,
        "last_msg_seq": last_msg_seq
    });

    // Aggiungi client_temp_id se presente
    if let Some(ref temp_id) = client_temp_id {
        conversation["client_temp_id"] = json!(temp_id);
    }

    // 5. Crea eventi user_events con sequenze per TUTTI i partecipanti (incluso il creatore)
    for participant_id in &all_participant_ids {
        // Genera sequenza per questo utente
        let user_sequence = state.get_next_user_sequence(*participant_id).await?;

        // Salva evento nella tabella user_events
        sqlx::query(
            "INSERT INTO user_events (user_id, sequence_num, event_type, event_data, conversation_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
            .bind(participant_id.to_string())
            .bind(user_sequence as i64)
            .bind("new_conversation")
            .bind(conversation.to_string())
            .bind(group_id.to_string())
            .bind(created_at)
            .execute(&state.pool)
            .await
            .map_err(|e| crate::error::AppError::from(e))?;

        info!(
            "Created user_event for participant {} with sequence {}",
            participant_id, user_sequence
        );

        // Invia notifica in tempo reale se l'utente è connesso
        let notification = json!({
            "type": "user_notification",
            "sequence": user_sequence,
            "event_type": "new_conversation",
            "event_data": {
                "conversation": conversation.clone()
            },
            "conversation_id": group_id
        });

        if let Some(user_tx) = state.user_notification_channels.read().await.get(participant_id) {
            match user_tx.send(notification) {
                Ok(_) => info!("Sent user_notification to participant {}", participant_id),
                Err(e) => warn!("Failed to send notification to participant {}: {}", participant_id, e),
            }
        } else {
            info!("Participant {} not connected, will receive event on reconnect", participant_id);
        }
    }

    info!(
        "Group '{}' created successfully with {} participants",
        group_name,
        all_participant_ids.len()
    );

    Ok(())
}

pub async fn handle_invite_user(
    state: &AppState,
    value: &Value,
    inviter_id: Uuid,
) -> Result<()> {
    info!("Handling invite_user request");

    // Estrai conversation_id
    let conversation_id_str = value
        .get("cid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing conversation_id (cid)".into()))?;

    let conversation_id = Uuid::parse_str(conversation_id_str)
        .map_err(|_| AppError::BadRequest("Invalid conversation_id format".into()))?;

    // Supporta sia singolo username che array di usernames
    let target_usernames: Vec<String> = if let Some(username_str) = value.get("username").and_then(|v| v.as_str()) {
        // Singolo username
        vec![username_str.trim().to_string()]
    } else if let Some(usernames_array) = value.get("usernames").and_then(|v| v.as_array()) {
        // Array di usernames
        usernames_array
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        return Err(AppError::BadRequest("Missing 'username' or 'usernames' field".into()));
    };

    if target_usernames.is_empty() {
        return Err(AppError::BadRequest("No valid usernames provided".into()));
    }

    info!(
        "User {} inviting {} users to conversation {}",
        inviter_id, target_usernames.len(), conversation_id
    );

    // Verifica che la conversazione esista e sia un gruppo (una sola volta)
    let conv_row = sqlx::query(
        "SELECT kind, owner_id FROM conversations WHERE id = ?"
    )
        .bind(conversation_id.to_string())
        .fetch_optional(&state.pool)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound)?;

    let kind: String = conv_row.try_get("kind").map_err(AppError::from)?;
    let owner_id_str: String = conv_row.try_get("owner_id").map_err(AppError::from)?;
    let owner_id = Uuid::parse_str(&owner_id_str)
        .map_err(|_| AppError::Internal("Invalid owner_id in database".into()))?;

    // Verifica che sia un gruppo
    if kind != "group" {
        return Err(AppError::BadRequest("Can only invite users to groups".into()));
    }

    // Verifica che l'inviter sia l'owner
    if inviter_id != owner_id {
        return Err(AppError::Forbidden);
    }

    // Ottieni informazioni sulla conversazione (una sola volta)
    let conv_info = sqlx::query(
        "SELECT id, kind, title, owner_id, created_at FROM conversations WHERE id = ?"
    )
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)?;

    let conv_title: Option<String> = conv_info.try_get("title").ok();
    let conv_created_at: i64 = conv_info.try_get("created_at").map_err(AppError::from)?;

    // Statistiche per il riepilogo finale
    let mut added_count = 0;
    let mut skipped_count = 0;

    // Processa ogni username
    for target_username in target_usernames {
        info!("Processing invite for username: '{}'", target_username);

        // Trova l'utente da invitare
        let target_user_row = match sqlx::query("SELECT id, username FROM users WHERE username = ?")
            .bind(&target_username)
            .fetch_optional(&state.pool)
            .await
        {
            Ok(Some(row)) => row,
            Ok(None) => {
                warn!("User '{}' not found, skipping", target_username);
                skipped_count += 1;
                continue;
            }
            Err(e) => {
                warn!("Database error looking up user '{}': {}, skipping", target_username, e);
                skipped_count += 1;
                continue;
            }
        };

        let target_user_id_str: String = match target_user_row.try_get("id") {
            Ok(id) => id,
            Err(e) => {
                warn!("Error extracting user id for '{}': {}, skipping", target_username, e);
                skipped_count += 1;
                continue;
            }
        };

        let target_user_id = match Uuid::parse_str(&target_user_id_str) {
            Ok(id) => id,
            Err(e) => {
                warn!("Invalid UUID for user '{}': {}, skipping", target_username, e);
                skipped_count += 1;
                continue;
            }
        };

        let target_username_actual: String = match target_user_row.try_get("username") {
            Ok(name) => name,
            Err(_) => target_username.clone(),
        };

        // Verifica che l'utente non sia già membro
        let is_member: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM participants WHERE conversation_id = ? AND user_id = ?"
        )
            .bind(conversation_id.to_string())
            .bind(&target_user_id_str)
            .fetch_one(&state.pool)
            .await
        {
            Ok(count) => count,
            Err(e) => {
                warn!("Error checking membership for '{}': {}, skipping", target_username_actual, e);
                skipped_count += 1;
                continue;
            }
        };

        if is_member > 0 {
            info!("User '{}' is already a member, skipping", target_username_actual);
            skipped_count += 1;
            continue;
        }

        // Aggiungi l'utente al gruppo
        match sqlx::query(
            "INSERT INTO participants (conversation_id, user_id, role) VALUES (?, ?, ?)"
        )
            .bind(conversation_id.to_string())
            .bind(&target_user_id_str)
            .bind("member")
            .execute(&state.pool)
            .await
        {
            Ok(_) => {
                info!("User '{}' added to group {} by owner {}", target_username_actual, conversation_id, inviter_id);
            }
            Err(e) => {
                error!("Failed to add user '{}' to group: {}, skipping", target_username_actual, e);
                skipped_count += 1;
                continue;
            }
        }

        // Crea evento per notificare il nuovo membro
        let user_sequence = match state.get_next_user_sequence(target_user_id).await {
            Ok(seq) => seq,
            Err(e) => {
                warn!("Failed to get user sequence for '{}': {}", target_username_actual, e);
                0 // Fallback, ma l'utente è stato aggiunto
            }
        };

        let ts = Utc::now().timestamp();

        // Salva evento nella tabella user_events per il nuovo membro
        let _ = sqlx::query(
            "INSERT INTO user_events (user_id, sequence_num, event_type, event_data, conversation_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
            .bind(&target_user_id_str)
            .bind(user_sequence as i64)
            .bind("new_conversation")
            .bind(json!({
                "conversation": {
                    "id": conversation_id,
                    "kind": &kind,
                    "title": &conv_title,
                    "owner_id": owner_id,
                    "created_at": conv_created_at,
                    "last_read_sequence": 0,
                    "last_activity": ts,
                    "last_msg_seq": 0
                }
            }).to_string())
            .bind(conversation_id.to_string())
            .bind(ts)
            .execute(&state.pool)
            .await;

        // Notifica il nuovo membro via user notification channel
        let notification = json!({
            "type": "user_notification",
            "sequence": user_sequence,
            "event_type": "new_conversation",
            "event_data": {
                "conversation": {
                    "id": conversation_id,
                    "kind": &kind,
                    "title": &conv_title,
                    "owner_id": owner_id,
                    "created_at": conv_created_at,
                    "last_read_sequence": 0,
                    "last_activity": ts,
                    "last_msg_seq": 0
                }
            },
            "conversation_id": conversation_id
        });

        if let Some(user_tx) = state.user_notification_channels.read().await.get(&target_user_id) {
            match user_tx.send(notification.clone()) {
                Ok(_) => info!("Sent new_conversation notification to user {}", target_user_id),
                Err(e) => warn!("Failed to send notification to user {}: {}", target_user_id, e),
            }
        } else {
            info!("User {} has no notification channel (offline or not subscribed)", target_user_id);
        }

        // Broadcast a tutti i membri del gruppo (incluso il nuovo)
        let member_added_msg = json!({
            "type": "member_added",
            "conversation_id": conversation_id,
            "user_id": target_user_id,
            "username": target_username_actual,
            "added_by": inviter_id,
            "timestamp": ts
        });

        // Best effort broadcast - non è un errore fatale se fallisce
        match broadcast_to_conversation(state, conversation_id, member_added_msg).await {
            Ok(n) => info!("Broadcast member_added to {} subscribers", n),
            Err(e) => warn!("Failed to broadcast member_added (non-fatal): {}", e),
        }

        // Invia messaggio di sistema come evento sequenziato (NON salvato nel DB messages)
        // Ma salvato in user_events per persistenza
        let system_message_content = format!("{} è stato aggiunto al gruppo", target_username_actual);

        // Ottieni lista di tutti i partecipanti per inviare il messaggio di sistema
        let all_participant_ids = match crate::services::conversation_service::ConversationService::list_participant_ids(
            &state.pool,
            conversation_id
        ).await {
            Ok(ids) => ids,
            Err(e) => {
                warn!("Failed to get participant ids for system message: {}", e);
                Vec::new()
            }
        };

        // Crea il payload del messaggio di sistema
        let system_msg = json!({
            "type": "message",
            "id": Uuid::new_v4(),
            "conversation_id": conversation_id,
            "author_id": Uuid::nil(),
            "author_username": "system",
            "content": system_message_content,
            "created_at": ts,
            "sequence_num": null
        });

        // Invia come evento new_message a tutti (salvato in user_events, non in messages)
        for participant_id in all_participant_ids {
            if let Err(e) = state.send_sequenced_event_to_user(
                participant_id,
                "new_message",
                system_msg.clone(),
                Some(conversation_id),
            ).await {
                warn!("Failed to send system message to {}: {}", participant_id, e);
            }
        }

        // Invia evento sequenziato member_added a tutti i partecipanti esistenti (escluso il nuovo membro)
        let participant_ids = match crate::services::conversation_service::ConversationService::list_participant_ids(
            &state.pool,
            conversation_id
        ).await {
            Ok(ids) => ids,
            Err(e) => {
                warn!("Failed to get participant ids for member_added event: {}", e);
                Vec::new()
            }
        };

        let member_added_payload = json!({
            "username": target_username_actual,
            "user_id": target_user_id,
            "added_by": inviter_id,
            "timestamp": ts
        });

        for participant_id in participant_ids {
            // Non inviare l'evento al nuovo membro (riceve già new_conversation)
            if participant_id == target_user_id {
                continue;
            }

            if let Err(e) = state.send_sequenced_event_to_user(
                participant_id,
                "member_added",
                member_added_payload.clone(),
                Some(conversation_id),
            ).await {
                warn!("Failed to send member_added event to {}: {}", participant_id, e);
            }
        }

        // Invia member_list_updated DOPO ogni aggiunta (non solo alla fine)
        // Ottieni la lista aggiornata dei membri
        let members_data = match crate::services::conversation_service::ConversationService::get_members(
            &state.pool,
            conversation_id,
            inviter_id
        ).await {
            Ok(data) => data,
            Err(e) => {
                warn!("Failed to get updated members list after adding {}: {}", target_username_actual, e);
                Vec::new()
            }
        };

        if !members_data.is_empty() {
            let members: Vec<serde_json::Value> = members_data
                .into_iter()
                .map(|(user_id, username, role, joined_at)| json!({
                    "user_id": user_id,
                    "username": username,
                    "role": role,
                    "joined_at": joined_at
                }))
                .collect();

            // Ottieni lista partecipanti aggiornata per inviare l'evento
            let current_participant_ids = match crate::services::conversation_service::ConversationService::list_participant_ids(
                &state.pool,
                conversation_id
            ).await {
                Ok(ids) => ids,
                Err(e) => {
                    warn!("Failed to get participant ids after adding {}: {}", target_username_actual, e);
                    Vec::new()
                }
            };

            let member_list_payload = json!({
                "conversation_id": conversation_id,
                "members": members,
                "timestamp": chrono::Utc::now().timestamp()
            });

            for participant_id in current_participant_ids {
                if let Err(e) = state.send_sequenced_event_to_user(
                    participant_id,
                    "member_list_updated",
                    member_list_payload.clone(),
                    Some(conversation_id),
                ).await {
                    warn!("Failed to send member_list_updated event to {}: {}", participant_id, e);
                }
            }

            info!("Sent member_list_updated to all participants after adding {}", target_username_actual);
        }

        added_count += 1;
    }

    // Log riepilogo finale
    info!(
        "Invite operation completed: {} added, {} skipped for conversation {}",
        added_count, skipped_count, conversation_id
    );

    if added_count == 0 && skipped_count > 0 {
        return Err(AppError::BadRequest(format!(
            "Nessun utente è stato aggiunto. {} utente/i sono stati saltati.",
            skipped_count
        )));
    }

    Ok(())
}

pub async fn handle_leave_group(
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
            "op":"leave_group"
        });
        if let Ok(txt) = serde_json::to_string(&err) {
            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
        }
        return Err(AppError::BadRequest("Missing conversation_id".into()));
    }

    let conversation_id = cid_opt.unwrap();

    let participants_res = ConversationService::list_participant_ids(&state.pool, conversation_id).await;

    let username_res = sqlx::query_scalar::<_, String>(
        "SELECT username FROM users WHERE id = ?"
    )
        .bind(user_id.to_string())
        .fetch_one(&state.pool)
        .await;

    let leave_res = ConversationService::leave_group(
        &state.pool,
        conversation_id,
        user_id
    ).await;

    if let Err(e) = leave_res {
        let (code, message) = match &e {
            AppError::Unauthorized => ("FORBIDDEN", "Non sei autorizzato a eseguire questa azione".to_string()),
            AppError::NotFound => ("NOT_FOUND", "Conversazione non trovata".to_string()),
            AppError::BadRequest(msg) => ("BAD_REQUEST", msg.clone()),
            _ => ("LEAVE_FAILED", "Impossibile uscire dal gruppo. Riprova.".to_string()),
        };
        let err = json!({
            "type":"error",
            "error_code": code,
            "message": message,
            "op":"leave_group",
            "conversation_id": conversation_id
        });
        if let Ok(txt) = serde_json::to_string(&err) {
            let _ = out_tx.send(OutboundMsg::Text(txt)).await;
        }
        return Err(e);
    }

    if let (Ok(participants), Ok(username)) = (participants_res, username_res) {
        let remaining_participants: Vec<Uuid> = participants.into_iter()
            .filter(|&id| id != user_id)
            .collect();

        if !remaining_participants.is_empty() {
            let timestamp = chrono::Utc::now().timestamp();
            let system_message_content = format!("{} ha lasciato il gruppo", username);
            let system_msg = json!({
                "type": "message",
                "id": uuid::Uuid::new_v4(),
                "conversation_id": conversation_id,
                "author_id": uuid::Uuid::nil(),
                "author_username": "system",
                "content": system_message_content,
                "created_at": timestamp,
                "sequence_num": null
            });

            for participant_id in &remaining_participants {
                if let Err(e) = state.send_sequenced_event_to_user(
                    *participant_id,
                    "new_message",
                    system_msg.clone(),
                    Some(conversation_id),
                ).await {
                    warn!("Failed to send system message to {}: {}", participant_id, e);
                }
            }

            let members_data = match ConversationService::get_members(
                &state.pool,
                conversation_id,
                remaining_participants[0]
            ).await {
                Ok(data) => data,
                Err(e) => {
                    warn!("Failed to get updated members list after user left: {}", e);
                    Vec::new()
                }
            };

            let members: Vec<serde_json::Value> = members_data
                .into_iter()
                .map(|(user_id, username, role, joined_at)| json!({
                    "user_id": user_id,
                    "username": username,
                    "role": role,
                    "joined_at": joined_at
                }))
                .collect();

            let payload = json!({
                "conversation_id": conversation_id,
                "members": members,
                "timestamp": chrono::Utc::now().timestamp()
            });

            for participant_id in &remaining_participants {
                if let Err(e) = state.send_sequenced_event_to_user(
                    *participant_id,
                    "member_list_updated",
                    payload.clone(),
                    Some(conversation_id),
                ).await {
                    error!("Failed to send member_list_updated event to {}: {}", participant_id, e);
                }
            }

            info!("Sent member_list_updated event to {} participants", remaining_participants.len());
        } else {
            info!("No remaining participants to notify (group now empty)");
        }
    }

    let ack = json!({
        "type":"leave_group_ack",
        "conversation_id": conversation_id,
        "status":"ok"
    });
    if let Ok(txt) = serde_json::to_string(&ack) {
        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
    }

    Ok(())
}