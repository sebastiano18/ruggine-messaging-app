use crate::{
    auth::AuthUser, error::Result, services::message_service::MessageService, state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
#[derive(Deserialize)]
pub struct PostMessageReq {
    pub content: String,
}
pub async fn list(
    Path(cid): Path<i64>,
    State(st): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>> {
    let rows = MessageService::list(&st.pool, cid, 50).await?;
    Ok(Json(rows.into_iter().map(|(id, author_id, content, created_at)| serde_json::json!({"id": id, "author_id": author_id, "content": content, "created_at": created_at})).collect()))
}
pub async fn post(
    user: AuthUser,
    Path(cid): Path<i64>,
    State(st): State<AppState>,
    Json(req): Json<PostMessageReq>,
) -> Result<Json<serde_json::Value>> {
    let id = MessageService::post(&st.pool, cid, user.id, &req.content).await?;
    Ok(Json(serde_json::json!({"id": id})))
}
