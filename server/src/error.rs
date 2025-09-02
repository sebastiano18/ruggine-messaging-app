use axum::{http::StatusCode, response::{IntoResponse, Response}};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("unauthorized")] Unauthorized,
    #[error("forbidden")]   Forbidden,
    #[error("not found")]   NotFound,
    #[error("bad request: {0}")] BadRequest(String),
    #[error(transparent)] Anyhow(#[from] anyhow::Error),
}

// ⬇️ Aggiungi questa conversione: sqlx::Error -> AppError
impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Anyhow(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::Unauthorized   => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            AppError::Forbidden      => (StatusCode::FORBIDDEN,    "forbidden".to_string()),
            AppError::NotFound       => (StatusCode::NOT_FOUND,     "not found".to_string()),
            AppError::BadRequest(m)  => (StatusCode::BAD_REQUEST,   m),
            AppError::Anyhow(e)      => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        (status, axum::Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
