// state/core.rs - Solo definizione struct e costruttore

use crate::api::ws::WsControl;
use crate::models::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::{runtime::Runtime, sync::mpsc};
use uuid::Uuid;

/// Timeout per gli stub (gruppi e DM)
pub const STUB_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout per la verifica utente
pub const USER_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
pub struct SequenceStats {
    pub total_events_received: u64,
    pub gaps_detected: u32,
    pub events_recovered: u32,
    pub ping_count: u32,
    pub pong_count: u32,
    pub average_gap_size: f64,
    pub last_gap_time: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct PendingDeletion {
    pub conversation: ConversationDto,
}

#[derive(Default)]
pub struct CreateGroupPopupState {
    pub group_name: String,
    pub manual_username_input: String,
    pub selected_participants: HashSet<String>,
    pub search_query: String,
    pub pending_user_verification: Option<String>,
}

impl CreateGroupPopupState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.group_name.clear();
        self.manual_username_input.clear();
        self.selected_participants.clear();
        self.search_query.clear();
        self.pending_user_verification = None;
    }
}

#[derive(Default)]
pub struct InvitePopupState {
    pub search_query: String,
    pub selected_users: HashSet<String>,
    pub pending_user_verification: Option<String>,
}

impl InvitePopupState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.search_query.clear();
        self.selected_users.clear();
        self.pending_user_verification = None;
    }
}

pub struct AppState {
    pub rt: Runtime,
    pub base: String,
    pub username: String,
    pub password: String,
    pub password_confirm: String,
    pub token: Option<String>,
    pub user_id: Option<Uuid>,
    pub page: Page,

    // Chat state
    pub cid: Option<Uuid>,
    pub conv_title: String,
    pub input: String,
    pub messages: Vec<MessageDto>,

    // Conversations
    pub conversations: Option<Vec<ConversationDto>>,
    pub request_conversations_refresh: bool,
    pub pending_deletion: Option<PendingDeletion>,

    // Group management
    pub group_name: String,
    pub dm_user_username_input: String,
    pub last_invite_token: Option<String>,
    pub invite_conversation_id: String,
    pub last_created_invite: Option<String>,

    // WebSocket
    pub ws_status: WsStatus,
    pub request_ws_reconnect: bool,
    pub ws_ctrl: Option<WsControl>,

    // Login state
    pub login_state: LoginState,

    // Event handling
    pub ui_tx: mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<UiEvent>,

    // WebSocket bidirectional communication
    pub ui_to_net_tx: mpsc::Sender<Outgoing>,
    pub ui_to_net_rx: mpsc::Receiver<Outgoing>,

    // Data caching
    pub conversation_messages: HashMap<Uuid, Vec<MessageDto>>,
    pub is_initial_load_complete: bool,
    pub is_loading: bool,

    // Stub tracking
    pub dm_stubs: HashMap<Uuid, (String, Instant)>,
    pub group_stubs: HashMap<Uuid, (String, Instant)>,

    // Message confirmation tracking
    pub pending_confirmations: HashMap<String, MessageDto>,
    pub confirmation_timeout: Duration,
    pub last_confirmation_cleanup: Instant,

    // Dual sequence system
    pub user_sequence_confirmed: u64,
    pub user_sequence_received: u64,
    pub conversation_sequences: HashMap<Uuid, u64>,
    pub conversation_sequences_confirmed: HashMap<Uuid, u64>,

    // Ping/Pong management
    pub ping_interval: Duration,
    pub last_ping_time: Instant,
    pub missed_pings: u32,
    pub max_missed_pings: u32,
    pub ping_timeout: Duration,

    // Recovery state
    pub is_recovering_user_events: bool,
    pub is_recovering_messages: HashMap<Uuid, bool>,
    pub pending_resume_requests: u32,

    // Statistics
    pub sequence_stats: SequenceStats,

    pub is_loading_more: bool,
    pub has_more_messages: HashMap<Uuid, bool>,

    #[allow(dead_code)]
    pub pending_conversations: HashMap<String, ConversationDto>,

    // User Management
    pub confirm_delete_account: bool,

    // UI Modals
    pub show_account_modal: bool,

    // UI Messages
    pub toasts: Vec<Toast>,
    pub auth_message: Option<String>,
    pub auth_message_is_error: bool,

