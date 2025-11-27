// state/ui.rs - Gestione stato UI (toast, messaggi auth, popup)

use super::{AppState, Toast, ToastKind};
use crate::models::Page;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ========================================
// UI STATE MANAGEMENT
// ========================================

impl AppState {
    // === AUTH MESSAGE HANDLING ===

    pub fn set_auth_message(&mut self, msg: String, is_error: bool) {
        self.auth_message = Some(msg);
        self.auth_message_is_error = is_error;
    }

    pub fn clear_auth_message(&mut self) {
        self.auth_message = None;
        self.auth_message_is_error = false;
    }

    pub fn set_message_info(&mut self, msg: String) {
        if self.token.is_none() {
            self.set_auth_message(msg, false);
        } else {
            self.push_toast(ToastKind::Info, msg);
        }
    }

    pub fn set_message_error(&mut self, msg: String) {
        if self.token.is_none() {
            self.set_auth_message(msg, true);
        } else {
            self.push_toast(ToastKind::Error, msg);
        }
    }

    // === TOAST HELPERS ===

    pub fn push_toast(&mut self, kind: ToastKind, message: String) {
        if matches!(self.page, Page::Auth) || self.token.is_none() {
            return;
        }
        self.toasts.push(Toast {
            id: Uuid::new_v4(),
            message,
            kind,
            created: Instant::now(),
        });
        if self.toasts.len() > 5 {
            self.toasts.drain(0..self.toasts.len() - 5);
        }
    }

    pub fn prune_expired_toasts(&mut self, lifetime: Duration) {
        let now = Instant::now();
        self.toasts
            .retain(|t| now.duration_since(t.created) < lifetime);
    }
}
