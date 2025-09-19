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
    pub last_sequence: u64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_num: Option<u64>,
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

// IMPORTANTE: MessageResponse ora include sequence
#[derive(Deserialize)]
pub struct MessageResponse {
    pub id: String,
    pub author_id: String,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    #[serde(default)]
    pub sequence_num: Option<i64>,  // Nome campo dal server: sequence_num
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Outgoing {
    ChatMessage {
        cid: Uuid,
        content: String,
        target_username: Option<String>,
    },
    InviteUser {
        cid: Uuid,
        username: String,
    },
    Typing {
        cid: Uuid,
        is_typing: bool,
    },
    EnhancedPing {
        user_sequence: Option<u64>,
        conversation_sequence: Option<u64>,
        active_conversation_id: Option<Uuid>,
    },
    RequestUserResume {
        from_sequence: u64,
        limit: i64,
    },
    RequestMessagesResume {
        conversation_id: Uuid,
        from_sequence: u64,
        limit: i64,
    },
    SequenceAck {
        user_sequence: Option<u64>,
        conversation_sequences: Option<HashMap<Uuid, u64>>,
    },
}

// === EVENTI UI UNIFICATI ===
#[derive(Debug)]
pub enum UiEvent {
    // Auth events
    Info(String),
    Error(String),
    LoginStarted,
    RegisterStarted,
    Logged(String, Uuid, u64),
    LoggedOut,

    // WebSocket events
    WsConnected,
    WsDisconnected,
    WsControlReady(crate::api::ws::WsControl),
    WsError(String),
    WsIncoming(MessageDto),

    // Conversation events
    Opened(Uuid),
    ConversationCreated(Uuid),
    DmStubCreated(Uuid, String),
    //DeleteConversation(Uuid),
    ConversationDeleted(Uuid),
    //RequestConversationsRefresh,

    // Data loading events
    ConversationsLoaded(Vec<ConversationDto>),
    AllMessagesLoaded(HashMap<Uuid, Vec<MessageDto>>),
    RefreshedMsgs(Vec<MessageDto>),
    SingleConversationLoaded(ConversationDto),
    InitialLoadComplete,
    LoadingProgress(String),

    // Message events
    MessageSendFailed(Uuid),

    // General events
    InviteCreated(String),

    // Sistema unificato di fetch conversazione
    TriggerConversationFetch(Uuid, String),
    ConversationCompleteFetched(ConversationDto, Vec<MessageDto>),

    // Conversation management events
    ConversationListUpdated,

    // Sistema di sequenze dual
    SendPing,
    EnhancedPongReceived {
        current_user_sequence: u64,
        conversation_sequences: Option<HashMap<String, u64>>,
        gaps_detected: bool,
        user_events_gap: Option<GapInfo>,
        message_gap: Option<GapInfo>,
    },
    UserEventsResume {
        events: Vec<UserEventData>,
    },
    MessagesResume {
        conversation_id: Uuid,
        messages: Vec<MessageDto>,
    },
    UserNotification {
        sequence: u64,
        event_type: String,
        event_data: serde_json::Value,
        conversation_id: Option<Uuid>,
        recovery: bool,
    },
}

#[derive(Debug, Clone)]
pub struct GapInfo {
    pub detected: bool,
    pub client_seq: u64,
    pub server_seq: u64,
    pub gap_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserEventData {
    pub sequence: u64,
    pub event_type: String,
    pub event_data: serde_json::Value,
    pub conversation_id: Option<Uuid>,
    pub created_at: i64,
}

impl MessageDto {
    pub fn system_message(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            author_id: Uuid::nil(),
            conversation_id: Uuid::nil(),
            author_username: "system".to_string(),
            content,
            created_at: chrono::Utc::now().timestamp(),
            sequence_num: None,
        }
    }

    pub fn is_system_message(&self) -> bool {
        self.author_id == Uuid::nil() && self.author_username == "system"
    }

    pub fn fetch_notification(conversation_id: Uuid, message_count: usize, reason: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            author_id: Uuid::nil(),
            conversation_id,
            author_username: "system".to_string(),
            content: format!("Sincronizzati {} messaggi ({})", message_count, reason),
            created_at: chrono::Utc::now().timestamp(),
            sequence_num: None,
        }
    }
}