    // Reorder Buffers
    pub message_reorder_buffer: BTreeMap<Uuid, BTreeMap<u64, MessageDto>>,
    pub user_event_reorder_buffer: BTreeMap<u64, Vec<serde_json::Value>>,

    // Unread counter
    pub conversation_unread_counts: HashMap<Uuid, i64>,

    // Popups state
    pub show_invite_popup: bool,
    pub invite_popup: InvitePopupState,
    pub show_group_info_popup: bool,
    pub members_list: HashMap<Uuid, Vec<ParticipantInfo>>,
    pub is_loading_members: bool,
    pub show_create_group_modal: bool,
    pub create_group_popup: CreateGroupPopupState,

    // DM management
    pub dm_username: String,

    // Confirmations
    pub pending_message_deletion: Option<Uuid>,
    pub pending_member_kick: Option<(Uuid, Uuid, String)>,

    // User check state
    pub pending_user_check: Option<String>,
    pub user_check_request_id: Option<String>,
    pub user_check_timestamp: Option<Instant>,
}

impl AppState {
    pub fn new() -> Self {
        let rt = Runtime::new().expect("tokio runtime");
        let (tx, rx) = mpsc::unbounded_channel();
        let (ui_to_net_tx, ui_to_net_rx) = mpsc::channel::<Outgoing>(200);

        Self {
            rt,
            base: std::env::var("RUGGINE_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            username: std::env::var("RUGGINE_USER").unwrap_or_else(|_| "alice".into()),
            password: std::env::var("RUGGINE_PASS").unwrap_or_else(|_| "password".into()),
            password_confirm: String::new(),
            token: None,
            user_id: None,
            page: Page::Auth,

            cid: None,
            conv_title: String::new(),
            input: String::new(),
            messages: vec![],

            conversations: None,
            request_conversations_refresh: false,
            pending_deletion: None,
            group_name: String::new(),
            dm_user_username_input: String::new(),
            last_invite_token: None,

            invite_conversation_id: String::new(),
            last_created_invite: None,

            ws_status: WsStatus::Disconnected,
            request_ws_reconnect: false,
            ws_ctrl: None,

            login_state: LoginState::Idle,
            confirm_delete_account: false,
            show_account_modal: false,

            auth_message: None,
            auth_message_is_error: false,

            ui_tx: tx,
            ui_rx: rx,
            ui_to_net_tx,
            ui_to_net_rx,

            conversation_messages: HashMap::new(),
            is_initial_load_complete: false,
            is_loading: false,

            dm_stubs: HashMap::new(),
            group_stubs: HashMap::new(),

            pending_confirmations: HashMap::new(),
            confirmation_timeout: Duration::from_secs(10),
            last_confirmation_cleanup: Instant::now(),

            user_sequence_confirmed: 0,
            user_sequence_received: 0,
            conversation_sequences: HashMap::new(),
            conversation_sequences_confirmed: HashMap::new(),

            ping_interval: Duration::from_secs(30),
            last_ping_time: Instant::now(),
            missed_pings: 0,
            max_missed_pings: 3,
            ping_timeout: Duration::from_secs(15),

            is_recovering_user_events: false,
            is_recovering_messages: HashMap::new(),
            pending_resume_requests: 0,

            sequence_stats: SequenceStats::default(),

            is_loading_more: false,
            has_more_messages: HashMap::new(),
            pending_conversations: HashMap::new(),
            toasts: Vec::new(),

            message_reorder_buffer: BTreeMap::new(),
            user_event_reorder_buffer: BTreeMap::new(),

            conversation_unread_counts: HashMap::new(),

            show_invite_popup: false,
            invite_popup: InvitePopupState::new(),
            show_group_info_popup: false,
            members_list: HashMap::new(),
            is_loading_members: false,
            show_create_group_modal: false,
            create_group_popup: CreateGroupPopupState::new(),

            dm_username: String::new(),
            pending_message_deletion: None,
            pending_member_kick: None,

            pending_user_check: None,
            user_check_request_id: None,
            user_check_timestamp: None,
        }
    }
}

// === Toast models ===
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: Uuid,
    pub message: String,
    pub kind: ToastKind,
    pub created: Instant,
}