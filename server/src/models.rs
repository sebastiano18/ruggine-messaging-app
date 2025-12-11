use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow, Debug, Clone, Serialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub pass_hash: String,
    pub created_at: i64,
}

#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: Uuid,
    pub kind: String,
    pub title: Option<String>,
    pub created_at: i64,
}

#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Participant {
    pub conversation_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub last_read_msg: Option<Uuid>,
    pub last_read_sequence: i64,
}

#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Message {
    pub id: Uuid,
    pub author_id: Uuid,
    pub conversation_id: Uuid,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub sequence_num: Option<i64>,
}

/// Conversation with optional last message - returned by repository queries
#[derive(Debug, Clone, Serialize)]
pub struct ConversationWithLastMessage {
    // Core conversation fields
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub owner_id: Uuid,
    pub created_at: i64,
    pub last_read_sequence: i64,
    pub last_activity: i64,
    pub last_msg_seq: i64,

    // Optional last message fields (populated via LEFT JOIN)
    pub last_msg_id: Option<Uuid>,
    pub last_msg_author_id: Option<Uuid>,
    pub last_msg_author_username: Option<String>,
    pub last_msg_content: Option<String>,
    pub last_msg_timestamp: Option<i64>,
    pub last_msg_sequence: Option<i64>,
}

#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub created_at: i64,
}

#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Invite {
    pub id: Uuid,
    pub group_id: Uuid,
    pub token: String,
    pub expires_at: i64,
    pub used: i64,
}