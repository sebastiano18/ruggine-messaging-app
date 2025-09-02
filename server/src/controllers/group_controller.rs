use crate::{
    auth::AuthUser,
    error::Result,
    services::group_service::GroupService,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateGroupReq {
    pub name: String,
}

#[derive(Deserialize)]
pub struct AddMemberReq {
    pub user_id: i64,
}

#[derive(Serialize)]
pub struct GroupOut {
    pub id: i64,
    pub name: String,
}

#[derive(Serialize)]
pub struct OkResp {
    pub ok: bool,
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn create(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<CreateGroupReq>,
) -> Result<Json<GroupOut>> {
    // Preservo la tua logica di service
    let id = GroupService::create(&st.pool, &req.name, user.id).await?;
    Ok(Json(GroupOut { id, name: req.name }))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn add_member(
    State(st): State<AppState>,
    Path(gid): Path<i64>,
    Json(req): Json<AddMemberReq>,
) -> Result<Json<OkResp>> {
    GroupService::add_member(&st.pool, gid, req.user_id).await?;
    Ok(Json(OkResp { ok: true }))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn mine(
    user: AuthUser,
    State(st): State<AppState>,
) -> Result<Json<Vec<GroupOut>>> {
    let rows = GroupService::mine(&st.pool, user.id).await?;
    // rows: Vec<(id, name)>
    let groups = rows
        .into_iter()
        .map(|(id, name)| GroupOut { id, name })
        .collect();
    Ok(Json(groups))
}
