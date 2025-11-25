//! Message routing and dispatching

use uuid::Uuid;
use serde_json::Value;

use crate::{
    error::{AppError, Result},
    state::AppState,
};

use super::{
    conversation::handle_create_conversation,
    message::handle_chat_message,
    group::handle_invite_user,
};

/// Router principale per gestire i messaggi in arrivo
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