use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
    services::conversation_service::ConversationService,
};
use crate::repositories::conversation_repo::ConversationRepo;
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

    // 1b. Cache client_temp_id → real UUID mapping (CRITICO per conferma client)
    if let Some(ref temp_id) = client_temp_id {
        state
            .conversation_confirmation_cache
            .insert(group_id, temp_id.clone())
            .await;
        info!("Cached client_temp_id {} → group_id {}", temp_id, group_id);
    }

    // 2. Raccogli tutti i participant IDs (creatore + invitati)
    let mut all_participant_ids = vec![creator_id];

    for username_value in participant_usernames {
        if let Some(username) = username_value.as_str() {
            // Cerca l'utente per username
            use crate::services::user_service::UserService;
            match UserService::get_user_id_by_username(&state.pool, username).await {
                Ok(user_id) => {
                    if user_id != creator_id {
                        all_participant_ids.push(user_id);
                    }
                }
                Err(_) => {
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

    // 4. Ottieni il timestamp di creazione
    let created_at: i64 = sqlx::query_scalar(
        "SELECT created_at FROM conversations WHERE id = ?"
    )
        .bind(group_id.to_string())
        .fetch_one(&state.pool)
        .await
        .map_err(crate::error::AppError::from)?;

    // 5. Invia eventi di creazione gruppo a tutti i partecipanti
    use crate::web_socket::broadcast::send_conversation_created_group_complete;
    send_conversation_created_group_complete(
        state,
        group_id,
        client_temp_id,
        creator_id,
        group_name.to_string(),
        created_at,
        all_participant_ids.clone(),
    )
        .await?;

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
        use crate::services::user_service::UserService;
        let target_user_id = match UserService::get_user_id_by_username(&state.pool, &target_username).await {
            Ok(user_id) => user_id,
            Err(_) => {
                warn!("User '{}' not found, skipping", target_username);
                skipped_count += 1;
                continue;
            }
        };

        let target_username_actual = target_username.clone();

        // Verifica che l'utente non sia già membro
        use crate::repositories::conversation_repo::ConversationRepo;
        let is_already_member = match ConversationRepo::is_participant(&state.pool, conversation_id, target_user_id).await {
            Ok(is_member) => is_member,
            Err(e) => {
                warn!("Error checking membership for '{}': {}, skipping", target_username_actual, e);
                skipped_count += 1;
                continue;
            }
        };

        if is_already_member {
            info!("User '{}' is already a member, skipping", target_username_actual);
            skipped_count += 1;
            continue;
        }

        // Aggiungi l'utente al gruppo
        match ConversationRepo::add_member(&state.pool, conversation_id, target_user_id).await {
            Ok(_) => {
                info!("User '{}' added to group {} by owner {}", target_username_actual, conversation_id, inviter_id);
            }
            Err(e) => {
                error!("Failed to add user '{}' to group: {}, skipping", target_username_actual, e);
                skipped_count += 1;
                continue;
            }
        }

        let ts = Utc::now().timestamp();

        // Invia evento new_conversation per notificare il nuovo membro
        let event_data = json!({
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
        });

        match state.send_sequenced_event_to_user(
            target_user_id,
            "new_conversation",
            event_data,
            Some(conversation_id)
        ).await {
            Ok(user_sequence) => {
                info!("Sent new_conversation event (seq={}) to user {}", user_sequence, target_user_id);
            }
            Err(e) => {
                warn!("Failed to send event to {}: {}", target_username_actual, e);
            }
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
        let all_participant_ids = crate::services::conversation_service::ConversationService::list_participant_ids(
            &state.pool,
            conversation_id
        ).await.unwrap_or_else(|e| {
            warn!("Failed to get participant ids for system message: {}", e);
            Vec::new()
        });

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
    let conversation_id = value
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest("Missing conversation_id".into()))?;

    // Ottieni username prima di rimuovere dal DB
    let username = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = ?")
        .bind(user_id.to_string())
        .fetch_one(&state.pool)
        .await?;

    // Rimuovi l'utente dal gruppo nel DB
    ConversationService::leave_group(&state.pool, conversation_id, user_id).await?;

    // Notifica gli altri partecipanti
    ConversationService::notify_user_left_group(state, conversation_id, user_id, &username).await?;

    // Invia ACK all'utente che è uscito
    let ack = json!({
        "type": "leave_group_ack",
        "conversation_id": conversation_id,
        "status": "ok"
    });
    if let Ok(txt) = serde_json::to_string(&ack) {
        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
    }

    Ok(())
}

pub async fn handle_remove_member(
    state: &AppState,
    value: &Value,
    requester_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    // 1. Estrai conversation_id
    let conversation_id = value
        .get("conversation_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest("Missing or invalid conversation_id".into()))?;

    // 2. Estrai user_id dell'utente da rimuovere
    let user_to_remove_id = value
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest("Missing or invalid user_id".into()))?;

    info!(
        "User {} requesting to remove user {} from conversation {}",
        requester_id, user_to_remove_id, conversation_id
    );

    // 3. Verifica che sia un gruppo
    let conversation_kind = ConversationRepo::get_conversation_kind(&state.pool, conversation_id)
        .await?
        .ok_or_else(|| AppError::NotFound)?;

    if conversation_kind != "group" {
        return Err(AppError::BadRequest("Can only remove members from groups".into()));
    }

    // 4. Verifica che il requester sia l'owner del gruppo
    let owner_id: String = sqlx::query_scalar(
        "SELECT owner_id FROM conversations WHERE id = ?"
    )
        .bind(conversation_id.to_string())
        .fetch_one(&state.pool)
        .await?;

    if owner_id != requester_id.to_string() {
        return Err(AppError::Forbidden);
    }

    // 5. Verifica che non stia cercando di rimuovere se stesso
    if requester_id == user_to_remove_id {
        return Err(AppError::BadRequest(
            "Cannot remove yourself. Use leave_group instead".into()
        ));
    }

    // 6. Verifica che l'utente da rimuovere sia effettivamente un membro
    let is_member = ConversationRepo::is_participant(&state.pool, conversation_id, user_to_remove_id).await?;
    if !is_member {
        return Err(AppError::BadRequest("User is not a member of this group".into()));
    }

    // 7. Ottieni username dell'utente da rimuovere prima di eliminarlo
    let removed_username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
        .bind(user_to_remove_id.to_string())
        .fetch_one(&state.pool)
        .await?;

    // 8. Rimuovi l'utente dal gruppo
    sqlx::query("DELETE FROM participants WHERE conversation_id = ? AND user_id = ?")
        .bind(conversation_id.to_string())
        .bind(user_to_remove_id.to_string())
        .execute(&state.pool)
        .await?;

    info!(
        "User {} removed from group {} by owner {}",
        removed_username, conversation_id, requester_id
    );

    // 9. Ottieni lista partecipanti DOPO la rimozione (per notificare chi è rimasto)
    let remaining_participants = ConversationService::list_participant_ids(&state.pool, conversation_id).await?;

    // 10. Notifica l'utente rimosso
    let removed_event = json!({
        "conversation_id": conversation_id.to_string(),
        "removed_user_id": user_to_remove_id.to_string(),
        "removed_username": removed_username.clone(),
        "removed_by": requester_id.to_string(),
    });

    if let Err(e) = state.send_sequenced_event_to_user(
        user_to_remove_id,
        "member_removed",
        removed_event,
        Some(conversation_id),
    ).await {
        warn!("Failed to notify removed user {}: {}", user_to_remove_id, e);
    }

    // 11. Notifica i membri rimanenti del gruppo
    let member_removed_event = json!({
        "conversation_id": conversation_id.to_string(),
        "removed_user_id": user_to_remove_id.to_string(),
        "removed_username": removed_username,
        "removed_by": requester_id.to_string(),
    });

    for participant_id in remaining_participants {
        if let Err(e) = state.send_sequenced_event_to_user(
            participant_id,
            "member_removed",
            member_removed_event.clone(),
            Some(conversation_id),
        ).await {
            warn!("Failed to notify participant {}: {}", participant_id, e);
        }
    }

    // 12. Invia ACK al requester
    let ack = json!({
        "type": "remove_member_ack",
        "conversation_id": conversation_id.to_string(),
        "removed_user_id": user_to_remove_id.to_string(),
        "status": "ok"
    });

    if let Ok(txt) = serde_json::to_string(&ack) {
        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
    }

    info!(
        "Successfully removed user {} from group {}",
        removed_username, conversation_id
    );

    Ok(())
}