use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Re-export API response types
pub use crate::api::conversation::PaginatedConversationsResponse;


#[derive(Deserialize)]
pub struct LoginResp {
    pub token: String,
    pub user_id: Uuid,
    pub username: String,
    pub last_sequence: u64,
}


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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_msg_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_confirmed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ConversationDto {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub owner_id: Uuid,
    pub created_at: i64,
    pub last_read_sequence: i64,
    pub last_activity: i64,
    pub last_msg_seq: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationSummary {
    pub conversation: ConversationDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<MessageDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<ParticipantInfo>>,
}


#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParticipantInfo {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub joined_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct MessageResponse {
    pub id: String,
    pub author_id: String,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    #[serde(default)]
    pub sequence_num: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Auth,
    Conversations,
    Chat,
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
        target_usernames: Option<Vec<String>>,
        client_msg_id: Option<String>,
    },
    InviteUser {
        cid: Uuid,
        usernames: Vec<String>,
    },
    CreateGroup {
        group_name: String,
    },
    CreateGroupWithParticipants {
        group_name: String,
        participant_usernames: Vec<String>,
        client_temp_id: Option<String>,
    },
    Typing {
        cid: Uuid,
        is_typing: bool,
    },
    Ping {
        user_sequence: Option<u64>,
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
    DeleteConversation {
        cid: Uuid,
    },
    MarkRead {
        conversation_id: Uuid,
        sequence_num: u64,
    },
    LeaveGroup {
        cid: Uuid,
    },
    RemoveMember {
        cid: Uuid,
        user_id: Uuid,
    },
    DeleteMessage {
        mid: Uuid,
    },
    CheckUser {
        username: String,
        request_id: String,
    },
    DeleteUser,
}

#[derive(Debug)]
pub enum UiEvent {
    Info(String),
    Error(ErrorType),
    LoginStarted,
    RegisterStarted,
    Logged(String, Uuid, u64),
    LoggedOut,
    LoadingError,
    DeleteAccountStart,
    DeleteAccountConfirm,
    DeleteAccountCancel,

    WsConnected,
    WsDisconnected,
    WsControlReady(crate::api::ws::WsControl),
    WsError(String),
    WsIncoming(MessageDto),

    Opened(Uuid),
    Closed(Uuid),
    ConversationCreated(Uuid),
    DmStubCreated(Uuid, String),
    UserCheckResult {
        username: String,
        exists: bool,
        user_id: Option<Uuid>,
        request_id: String,
    },
    ConversationDeleted(Uuid),

    ConversationsLoaded(Vec<ConversationDto>),
    ConversationsAppended(PaginatedConversationsResponse),
    ConversationSummaryFetched(ConversationSummary),
    ConversationFetchFailed(Uuid),
    AllMessagesLoaded(HashMap<Uuid, Vec<MessageDto>>),
    RefreshedMsgs(Vec<MessageDto>),
    SingleConversationLoaded(ConversationDto),
    InitialLoadComplete,
    LoadingProgress(String),

    MessageSendFailed(Uuid),
    MessageConfirmation {
        client_msg_id: String,
        server_msg_id: Uuid,
        sequence: Option<u64>,
        status: String,
    },
    MessageDeleted {
        message_id: Uuid,
        conversation_id: Uuid,
    },

    InviteCreated(String),
    MembersLoaded(Uuid, Vec<ParticipantInfo>),

    TriggerConversationFetch(Uuid, String),
    ConversationCompleteFetched(ConversationDto, Vec<MessageDto>),

    SendPing,
    PongReceived {
        current_user_sequence: u64,
        gaps_detected: bool,
        user_events_gap: Option<GapInfo>,
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

    InitialStateReceived {
        conversations: Vec<ConversationDto>,
        user_sequence: u64,
        members_by_conversation: Option<std::collections::HashMap<Uuid, Vec<ParticipantInfo>>>,
    },
    LastMessageUpdate {
        conversation_id: Uuid,
        message: MessageDto,
    },
    ConversationMessagesReceived {
        conversation_id: Uuid,
        messages: Vec<MessageDto>,
        has_more: bool,
    },

    OlderMessagesLoaded(Vec<MessageDto>),

    ConversationConfirmed {
        conversation: ConversationDto,
        messages: Vec<MessageDto>,
        client_temp_id: Option<String>,
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

#[derive(Debug, Clone)]
pub enum ErrorType {
    Connection,
    MessageSend,
    MessageDelete,
    ConversationDelete,
    GroupLeave,
    GroupCreate,
    Invite,
    GroupRemoveMember,
    Auth(String),
    DataRecovery,
    Generic(String),
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
            client_msg_id: None,
            is_confirmed: None,
        }
    }

    pub fn is_system_message(&self) -> bool {
        if self.author_id == Uuid::nil() && self.author_username == "system" {
            return true;
        }

        self.content.ends_with(" è stato aggiunto al gruppo") ||
            self.content.ends_with(" è stato espulso dal gruppo") ||
            self.content.ends_with(" ha lasciato il gruppo") ||
            self.content.ends_with(" ha eliminato il proprio account")
    }


    pub fn optimistic_message(
        author_id: Uuid,
        author_username: String,
        conversation_id: Uuid,
        content: String,
        client_msg_id: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            author_id,
            author_username,
            conversation_id,
            content,
            created_at: chrono::Utc::now().timestamp(),
            sequence_num: None,
            client_msg_id: Some(client_msg_id),
            is_confirmed: None,
        }
    }
}