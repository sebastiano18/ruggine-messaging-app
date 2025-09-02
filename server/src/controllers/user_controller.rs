use crate::{error::Result, services::user_service::UserService, state::AppState};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct RegisterReq {
    pub name: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginReq {
    pub name: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResp {
    pub token: String,
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn register(
    State(st): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<i64>> {
    // sqlx::Pool è Send/Sync; String è Send; niente tipi non-Send qui.
    let id = UserService::register(&st.pool, &req.name, &req.password).await?;
    Ok(Json(id))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>> {
    let token = UserService::login(&st.pool, &st.jwt_secret, &req.name, &req.password).await?;
    Ok(Json(LoginResp { token }))
}
