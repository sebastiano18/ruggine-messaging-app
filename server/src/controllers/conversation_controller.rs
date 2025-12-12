use crate::{
    auth::AuthUser,
    error::Result,
    services::conversation_service::ConversationService,
    state::AppState
};
use axum::{extract::{Path, State}, Json};
use axum::extract::Query;
use serde::{Deserialize,};
use uuid::Uuid;

use crate::models::{
    ConversationOut,
    ParticipantOut,
    ConversationSummary,
    PaginatedConversationsResponse
};

#[derive(Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub before: Option<i64>,
}

fn default_limit() -> i32 {
    20
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_conversations(
    user: AuthUser,
    State(st): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedConversationsResponse>> {
    let response = ConversationService::get_conversations(
        &st.pool,
        user.id,
        params.limit,
        params.before
    ).await?;

    tracing::info!(
        "User {} requested paginated conversations: limit={}, before={:?}, returned={}, has_more={}",
        user.id, params.limit, params.before, response.conversations.len(), response.has_more
    );

    Ok(Json(response))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn get_conversation(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<ConversationSummary>> {
    let summary = ConversationService::get_conversation(&st.pool, conversation_id, user.id)
        .await?
        .ok_or(crate::error::AppError::NotFound)?;

    Ok(Json(summary))
}