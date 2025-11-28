pub mod auth;
pub mod error;
pub mod models;
pub mod repositories;
pub mod services;
pub mod state;
pub mod web_socket;

// Re-export per comodità
pub use error::{AppError, Result};
pub use state::AppState;