//! Handler dispatcher and re-exports

use uuid::Uuid;
use serde_json::Value;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

// Re-export all handler modules
pub mod conversation;
pub mod message;
pub mod group;
pub mod user;

// Re-export main handlers for convenience
pub use conversation::{handle_create_conversation, handle_delete_conversation};
pub use message::{handle_chat_message, handle_mark_read, handle_delete_message};
pub use group::{handle_create_group_with_participants, handle_invite_user, handle_leave_group};
pub use user::{handle_check_user, handle_user_events_resume_request};

pub async fn handle_incoming_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    let message_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("chat_message");

    match message_type {
        "create_conversation" => handle_create_conversation(state, value, user_id, username).await,
        "chat_message" | "message" => {
            handle_chat_message(state, value, user_id, username).await
        }
        "invite_user" => handle_invite_user(state, value, user_id).await,
        _ => Err(AppError::BadRequest(format!(
            "Unknown message type: {}",
            message_type
        ))),
    }
}