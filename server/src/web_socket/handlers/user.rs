use chrono::Utc;
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    state::AppState,
    services::conversation_service::ConversationService,
    services::message_service::MessageService,
};
use crate::web_socket::actor::OutboundMsg;


/// Router principale per gestire i messaggi in arrivo

/// Handlers for user operations (check user, resume events)

pub async fn handle_check_user(
    state: &AppState,
    value: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    let target_username = value
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let request_id = value
        .get("request_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    info!("User {} checking if user '{}' exists", user_id, target_username);

    let user_result: Option<String> = sqlx::query_scalar(
        "SELECT id FROM users WHERE LOWER(username) = LOWER(?)"
    )
        .bind(target_username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let (exists, found_user_id) = match user_result {
        Some(uid) => (true, Some(uid)),
        None => (false, None),
    };

    let response = json!({
        "type": "check_user_response",
        "username": target_username,
        "exists": exists,
        "user_id": found_user_id,
        "request_id": request_id
    });

    if let Ok(txt) = serde_json::to_string(&response) {
        let _ = out_tx.send(OutboundMsg::Text(txt)).await;
    }

    info!(
        "User check result: username='{}', exists={}",
        target_username, exists
    );

    Ok(())
}

pub async fn handle_user_events_resume_request(
    state: &AppState,
    value: &Value,
    user_id: Uuid,
    out_tx: &mpsc::Sender<OutboundMsg>,
) -> Result<()> {
    debug!("User resume request from user {}", user_id);

    let from_sequence = value
        .get("from_sequence")
        .and_then(|s| s.as_u64())
        .unwrap_or(0);

    let limit = value
        .get("limit")
        .and_then(|s| s.as_i64())
        .unwrap_or(100)
        .min(1000);

    match state
        .get_user_events_since(user_id, from_sequence, limit)
        .await
    {
        Ok(events) => {
            if !events.is_empty() {
                info!(
                    "Sending {} user events in resume to user {}",
                    events.len(),
                    user_id
                );
                state
                    .send_user_events_resume(user_id, events, out_tx)
                    .await?;
            } else {
                let response = json!({
                    "type": "user_resume_complete",
                    "from_sequence": from_sequence,
                    "current_sequence": state.get_current_user_sequence(user_id).await.unwrap_or(0),
                    "events_count": 0,
                    "message": "No events to resume"
                });
                if let Ok(txt) = serde_json::to_string(&response) {
                    out_tx
                        .send(OutboundMsg::Text(txt))
                        .await
                        .map_err(|_| AppError::Internal("Failed to send response".into()))?;
                }
            }
            Ok(())
        }
        Err(e) => {
            error!("Failed to get user events for resume: {}", e);
            let error_response = json!({
                "type": "error",
                "message": "Failed to retrieve user events",
                "error_code": "RESUME_ERROR",
                "details": e.to_string()
            });
            if let Ok(txt) = serde_json::to_string(&error_response) {
                let _ = out_tx.send(OutboundMsg::Text(txt)).await;
            }
            Err(AppError::Internal(format!(
                "Failed to get user events: {}",
                e
            )))
        }
    }
}