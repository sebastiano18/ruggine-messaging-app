// In your mod.rs file

use crate::{
    error::{AppError, Result},
    state::AppState,
};
use axum::{
    async_trait,
    extract::{FromRequestParts, State}, // Import State extractor
    http::{HeaderMap, request::Parts},
};
use axum::extract::FromRef;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;

// Your structs remain the same
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
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
    AppState: FromRef<S>, // Correct trait bound for state extraction
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self> {
        // Use the State extractor to get AppState
        let State(app_state): State<AppState> = State::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let headers: &HeaderMap = &parts.headers;

        // The rest of your code for token extraction and decoding remains the same
        let auth = headers
            .get(axum::http::header::AUTHORIZATION)
            .ok_or(AppError::Unauthorized)?
            .to_str()
            .map_err(|_| AppError::Unauthorized)?
            .trim();

        let token = auth.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;

        let data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(app_state.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )
            .map_err(|_| AppError::Unauthorized)?;

        Ok(AuthUser {
            id: data.claims.uid,
            username: data.claims.sub,
        })
    }
}

// And remember to add `FromRef` implementation for your AppState