use crate::{
    auth::AuthUser,
    error::Result,
    services::invite_service::{InviteService, InviteInfo},
    state::AppState
};
use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};
use axum::http::StatusCode;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateInviteReq {
    pub conversation_id: Uuid,
    pub expires_in_hours: Option<i32>, // Default 24 ore
}

#[derive(Serialize)]
pub struct InviteResponse {
    pub token: String,
}

#[derive(Deserialize)]
pub struct UseInviteReq {
    pub token: String,
}

#[derive(Serialize)]
pub struct ConversationIdResponse {
    pub id: Uuid,
}

#[derive(Serialize)]
pub struct InviteListResponse {
    pub invites: Vec<InviteInfo>,
}

/// Crea un nuovo invito per una conversazione
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn create_invite(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<CreateInviteReq>,
) -> Result<Json<InviteResponse>> {
    let token = InviteService::create_invite(
        &st.pool,
        req.conversation_id,
        user.id,
        req.expires_in_hours
    ).await?;

    Ok(Json(InviteResponse { token }))
}

/// Usa un invito per unirsi a una conversazione
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn use_invite(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<UseInviteReq>,
) -> Result<Json<ConversationIdResponse>> {
    let conversation_id = InviteService::use_invite(&st.pool, &req.token, user.id).await?;
    Ok(Json(ConversationIdResponse { id: conversation_id }))
}

/// Ottieni tutti gli inviti per una conversazione
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_invites(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<InviteListResponse>> {
    let invites = InviteService::get_invites_by_conversation(&st.pool, conversation_id, user.id).await?;
    Ok(Json(InviteListResponse { invites }))
}

/// Elimina un invito
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn delete_invite(
    user: AuthUser,
    State(st): State<AppState>,
    Path(token): Path<String>,
) -> Result<StatusCode> {
    InviteService::delete_invite(&st.pool, &token, user.id).await?;
    Ok(StatusCode::OK)
}