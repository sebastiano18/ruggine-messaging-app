use crate::models::Message;
use crate::{
    auth::AuthUser,
    error::Result,
    services::message_service::MessageService,
    state::AppState,
    web_socket::helpers::{get_conversation_messages_api, MessageResponse},
};
use axum::{
    Json,
    extract::{Path, State, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::error::AppError;

#[derive(Deserialize)]
pub struct PostMessageReq {
    pub content: String,
}

#[derive(Serialize)]
pub struct CreatedId {
    pub id: Uuid,
}

#[derive(Deserialize)]
pub struct MessageQuery {
    pub limit: Option<i64>,
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn list(
    Path(conversation_id): Path<Uuid>,
    State(st): State<AppState>,
) -> Result<Json<Vec<Message>>> {
    // Ora list ritorna 6 elementi, incluso sequence_num
    let rows = MessageService::list(&st.pool, conversation_id, 50).await?;
    let out = rows
        .into_iter()
        .map(
            |(id, author_id, author_username, content, created_at, sequence_num)| Message {
                id,
                author_id,
                conversation_id,
                author_username,
                content,
                created_at,
                sequence_num, // AGGIUNTO
            },
        )
        .rev()
        .collect();
    Ok(Json(out))
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

/// Endpoint per fetch messaggi (usato dal sistema fetch-on-subscribe)
#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn fetch_messages(
    user: AuthUser,
    Path(conversation_id): Path<Uuid>,
    Query(params): Query<MessageQuery>,
    State(st): State<AppState>,
) -> Result<Json<Vec<MessageResponse>>> {
    let messages = get_conversation_messages_api(&st, conversation_id, user.id, params.limit).await?;
    Ok(Json(messages))
}