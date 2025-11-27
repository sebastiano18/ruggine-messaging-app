pub(crate) mod events;
pub mod ws_manager;
mod app;

// Re-export App al livello corretto
pub use app::App;
