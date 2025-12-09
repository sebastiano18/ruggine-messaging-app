// auth/mod.rs
use crate::{error::{AppError, Result}, state::AppState};
use axum::{async_trait, extract::{FromRequestParts, State}, http::{HeaderMap, request::Parts}};
use axum::extract::FromRef;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    uid: Uuid,
    exp: i64,
}
#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>, S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self> {
        let State(app_state): State<AppState> =
            State::from_request_parts(parts, state).await.map_err(|_| AppError::Unauthorized)?;

        let headers: &HeaderMap = &parts.headers;
        let auth = headers.get(axum::http::header::AUTHORIZATION)
            .ok_or(AppError::Unauthorized)?
            .to_str().map_err(|_| AppError::Unauthorized)?
            .trim();

        let token = auth.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(app_state.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        ).map_err(|_| AppError::Unauthorized)?;

        // Verifica che l'utente esista ancora
        let uid = data.claims.uid;
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM users WHERE id = ? LIMIT 1")
            .bind(uid.to_string())
            .fetch_optional(&app_state.pool)
            .await
            .map_err(|_| AppError::Unauthorized)?;
        if exists.is_none() {
            return Err(AppError::Unauthorized);
        }

        Ok(AuthUser { id: data.claims.uid, username: data.claims.sub })
    }
}


// And remember to add `FromRef` implementation for your AppState