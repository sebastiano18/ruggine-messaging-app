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
    pub created_at: String, // oppure DateTime<Utc> se già serializzi correttamente
}

#[derive(Serialize)]
pub struct CreatedId { pub id: i64 }

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn list(
    Path(cid): Path<i64>,
    State(st): State<AppState>,
) -> Result<Json<Vec<MessageOut>>> {
    // supponiamo che il service ritorni: Vec<(id, author_id, content, created_at)>
    let rows = MessageService::list(&st.pool, cid, 50).await?;
    let out = rows.into_iter().map(|(id, author_id, content, created_at)| {
        MessageOut {
            id,
            author_id,
            content,
            created_at: created_at.to_string(),
        }
    }).collect();
    Ok(Json(out))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn post(
    user: AuthUser,
    Path(cid): Path<i64>,
    State(st): State<AppState>,
    Json(req): Json<PostMessageReq>,
) -> Result<Json<CreatedId>> {
    let id = MessageService::post(&st.pool, cid, user.id, &req.content).await?;
    Ok(Json(CreatedId { id }))
}
