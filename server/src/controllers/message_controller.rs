use crate::models::Message;
use crate::{
    auth::AuthUser, error::Result, services::message_service::MessageService, state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct PostMessageReq {
    pub content: String,
}

#[derive(Serialize)]
pub struct CreatedId {
    pub id: Uuid,
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn list(
    Path(conversation_id): Path<Uuid>,
    State(st): State<AppState>,
) -> Result<Json<Vec<Message>>> {
    let rows = MessageService::list(&st.pool, conversation_id, 50).await?;
    let out = rows
        .into_iter()
        .map(
            |(id, author_id, author_username, content, created_at)| Message {
                id,
                author_id,
                conversation_id,
                author_username,
                content,
                created_at,
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
