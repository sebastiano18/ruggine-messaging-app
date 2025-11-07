use crate::{
    auth::AuthUser,
    error::Result,
    services::conversation_service::ConversationService,
    state::AppState
};
use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};
use axum::http::StatusCode;
use uuid::Uuid;
use crate::services::user_service::UserService;
use serde_json::json;

#[derive(Deserialize)]
pub struct CreateGroupReq {
    pub name: String,
}

#[derive(Deserialize)]
pub struct CreateDmReq {
    pub user_username: String,
}

#[derive(Serialize)]
pub struct CreatedId {
    pub id: Uuid
}

#[derive(Deserialize)]
pub struct AddMemberReq {
    pub member_id: Uuid,
}

#[derive(Serialize)]
pub struct ConversationOut {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub owner_id: Uuid,
    pub created_at: i64,
    pub last_read_sequence: i64,  // AGGIUNTO per il client
    pub last_activity: i64,        // AGGIUNTO per il client
    pub last_msg_seq: i64,         // AGGIUNTO per il client
}

#[derive(Serialize)]
pub struct ConversationWithMessages {
    pub conversation: ConversationOut,
    pub messages: Vec<MessageOut>,
    pub members: Vec<ParticipantOut>,
}

#[derive(Serialize)]
pub struct MessageOut {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_username: String,
    pub conversation_id: Uuid,
    pub content: String,
    pub created_at: i64,
    pub sequence_num: Option<i64>, // AGGIUNTO
}

#[derive(Serialize)]
pub struct ParticipantOut {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub joined_at: i64,
}

// Crea un nuovo gruppo
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn create_group(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<CreateGroupReq>,
) -> Result<Json<CreatedId>> {
    let id = ConversationService::create_group(&st.pool, &req.name, user.id).await?;
    Ok(Json(CreatedId { id }))
}

// Crea o trova una DM
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn create_dm(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<CreateDmReq>,
) -> Result<Json<CreatedId>> {
    let user2_id = UserService::get_user_id_by_username(&st.pool, &req.user_username).await?;
    let id = ConversationService::create_dm(&st.pool, user.id, user2_id).await?;
    Ok(Json(CreatedId { id }))
}

// Ottieni le mie conversazioni
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn mine(
    user: AuthUser,
    State(st): State<AppState>,
) -> Result<Json<Vec<ConversationOut>>> {
    let rows = ConversationService::mine(&st.pool, user.id).await?;

    let conversations: Vec<ConversationOut> = rows.into_iter().map(|(id, kind, title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq)| {
        ConversationOut {
            id,
            kind,
            title,
            owner_id,
            created_at,
            last_read_sequence,  // Valore reale dal DB
            last_activity,       // Valore reale dal DB
            last_msg_seq,        // Valore reale dal DB
        }
    }).collect();

    // DEBUG: Logga il JSON che stiamo per inviare
    if let Ok(json_str) = serde_json::to_string_pretty(&conversations) {
        tracing::info!("Sending conversations JSON to client:\n{}", json_str);
    }

    Ok(Json(conversations))
}

