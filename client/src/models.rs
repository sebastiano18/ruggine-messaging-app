// In models.rs (client)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// === REQUEST/RESPONSE DTOs ===
#[derive(Serialize)]
pub struct RegisterReq<'a> {
    pub username: &'a str,
    pub password: &'a str,
}

#[derive(Deserialize)]
pub struct LoginResp {
    pub token: String,
    pub user_id: Uuid,
    pub username: String,
}

#[derive(Deserialize)]
pub struct UserInfo {
    pub id: Uuid,
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
    pub group_id: Uuid,
}

#[derive(Deserialize)]
pub struct InviteResp {
    pub token: String,
}

#[derive(Serialize)]
pub struct JoinByTokenReq<'a> {
    pub token: &'a str,
}

#[derive(Serialize)]
pub struct SendMsgReq<'a> {
    pub content: &'a str,
}

// === CORE DATA MODELS ===
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MessageDto {
    pub id: Uuid,
    pub author_id: Uuid,
    pub conversation_id: Uuid,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ConversationDto {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub owner_id: Uuid,
    pub created_at: i64,
}

#[derive(Deserialize, Serialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
}

// === ENUMS ===
#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Auth,
    Conversations,
    Chat,
    GroupManagement,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WsStatus {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoginState {
    Idle,
    LoggingIn,
    Registering,
    LoggedIn,
}

#[derive(Debug, Clone)]
pub enum Outgoing {
    ChatMessage { cid: Uuid, content: String },
    InviteUser { cid: Uuid, username: String },
    Typing { cid: Uuid, is_typing: bool },
}

#[derive(Debug, Clone)]
pub enum Incoming {
    ChatMessage {
        id: Uuid,
        cid: Uuid,
        author_id: Uuid,
        author_username: String,
        content: String,
        created_at: i64,
    },
    System {
        text: String,
    },
    Error {
        text: String,
    },
}

// === UI EVENTS ===
#[derive(Debug)]
pub enum UiEvent {
    Info(String),
    Error(String),
    LoginStarted,
    RegisterStarted,
    Logged(String /* token */, Uuid /* user_id */),
    Opened(Uuid),
    WsConnected,
    WsDisconnected,
    WsControlReady(crate::api::ws::WsControl),
    WsError(String),
    WsIncoming(MessageDto),
    RefreshedMsgs(Vec<MessageDto>),
    ConversationsLoaded(Vec<ConversationDto>),
    LoggedOut,
    InviteCreated(String),
    AllMessagesLoaded(HashMap<Uuid, Vec<MessageDto>>),
    InitialLoadComplete,
    LoadingProgress(String),
    MessageSendFailed(Uuid),
    ConversationCreated(Uuid),
}
impl MessageDto {
    /// Helper per creare messaggi di sistema
    pub fn system_message(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            author_id: Uuid::nil(),
            conversation_id: Uuid::nil(),
            author_username: "system".to_string(),
            content,
            created_at: chrono::Utc::now().timestamp(),
        }
    }
}
