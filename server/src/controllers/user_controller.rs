use crate::{error::Result, services::user_service::UserService, state::AppState};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct RegisterReq {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResp {
    pub token: String,
    pub user_id: i64,    // Aggiunto user_id
    pub username: String, // Aggiunto username per completezza
}

#[derive(Deserialize)]
pub struct LogoutReq {
    pub token: String,
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn register(
    State(st): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<i64>> {
    let id = UserService::register(&st.pool, &req.username, &req.password).await?;
    Ok(Json(id))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>> {
    let (token, user_id) = UserService::login(&st.pool, &st.jwt_secret, &req.username, &req.password).await?;
    Ok(Json(LoginResp {
        token,
        user_id,
        username: req.username,
    }))
}

#[cfg_attr(debug_assertions, axum::debug_handler)]
pub async fn logout(
    Json(req): Json<LogoutReq>,
) -> Result<()> {
    // Per JWT il logout è stateless - nessuna azione necessaria
    Ok(())
}