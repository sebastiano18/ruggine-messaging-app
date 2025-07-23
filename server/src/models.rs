use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub is_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub group_id: Uuid,
    pub sender_id: Uuid,
    pub content: String,
    pub message_type: MessageType,
    pub sent_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    // Campo opzionale per il nome utente (usato nelle query con JOIN)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    SystemNotification,
    UserJoined,
    UserLeft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub group_id: Uuid,
    pub user_id: Uuid,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInvite {
    pub id: Uuid,
    pub group_id: Uuid,
    pub inviter_id: Uuid,
    pub invited_user_id: Uuid,
    pub invited_at: DateTime<Utc>,
    pub status: InviteStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InviteStatus {
    Pending,
    Accepted,
    Declined,
    Expired,
}

// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    // Authentication
    Login { username: String, password: String },
    Register { username: String, email: String, password: String },
    Logout,
    
    // Groups
    CreateGroup { name: String, description: Option<String>, is_private: bool },
    JoinGroup { group_id: Uuid },
    LeaveGroup { group_id: Uuid },
    InviteToGroup { group_id: Uuid, username: String },
    AcceptInvite { invite_id: Uuid },
    DeclineInvite { invite_id: Uuid },
    ListGroups,
    
    // Messages
    SendMessage { group_id: Uuid, content: String },
    GetMessages { group_id: Uuid, limit: Option<u32>, offset: Option<u32> },
    EditMessage { message_id: Uuid, new_content: String },
    DeleteMessage { message_id: Uuid },
    
    // User management
    GetUsers,
    GetUserProfile { user_id: Uuid },
    
    // Ping for connection health
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    // Authentication responses
    LoginSuccess { user: User, token: String },
    LoginFailed { error: String },
    RegisterSuccess { user: User },
    RegisterFailed { error: String },
    
    // Group responses
    GroupCreated { group: Group },
    GroupJoined { group: Group },
    GroupLeft { group_id: Uuid },
    UserInvited { group_id: Uuid, username: String },
    InviteReceived { invite: GroupInvite, group_name: String, inviter_name: String },
    InviteAccepted { group_id: Uuid, username: String },
    InviteDeclined { group_id: Uuid, username: String },
    PendingInvites { invites: Vec<GroupInvite> },
    GroupsList { groups: Vec<Group> },
    
    // Message responses
    MessageSent { message: Message },
    MessageReceived { message: Message },
    MessagesHistory { messages: Vec<Message> },
    MessageEdited { message: Message },
    MessageDeleted { message_id: Uuid },
    
    // User responses
    UsersList { users: Vec<User> },
    UserProfile { user: User },
    UserOnline { user_id: Uuid },
    UserOffline { user_id: Uuid },
    
    // System messages
    Error { error: String },
    Success { message: String },
    NotAuthorized,
    Pong,
}
