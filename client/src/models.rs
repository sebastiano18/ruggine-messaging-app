// In models.rs (client)

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct RegisterReq<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

#[derive(Deserialize)]
pub struct LoginResp {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Deserialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct LoginReq<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

#[derive(Serialize)]
pub struct GroupReq<'a> {
    pub name: &'a str,
}

#[derive(Serialize)]
pub struct InviteReq {
    pub group_id: i64,
}
#[derive(Deserialize)]
pub struct InviteResp {
    pub token: String,
}

#[derive(Serialize)]
pub struct JoinByTokenReq<'a> {
    pub token: &'a str,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MessageDto {
    pub id: i64,
    pub conversation_id: i64,
    pub author_id: i64,
    pub content: String, // Corrected field name
    pub created_at: i64, // Corrected data type
}
#[derive(Serialize)]
pub struct SendMsgReq<'a> {
    pub content: &'a str, // Corrected field name
}

#[derive(Deserialize, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
}