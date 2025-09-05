// In models (1).rs (server)

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
}

#[derive(FromRow, Debug, Clone, Serialize)]
pub(crate) struct Message {
    pub(crate) id: Uuid,
    pub(crate) author_id: Uuid,
    pub(crate) conversation_id: Uuid,
    pub(crate) author_username: String,
    pub(crate) content: String,
    pub(crate) created_at: i64
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