use crate::{auth::AuthUser, error::Result, services::message_service::MessageService, state::AppState};
use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct PostMessageReq {
    pub content: String,
}

#[derive(Serialize)]
pub struct MessageOut {
    pub id: i64,
    pub author_id: i64,
    pub content: String,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct CreatedId {
    pub id: i64
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn list(
    Path(conversation_id): Path<i64>,
    State(st): State<AppState>,
) -> Result<Json<Vec<MessageOut>>> {
    let rows = MessageService::list(&st.pool, conversation_id, 50).await?;
    let out = rows
        .into_iter()
        .map(|(id, author_id, content, created_at)| MessageOut { id, author_id, content, created_at })
        .collect();
    Ok(Json(out))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn post(
    user: AuthUser,
    Path(conversation_id): Path<i64>,
    State(st): State<AppState>,
    Json(req): Json<PostMessageReq>,
) -> Result<Json<CreatedId>> {
    // NB: MessageService::post ora accetta anche &AppState per fare il broadcast WS
    let message_id = MessageService::post(&st.pool, conversation_id, user.id, &req.content, &st).await?;
    Ok(Json(CreatedId { id: message_id }))
}
