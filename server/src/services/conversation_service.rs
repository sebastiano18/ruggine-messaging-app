use serde_json::json;
use tracing::{info, warn};
use uuid::Uuid;
use crate::{error::Result, repositories::conversation_repo::ConversationRepo, state::AppState};
use crate::models::{ConversationOut, ConversationSummary, Message, PaginatedConversationsResponse, ParticipantOut};
use crate::web_socket::utils::send_event_to_multiple_users;

#[derive(Debug, Clone)]
pub struct ConversationService;

impl ConversationService {
    pub async fn create_group(pool: &sqlx::SqlitePool, name: &str, owner_id: Uuid) -> Result<Uuid> {
        ConversationRepo::create_group(pool, name, owner_id).await
    }
    
    pub async fn get_conversation(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        user_id: Uuid
    ) -> Result<Option<ConversationSummary>> {
        let conv_opt = ConversationRepo::get_single_conversation(pool, conversation_id, user_id).await?;

        Ok(conv_opt.map(|conv| {
            let last_message = if let (Some(msg_id), Some(author_id), Some(author_username), Some(content), Some(timestamp)) =
                (conv.last_msg_id, conv.last_msg_author_id, conv.last_msg_author_username,
                 conv.last_msg_content, conv.last_msg_timestamp)
            {
                Some(Message {
                    id: msg_id,
                    author_id,
                    conversation_id: conv.id,
                    author_username,
                    content,
                    created_at: timestamp,
                    sequence_num: conv.last_msg_sequence,
                })
            } else {
                None
            };

            let members = conv.members.as_ref().map(|m| {
                m.iter()
                    .map(|p| ParticipantOut {
                        user_id: p.user_id,
                        username: p.username.clone(),
                        role: p.role.clone(),
                    })
                    .collect()
            });

            ConversationSummary {
                conversation: ConversationOut {
                    id: conv.id,
                    kind: conv.kind.clone(),
                    title: conv.title.clone(),
                    owner_id: conv.owner_id,
                    created_at: conv.created_at,
                    last_read_sequence: conv.last_read_sequence,
                    last_activity: conv.last_activity,
                    last_msg_seq: conv.last_msg_seq,
                },
                last_message,
                members,
            }
        }))
    }

    pub async fn add_member(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        member_id: Uuid,
        requester_id: Uuid
    ) -> Result<()> {
        if !ConversationRepo::is_owner(pool, conversation_id, requester_id).await? {
            return Err(crate::error::AppError::Unauthorized);
        }

        ConversationRepo::add_member(pool, conversation_id, member_id).await
    }
    

    pub async fn delete_conversation(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
    ) -> Result<()> {
        let kind_opt = ConversationRepo::get_conversation_kind(pool, conversation_id).await?;
        let kind = match kind_opt {
            Some(k) => k,
            None => return Err(crate::error::AppError::NotFound),
        };

        match kind.as_str() {
            "group" => {
                let is_owner = ConversationRepo::is_owner(pool, conversation_id, requester_id).await?;
                if !is_owner {
                    return Err(crate::error::AppError::Unauthorized);
                }
            }
            "dm" => {
                let allowed = ConversationRepo::user_has_dm_access(pool, conversation_id, requester_id).await?;
                if !allowed {
                    return Err(crate::error::AppError::Unauthorized);
                }
            }
            _ => {
                return Err(crate::error::AppError::Unauthorized);
            }
        }

        ConversationRepo::delete_conversation(pool, conversation_id).await?;
        Ok(())
    }

    pub async fn leave_group(
        pool: &sqlx::SqlitePool,
        conversation_id: Uuid,
        requester_id: Uuid,
    ) -> Result<()> {
        let kind_opt = ConversationRepo::get_conversation_kind(pool, conversation_id).await?;
        let kind = match kind_opt {
            Some(k) => k,
            None => return Err(crate::error::AppError::NotFound),
        };

        if kind != "group" {
            return Err(crate::error::AppError::BadRequest("Non è un gruppo".to_string()));
        }

        let is_owner = ConversationRepo::is_owner(pool, conversation_id, requester_id).await?;
        if is_owner {
            return Err(crate::error::AppError::BadRequest("L'owner non può uscire dal gruppo, deve eliminarlo".to_string()));
        }

        let is_participant = ConversationRepo::is_participant(pool, conversation_id, requester_id).await?;
        if !is_participant {
            return Err(crate::error::AppError::Unauthorized);
        }

        ConversationRepo::remove_member(pool, conversation_id, requester_id).await?;
        Ok(())
    }

    pub async fn list_participant_ids(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        conversation_id: Uuid,
    ) -> Result<Vec<Uuid>> {
        ConversationRepo::list_participant_ids(pool, conversation_id).await
    }
    

    pub async fn broadcast_message_deleted(
        st: &AppState,
        conversation_id: Uuid,
        message_id: Uuid,
        participant_ids: Vec<Uuid>,
    ) {
        info!(
            "Broadcasting and persisting message_deleted event for message {} in conversation {}",
            message_id, conversation_id
        );

        let event_data = json!({
            "message_id": message_id,
            "conversation_id": conversation_id,
        });

        if let Err(e) = send_event_to_multiple_users(
            st,
            &participant_ids,
            "message_deleted",
            &event_data,
            Some(conversation_id),
        ).await {
            warn!("Failed to broadcast message_deleted: {}", e);
        }
    }

