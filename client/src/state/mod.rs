// state/mod.rs
// Re-export pattern per mantenere compatibilità

// Modulo principale con definizione AppState
pub mod core;

// Moduli separati per responsabilità
pub mod commands;
pub mod ui;
pub mod helpers;


// Re-export pubblico - IL CODICE ESISTENTE NON SI ROMPE
pub use core::{
    AppState,
    PendingDeletion,
    ToastKind,
    Toast,
    STUB_TIMEOUT,
    USER_CHECK_TIMEOUT
};
