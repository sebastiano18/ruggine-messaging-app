use crate::{
    error::{AppError, Result},
    state::AppState,
};
use axum::{async_trait, extract::FromRequestParts, http::HeaderMap, http::request::Parts};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub name: String,
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    uid: i64,
    exp: usize,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let headers: &HeaderMap = &parts.headers;
        let auth = headers
            .get(axum::http::header::AUTHORIZATION)
            .ok_or(AppError::Unauthorized)?
            .to_str()
            .map_err(|_| AppError::Unauthorized)?;
        let token = auth.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;
        let app_state = parts
            .extensions
            .get::<AppState>()
            .cloned()
            .ok_or(AppError::Unauthorized)?;
        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(app_state.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|_| AppError::Unauthorized)?;
        Ok(AuthUser {
            id: data.claims.uid,
            name: data.claims.sub,
        })
    }
}