    pub async fn broadcast_conversation_deleted(
        st: &AppState,
        conversation_id: Uuid,
        by: Uuid,
        participant_ids: Vec<Uuid>,
        include_author: bool,
    ) {
        let recipients: Vec<Uuid> = if include_author {
            participant_ids
        } else {
            participant_ids.into_iter().filter(|&id| id != by).collect()
        };

        if recipients.is_empty() {
            return;
        }

        let payload = json!({
            "type": "conversation_deleted",
            "conversation_id": conversation_id,
            "by": by,
            "timestamp": chrono::Utc::now().timestamp()
        });

        if let Err(e) = send_event_to_multiple_users(
            st,
            &recipients,
            "conversation_deleted",
            &payload,
            Some(conversation_id),
        ).await {
            warn!(
                "Failed to broadcast conversation_deleted for conv {}: {}",
                conversation_id, e
            );
        }
    }
    

    pub async fn notify_user_left_group(
        state: &AppState,
        conversation_id: Uuid,
        user_id: Uuid,
        username: &str,
    ) -> Result<()> {
        let participants = ConversationRepo::list_participant_ids(&state.pool, conversation_id)
            .await?
            .into_iter()
            .filter(|&id| id != user_id)
            .collect::<Vec<_>>();

        if participants.is_empty() {
            info!("No remaining participants in group {}", conversation_id);
            return Ok(());
        }

        info!(
            "Notifying {} participants that user {} ({}) left conversation {}",
            participants.len(), user_id, username, conversation_id
        );

        let event = json!({
            "type": "user_left_group",
            "conversation_id": conversation_id,
            "user_id": user_id,
            "username": username,
            "timestamp": chrono::Utc::now().timestamp()
        });

        if let Err(e) = send_event_to_multiple_users(
            state,
            &participants,
            "user_left_group",
            &event,
            Some(conversation_id),
        ).await {
            warn!("Failed to notify user_left_group: {}", e);
        }

        Ok(())
    }

    pub async fn notify_user_deleted_account(
        state: &AppState,
        conversation_id: Uuid,
        deleted_user_id: Uuid,
        deleted_username: &str,
    ) -> Result<()> {
        let participants = ConversationRepo::list_participant_ids(&state.pool, conversation_id)
            .await?
            .into_iter()
            .filter(|&id| id != deleted_user_id)
            .collect::<Vec<_>>();

        if participants.is_empty() {
            info!("No remaining participants in group {}", conversation_id);
            return Ok(());
        }

        info!(
            "Notifying {} participants that user {} ({}) deleted their account in conversation {}",
            participants.len(), deleted_user_id, deleted_username, conversation_id
        );

        let event = json!({
            "type": "user_deleted_account",
            "conversation_id": conversation_id,
            "deleted_user_id": deleted_user_id,
            "deleted_username": deleted_username,
        });

        if let Err(e) = send_event_to_multiple_users(
            state,
            &participants,
            "user_deleted_account",
            &event,
            Some(conversation_id),
        ).await {
            warn!("Failed to notify user_deleted_account: {}", e);
        }

        Ok(())
    }

    pub async fn notify_conversation_deleted(
        state: &AppState,
        conversation_id: Uuid,
        deleted_user_id: Uuid,
        reason: &str,
    ) -> Result<()> {
        let participants = ConversationRepo::list_participant_ids(&state.pool, conversation_id)
            .await?
            .into_iter()
            .filter(|&id| id != deleted_user_id)
            .collect::<Vec<_>>();

        if participants.is_empty() {
            return Ok(());
        }

        let notification = json!({
            "type": "conversation_deleted",
            "conversation_id": conversation_id.to_string(),
            "reason": reason,
            "deleted_user_id": deleted_user_id.to_string(),
        });

        if let Err(e) = send_event_to_multiple_users(
            state,
            &participants,
            "conversation_deleted",
            &notification,
            Some(conversation_id),
        ).await {
            warn!("Failed to notify conversation_deleted: {}", e);
        }

        Ok(())
    }

    pub async fn get_conversations(
        pool: &sqlx::SqlitePool,
        user_id: Uuid,
        limit: i32,
        before: Option<i64>
    ) -> Result<PaginatedConversationsResponse> {
        let conversations = ConversationRepo::get_conversations(pool, user_id, limit, before).await?;

        let summaries: Vec<ConversationSummary> = conversations
            .iter()
            .map(|conv| {
                let last_message = if let (Some(msg_id), Some(author_id), Some(author_username), Some(content), Some(timestamp)) =
                    (conv.last_msg_id, conv.last_msg_author_id, conv.last_msg_author_username.clone(),
                     conv.last_msg_content.clone(), conv.last_msg_timestamp)
                {
                    Some(Message {
                        id: msg_id,
                        author_id,
                        conversation_id: conv.id,
                        author_username,
                        content,
                        created_at: timestamp,
                        sequence_num: conv.last_msg_sequence,
                    })
                } else {
                    None
                };

                let members = conv.members.as_ref().map(|m| {
                    m.iter()
                        .map(|p| ParticipantOut {
                            user_id: p.user_id,
                            username: p.username.clone(),
                            role: p.role.clone(),
                        })
                        .collect()
                });

                ConversationSummary {
                    conversation: ConversationOut {
                        id: conv.id,
                        kind: conv.kind.clone(),
                        title: conv.title.clone(),
                        owner_id: conv.owner_id,
                        created_at: conv.created_at,
                        last_read_sequence: conv.last_read_sequence,
                        last_activity: conv.last_activity,
                        last_msg_seq: conv.last_msg_seq,
                    },
                    last_message,
                    members,
                }
            })
            .collect();

        let next_cursor = summaries.last().map(|s| s.conversation.last_activity);
        let has_more = summaries.len() == limit as usize;

        Ok(PaginatedConversationsResponse {
            conversations: summaries,
            next_cursor,
            has_more,
        })
    }
}