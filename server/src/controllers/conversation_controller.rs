use crate::{
    auth::AuthUser,
    error::Result,
    services::conversation_service::ConversationService,
    state::AppState
};
use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};
use axum::http::StatusCode;

#[derive(Deserialize)]
pub struct CreateGroupReq {
    pub name: String,
}

#[derive(Deserialize)]
pub struct CreateDmReq {
    pub user_id: i64,
}

#[derive(Serialize)]
pub struct CreatedId { pub id: i64 }

#[derive(Deserialize)]
pub struct AddMemberReq {
    pub member_id: i64,
}

#[derive(Serialize)]
pub struct ConversationOut {
    pub id: i64,
    pub kind: String,
    pub title: String,
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
    let id = ConversationService::create_dm(&st.pool, user.id, req.user_id).await?;
    Ok(Json(CreatedId { id }))
}

// Ottieni le mie conversazioni
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn mine(
    user: AuthUser,
    State(st): State<AppState>,
) -> Result<Json<Vec<ConversationOut>>> {
    let rows = ConversationService::mine(&st.pool, user.id).await?;
    let conversations = rows.into_iter().map(|(id, kind, title)| ConversationOut {
        id,
        kind,
        title
    }).collect();
    Ok(Json(conversations))
}

// Aggiungi membro a un gruppo
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn add_member(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<i64>,
    Json(req): Json<AddMemberReq>,
) -> Result<StatusCode> {
    ConversationService::add_member(&st.pool, conversation_id, req.member_id, user.id).await?;
    Ok(StatusCode::OK)
}