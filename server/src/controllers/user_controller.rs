use crate::{error::Result, services::user_service::UserService, state::AppState};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{debug, warn};
use crate::auth::AuthUser;


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

// Added last_sequence field
#[derive(Serialize)]
pub struct LoginResp {
    pub token: String,
    pub user_id: Uuid,
    pub username: String,
    pub last_sequence: u64,  // Include sequenza corrente dell'utente
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

    // Get user's current sequence
    let last_sequence = match st.get_current_user_sequence(user_id).await {
        Ok(seq) => {
            debug!("Retrieved sequence {} for user {}", seq, user_id);
            seq
        }
        Err(e) => {
            warn!("Failed to get sequence for user {}: {}, defaulting to 0", user_id, e);
            0
        }
    };

    let response = LoginResp {
        token,
        user_id,
        username: req.username,
        last_sequence,  // Include current sequence
    };

    tracing::info!("User {} logged in successfully with sequence {}", user_id, last_sequence);

    Ok(Json(response))
}

#[axum::debug_handler]
pub async fn logout(Json(_req): Json<LogoutReq>) -> Result<()> {
    Ok(()) // stateless
}
