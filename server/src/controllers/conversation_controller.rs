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
}

#[derive(Serialize)]
pub struct ConversationWithMessages {
    pub conversation: ConversationOut,
    pub messages: Vec<MessageOut>,
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

    let conversations = rows.into_iter().map(|(id, kind, title, owner_id, created_at)| {
        ConversationOut {
            id,
            kind,
            title,
            owner_id,
            created_at,
        }
    }).collect();

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
        Some((id, kind, title, owner_id, created_at)) => {
            Ok(Json(ConversationOut {
                id,
                kind,
                title,
                owner_id,
                created_at,
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
        Some((id, kind, title, owner_id, created_at)) => ConversationOut {
            id,
            kind,
            title,
            owner_id,
            created_at,
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

    Ok(Json(ConversationWithMessages {
        conversation,
        messages,
    }))
}