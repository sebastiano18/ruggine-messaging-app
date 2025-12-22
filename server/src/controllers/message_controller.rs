use crate::models::Message;
use crate::{
    auth::AuthUser,
    error::Result,
    services::message_service::MessageService,
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::AppError;
use crate::repositories::conversation_repo::ConversationRepo;

#[derive(Deserialize)]
pub struct PostMessageReq {
    pub content: String,
}

#[derive(Serialize)]
pub struct CreatedId {
    pub id: Uuid,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub before_sequence: Option<i64>,
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn list(
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
    Query(params): Query<ListQuery>,
    State(st): State<AppState>,
) -> Result<Json<Vec<Message>>> {
    
    let is_participant = ConversationRepo::is_participant(
        &st.pool,
        conversation_id,
        user.id
    ).await?;

    if !is_participant {
        return Err(AppError::Forbidden);
    }
    
    let messages = if params.before_sequence.is_some() {
        MessageService::list_with_pagination(
            &st.pool,
            conversation_id,
            params.limit.unwrap_or(50),
            params.before_sequence,
        ).await?
    } else {
        let rows = MessageService::list(&st.pool, conversation_id, params.limit.unwrap_or(50)).await?;
        rows.into_iter()
            .map(|(id, author_id, author_username, content, created_at, sequence_num)| Message {
                id,
                author_id,
                conversation_id,
                author_username,
                content,
                created_at,
                sequence_num,
            })
            .rev()
            .collect()
    };

    Ok(Json(messages))
}
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn post(
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
    State(st): State<AppState>,
    Json(req): Json<PostMessageReq>,
) -> Result<Json<CreatedId>> {
    let message_id = MessageService::post(
        &st.pool,
        conversation_id,
        user.id,
        user.username,
        &req.content,
        &st,
    )
        .await?;
    Ok(Json(CreatedId { id: message_id }))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn delete_message(
    user: AuthUser,
    Path(message_id): Path<Uuid>,
    State(st): State<AppState>,
) -> Result<StatusCode> {
    MessageService::delete(&st.pool, message_id, user.id, &st).await?;
    Ok(StatusCode::NO_CONTENT)
}