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
    pub sequence_num: Option<i64>, // AGGIUNTO: supporto sequence per messaggi
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