// Ottieni singola conversazione
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_conversation(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<ConversationOut>> {
    let conversation_data = ConversationService::get_conversation(&st.pool, conversation_id, user.id).await?;

    match conversation_data {
        Some((id, kind, title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq)) => {
            Ok(Json(ConversationOut {
                id,
                kind,
                title,
                owner_id,
                created_at,
                last_read_sequence,  // Valore reale dal DB
                last_activity,       // Valore reale dal DB
                last_msg_seq,        // Valore reale dal DB
            }))
        }
        None => Err(crate::error::AppError::NotFound),
    }
}

// Aggiungi membro a un gruppo
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn add_member(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
    Json(req): Json<AddMemberReq>,
) -> Result<StatusCode> {
    ConversationService::add_member(&st.pool, conversation_id, req.member_id, user.id).await?;
    
    // Ottieni lista aggiornata dei membri
    let members_data = ConversationService::get_members(&st.pool, conversation_id, user.id).await?;
    let members: Vec<ParticipantOut> = members_data
        .into_iter()
        .map(|(user_id, username, role, joined_at)| ParticipantOut {
            user_id,
            username,
            role,
            joined_at,
        })
        .collect();
    
    // Ottieni lista partecipanti per inviare l'evento
    let participant_ids = ConversationService::list_participant_ids(&st.pool, conversation_id).await?;
    
    // Invia evento a tutti i partecipanti
    let payload = json!({
        "conversation_id": conversation_id,
        "members": members,
        "timestamp": chrono::Utc::now().timestamp()
    });
    
    for participant_id in participant_ids {
        if let Err(e) = st.send_sequenced_event_to_user(
            participant_id,
            "member_list_updated",
            payload.clone(),
            Some(conversation_id),
        ).await {
            tracing::error!("Failed to send member_list_updated event to {}: {}", participant_id, e);
        }
    }
    
    Ok(StatusCode::OK)
}

// Ottieni conversazione con messaggi
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_conversation_with_messages(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<ConversationWithMessages>> {
    // 1. Ottieni la conversazione (include controllo autorizzazione)
    let conversation_data = ConversationService::get_conversation(&st.pool, conversation_id, user.id).await?;

    let conversation = match conversation_data {
        Some((id, kind, title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq)) => ConversationOut {
            id,
            kind,
            title,
            owner_id,
            created_at,
            last_read_sequence,  // Valore reale dal DB
            last_activity,       // Valore reale dal DB
            last_msg_seq,        // Valore reale dal DB
        },
        None => return Err(crate::error::AppError::NotFound),
    };

    // 2. Usa il metodo 'list' aggiornato che ora ritorna anche sequence_num
    let messages_data = crate::services::message_service::MessageService::list(&st.pool, conversation_id, 50).await?;

    let messages: Vec<MessageOut> = messages_data
        .into_iter()
        .map(|(id, author_id, author_username, content, created_at, sequence_num)| MessageOut {
            id,
            author_id,
            author_username,
            conversation_id,
            content,
            created_at,
            sequence_num, // AGGIUNTO
        })
        .collect();

    // 3. Carica i membri della conversazione
    let members_data = ConversationService::get_members(&st.pool, conversation_id, user.id).await?;
    
    let members: Vec<ParticipantOut> = members_data
        .into_iter()
        .map(|(user_id, username, role, joined_at)| ParticipantOut {
            user_id,
            username,
            role,
            joined_at,
        })
        .collect();

    Ok(Json(ConversationWithMessages {
        conversation,
        messages,
        members,
    }))
}

// Ottieni membri di una conversazione
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_members(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<Vec<ParticipantOut>>> {
    let members = ConversationService::get_members(&st.pool, conversation_id, user.id).await?;
    
    let participants: Vec<ParticipantOut> = members
        .into_iter()
        .map(|(user_id, username, role, joined_at)| ParticipantOut {
            user_id,
            username,
            role,
            joined_at,
        })
        .collect();
    
    Ok(Json(participants))
}

// Espelli un membro da una conversazione
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn kick_member(
    user: AuthUser,
    State(st): State<AppState>,
    Path((conversation_id, user_id_to_kick)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    // Ottieni l'username dell'utente espulso prima di rimuoverlo
    let kicked_username = sqlx::query_scalar::<_, String>(
        "SELECT username FROM users WHERE id = ?"
    )
    .bind(user_id_to_kick.to_string())
    .fetch_one(&st.pool)
    .await?;
    
    ConversationService::kick_member(&st.pool, conversation_id, user.id, user_id_to_kick).await?;
    
    let timestamp = chrono::Utc::now().timestamp();
    
    // Invia evento sequenziato all'utente espulso per rimuovere la conversazione
    let kick_payload = json!({
        "conversation_id": conversation_id,
        "kicked_by": user.id,
        "timestamp": timestamp
    });
    
    if let Err(e) = st.send_sequenced_event_to_user(
        user_id_to_kick,
        "member_kicked",
        kick_payload,
        Some(conversation_id),
    ).await {
        tracing::error!("Failed to send member_kicked event: {}", e);
    }
    
    // Broadcast evento member_removed agli altri membri rimanenti (come member_added)
    let member_removed_payload = json!({
        "conversation_id": conversation_id,
        "user_id": user_id_to_kick,
        "username": kicked_username,
        "removed_by": user.id,
        "timestamp": timestamp
    });
    
    // Ottieni lista partecipanti rimanenti per il broadcast
    let participant_ids = ConversationService::list_participant_ids(&st.pool, conversation_id).await?;
    
    // Crea messaggio di sistema persistente per l'espulsione
    let system_message_content = format!("{} è stato espulso dal gruppo", kicked_username);
    match crate::web_socket::helpers::create_system_message(&st, conversation_id, system_message_content.clone()).await {
        Ok((msg_id, sequence)) => {
            tracing::info!("Created persistent kick system message (id={}, seq={})", msg_id, sequence);
            
            // Ottieni owner_id e username per il broadcast
            let owner_data: Option<(String, String)> = sqlx::query_as(
                "SELECT c.owner_id, u.username FROM conversations c 
                 JOIN users u ON c.owner_id = u.id 
                 WHERE c.id = ?"
            )
            .bind(conversation_id.to_string())
            .fetch_optional(&st.pool)
            .await
            .unwrap_or(None);
            
            if let Some((owner_id, owner_username)) = owner_data {
                // Broadcast il messaggio di sistema a tutti i partecipanti rimanenti
                let system_msg_broadcast = json!({
                    "type": "message",
                    "id": msg_id,
                    "conversation_id": conversation_id,
                    "author_id": owner_id,
                    "author_username": owner_username,
                    "content": system_message_content.clone(),
                    "created_at": timestamp,
                    "sequence_num": sequence
                });
                
                let _ = crate::web_socket::helpers::broadcast_to_conversation(&st, conversation_id, system_msg_broadcast.clone()).await;
                
                // Invia anche come evento sequenziato a tutti i partecipanti rimanenti
                for participant_id in &participant_ids {
                    if let Err(e) = st.send_sequenced_event_to_user(
                        *participant_id,
                        "new_message",
                        system_msg_broadcast.clone(),
                        Some(conversation_id),
                    ).await {
                        tracing::warn!("Failed to send kick system message event to {}: {}", participant_id, e);
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to create kick system message (non-fatal): {}", e);
        }
    }
    
    // Invia member_removed a tutti i partecipanti rimanenti
    for participant_id in &participant_ids {
        if let Err(e) = st.send_sequenced_event_to_user(
            *participant_id,
            "member_removed",
            member_removed_payload.clone(),
            Some(conversation_id),
        ).await {
            tracing::error!("Failed to send member_removed event to {}: {}", participant_id, e);
        }
    }
    
    // Ottieni lista aggiornata dei membri (dopo la rimozione)
    let members_data = ConversationService::get_members(&st.pool, conversation_id, user.id).await?;
    let members: Vec<ParticipantOut> = members_data
        .into_iter()
        .map(|(user_id, username, role, joined_at)| ParticipantOut {
            user_id,
            username,
            role,
            joined_at,
        })
        .collect();
    
    // Invia evento di aggiornamento lista a tutti i partecipanti rimanenti
    let update_payload = json!({
        "conversation_id": conversation_id,
        "members": members,
        "timestamp": timestamp
    });
    
    for participant_id in participant_ids {
        if let Err(e) = st.send_sequenced_event_to_user(
            participant_id,
            "member_list_updated",
            update_payload.clone(),
            Some(conversation_id),
        ).await {
            tracing::error!("Failed to send member_list_updated event to {}: {}", participant_id, e);
        }
    }
    
    Ok(StatusCode::NO_CONTENT)
}