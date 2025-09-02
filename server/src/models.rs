use serde::{Serialize, Deserialize};
use sqlx::FromRow;


#[derive(FromRow, Debug, Clone, Serialize)]
pub struct User { pub id: i64, pub username: String, pub pass_hash: String, pub created_at: i64 }


#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Conversation { pub id: i64, pub kind: String, pub title: Option<String>, pub created_at: i64 }


#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Participant { pub conversation_id: i64, pub user_id: i64, pub role: String, pub last_read_msg: Option<i64> }


#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Message { pub id: i64, pub conversation_id: i64, pub author_id: i64, pub body: String, pub created_at: i64 }


#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Group { pub id: i64, pub name: String, pub owner_id: i64, pub created_at: i64 }


#[derive(FromRow, Debug, Clone, Serialize)]
pub struct Invite { pub id: i64, pub group_id: i64, pub token: String, pub expires_at: i64, pub used: i64 }