use crate::{auth::AuthUser, error::Result, services::group_service::GroupService, state::AppState};
use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CreateGroupReq {
    pub name: String,
}

#[derive(Serialize)]
pub struct GroupOut {
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize)]
pub struct AddMemberReq {
    pub member_id: i64,
}

#[derive(Serialize)]
pub struct CreatedId { pub id: i64 }

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn create(
    user: AuthUser,
    State(st): State<AppState>,
    Json(req): Json<CreateGroupReq>,
) -> Result<Json<CreatedId>> {
    let id = GroupService::create(&st.pool, &req.name, user.id).await?;
    Ok(Json(CreatedId { id }))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn mine(
    user: AuthUser,
    State(st): State<AppState>,
) -> Result<Json<Vec<GroupOut>>> {
    let rows = GroupService::mine(&st.pool, user.id).await?;
    let groups = rows.into_iter().map(|(id, name)| GroupOut { id, name }).collect();
    Ok(Json(groups))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn add_member(
    _user: AuthUser, // se vuoi controlli di ownership, gestiscili nel service
    Path(group_id): Path<i64>,
    State(st): State<AppState>,
    Json(req): Json<AddMemberReq>,
) -> Result<Json<()>> {
    GroupService::add_member(&st.pool, group_id, req.member_id).await?;
    Ok(Json(()))
}
