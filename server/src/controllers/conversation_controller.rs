use crate::{
    auth::AuthUser,
    error::Result,
    services::conversation_service::ConversationService,
    state::AppState
};
use axum::{extract::{Path, State}, Json};
use axum::extract::Query;
use serde::{Deserialize, Serialize};
use axum::http::StatusCode;
use uuid::Uuid;
use crate::services::user_service::UserService;
use serde_json::json;
use crate::models::Message;

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
    pub last_read_sequence: i64,
    pub last_activity: i64,
    pub last_msg_seq: i64,
}

#[derive(Serialize)]
pub struct ConversationWithMessages {
    pub conversation: ConversationOut,
    pub messages: Vec<Message>,
    pub members: Vec<ParticipantOut>,
}

#[derive(Serialize)]
pub struct ConversationSummary {
    pub conversation: ConversationOut,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<ParticipantOut>>,
}

#[derive(Serialize)]
pub struct ParticipantOut {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub joined_at: i64,
}

#[derive(Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub before: Option<i64>,
}

fn default_limit() -> i32 {
    20
}

#[derive(Serialize)]
pub struct PaginatedConversationsResponse {
    pub conversations: Vec<ConversationSummary>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn create_group(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<CreateGroupReq>,
) -> Result<Json<CreatedId>> {
    let id = ConversationService::create_group(&st.pool, &req.name, user.id).await?;
    Ok(Json(CreatedId { id }))
}

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

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn mine(
    user: AuthUser,
    State(st): State<AppState>,
) -> Result<Json<Vec<ConversationOut>>> {
    let rows = ConversationService::mine(&st.pool, user.id).await?;

    let conversations: Vec<ConversationOut> = rows
        .into_iter()
        .map(|(id, kind, title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq)| {
            ConversationOut {
                id,
                kind,
                title,
                owner_id,
                created_at,
                last_read_sequence,
                last_activity,
                last_msg_seq,
            }
        })
        .collect();

    Ok(Json(conversations))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn add_member(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
    Json(req): Json<AddMemberReq>,
) -> Result<StatusCode> {
    ConversationService::add_member(&st.pool, conversation_id, req.member_id, user.id).await?;

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

    let participant_ids = ConversationService::list_participant_ids(&st.pool, conversation_id).await?;

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

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_conversation_with_messages(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<ConversationWithMessages>> {
    let result = ConversationService::get_conversation(&st.pool, conversation_id, user.id).await?;

    let (conversation_data, _last_message) = match result {
        Some(data) => data,
        None => return Err(crate::error::AppError::NotFound),
    };

    let (id, kind, title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq) = conversation_data;

    let conversation = ConversationOut {
        id,
        kind,
        title,
        owner_id,
        created_at,
        last_read_sequence,
        last_activity,
        last_msg_seq,
    };

    let messages_data = crate::services::message_service::MessageService::list(&st.pool, conversation_id, 50).await?;

    let messages: Vec<Message> = messages_data
        .into_iter()
        .map(|(id, author_id, author_username, content, created_at, sequence_num)| Message{
            id,
            author_id,
            author_username,
            conversation_id,
            content,
            created_at,
            sequence_num,
        })
        .collect();

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

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn kick_member(
    user: AuthUser,
    State(st): State<AppState>,
    Path((conversation_id, user_id_to_kick)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let kicked_username = sqlx::query_scalar::<_, String>(
        "SELECT username FROM users WHERE id = ?"
    )
        .bind(user_id_to_kick.to_string())
        .fetch_one(&st.pool)
        .await?;

    ConversationService::kick_member(&st.pool, conversation_id, user.id, user_id_to_kick).await?;

    let timestamp = chrono::Utc::now().timestamp();

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

    let member_removed_payload = json!({
        "conversation_id": conversation_id,
        "user_id": user_id_to_kick,
        "username": kicked_username,
        "removed_by": user.id,
        "timestamp": timestamp
    });

    let participant_ids = ConversationService::list_participant_ids(&st.pool, conversation_id).await?;

    let system_message_content = format!("{} è stato espulso dal gruppo", kicked_username);
    let system_msg = serde_json::json!({
        "type": "message",
        "id": uuid::Uuid::new_v4(),
        "conversation_id": conversation_id,
        "author_id": uuid::Uuid::nil(),
        "author_username": "system",
        "content": system_message_content,
        "created_at": timestamp,
        "sequence_num": null
    });

    for participant_id in &participant_ids {
        if let Err(e) = st.send_sequenced_event_to_user(
            *participant_id,
            "new_message",
            system_msg.clone(),
            Some(conversation_id),
        ).await {
            tracing::warn!("Failed to send system message to {}: {}", participant_id, e);
        }
    }

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

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_conversations(
    user: AuthUser,
    State(st): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedConversationsResponse>> {
    let results = ConversationService::get_conversations(
        &st.pool,
        user.id,
        params.limit,
        params.before
    ).await?;

    let conversations: Vec<ConversationSummary> = results
        .into_iter()
        .map(|((id, kind, title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq), last_message)| {
            ConversationSummary {
                conversation: ConversationOut {
                    id,
                    kind,
                    title,
                    owner_id,
                    created_at,
                    last_read_sequence,
                    last_activity,
                    last_msg_seq,
                },
                last_message,
                members: None,
            }
        })
        .collect();

    let next_cursor = conversations.last().map(|c| c.conversation.last_activity);
    let has_more = conversations.len() == params.limit as usize;

    tracing::info!(
        "User {} requested paginated conversations: limit={}, before={:?}, returned={}, has_more={}",
        user.id, params.limit, params.before, conversations.len(), has_more
    );

    Ok(Json(PaginatedConversationsResponse {
        conversations,
        next_cursor,
        has_more,
    }))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_conversation(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<ConversationSummary>> {
    let result = ConversationService::get_conversation(&st.pool, conversation_id, user.id).await?;

    let (conversation_data, last_message) = match result {
        Some(data) => data,
        None => return Err(crate::error::AppError::NotFound),
    };

    let (id, kind, title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq) = conversation_data;

    let conversation = ConversationOut {
        id,
        kind: kind.clone(),
        title,
        owner_id,
        created_at,
        last_read_sequence,
        last_activity,
        last_msg_seq,
    };

    let members = if kind == "group" {
        let members_data = ConversationService::get_members(&st.pool, conversation_id, user.id).await?;
        Some(
            members_data
                .into_iter()
                .map(|(user_id, username, role, joined_at)| ParticipantOut {
                    user_id,
                    username,
                    role,
                    joined_at,
                })
                .collect()
        )
    } else {
        None
    };

    Ok(Json(ConversationSummary {
        conversation,
        last_message,
        members,
    }))
}