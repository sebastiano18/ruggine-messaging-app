// controllers/users_controller.rs
use crate::{error::Result, services::user_service::UserService, state::AppState};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub user_id: Uuid, // <- UUID nativo (in JSON sarà una stringa)
    pub username: String,
}

#[derive(Serialize)]
pub struct CreatedId {
    pub id: Uuid,
}

#[derive(Deserialize)]
pub struct LogoutReq {
    pub token: String,
}

#[axum::debug_handler]
pub async fn register(
    State(st): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<CreatedId>> {
    let id = UserService::register(&st.pool, &req.username, &req.password).await?;
    Ok(Json(CreatedId { id }))
}

#[axum::debug_handler]
pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>> {
    let (token, user_id) =
        UserService::login(&st.pool, &st.jwt_secret, &req.username, &req.password).await?;
    Ok(Json(LoginResp {
        token,
        user_id,
        username: req.username,
    }))
}

#[axum::debug_handler]
pub async fn logout(Json(_req): Json<LogoutReq>) -> Result<()> {
    Ok(()) // stateless
}
