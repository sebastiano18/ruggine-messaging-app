use crate::models::*;
use crate::net::ws::WsControl;
use tokio::{runtime::Runtime, sync::mpsc};
use uuid::Uuid;
use std::collections::HashMap;

use super::events::EventHandler;
use super::data_loader::DataLoader;

pub struct AppState {
    pub rt: Runtime,
    pub base: String,
    pub username: String,
    pub password: String,
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

    pub ui_to_net_tx: mpsc::Sender<Outgoing>,

    // Data caching
    pub conversation_messages: HashMap<Uuid, Vec<MessageDto>>,
    pub is_initial_load_complete: bool,
    pub is_loading: bool,
}

impl AppState {
    pub fn new() -> Self {
        let rt = Runtime::new().expect("tokio runtime");
        let (tx, rx) = mpsc::unbounded_channel();
        let (ui_to_net_tx, ui_to_net_rx) = mpsc::channel::<Outgoing>(200);

        Self {
            rt,
            base: std::env::var("RUGGINE_BASE").unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            username: std::env::var("RUGGINE_USER").unwrap_or_else(|_| "alice".into()),
            password: std::env::var("RUGGINE_PASS").unwrap_or_else(|_| "password".into()),
            token: None,
            user_id: None,
            page: Page::Auth,

            cid: None,
            conv_title: String::new(),
            input: String::new(),
            messages: vec![],

            conversations: None,
            group_name: String::new(),
            dm_user_username_input: String::new(),
            last_invite_token: None,

            invite_conversation_id: String::new(),
            last_created_invite: None,

            ws_status: WsStatus::Disconnected,
            request_ws_reconnect: false,
            ws_ctrl: None,

            login_state: LoginState::Idle,

            ui_tx: tx,
            ui_rx: rx,
            ui_to_net_tx,
            

            conversation_messages: HashMap::new(),
            is_initial_load_complete: false,
            is_loading: false,
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            EventHandler::handle_event(self, ev);
        }
    }

    pub fn preload_all_data(&mut self, token: String) {
        DataLoader::preload_all_data(self, token);
    }

    pub fn load_single_conversation_messages(&self, cid: Uuid) {
        DataLoader::load_single_conversation_messages(self, cid);
    }

    // === Conversation helpers ===
    pub fn current_conversation_title(&self) -> String {
        if let (Some(cid), Some(ref conversations)) = (self.cid, &self.conversations) {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                return conv.title.clone();
            }
        }
        "Conversazione sconosciuta".to_string()
    }

    pub fn current_conversation_kind(&self) -> Option<String> {
        if let (Some(cid), Some(ref conversations)) = (self.cid, &self.conversations) {
            if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                return Some(conv.kind.clone());
            }
        }
        None
    }

    pub fn current_conversation_id_string(&self) -> Option<String> {
        self.cid.map(|id| id.to_string())
    }

    pub fn has_current_conversation(&self) -> bool {
        self.cid.is_some()
    }

    pub fn current_conversation_info(&self) -> Option<&ConversationDto> {
        if let (Some(cid), Some(ref conversations)) = (self.cid, &self.conversations) {
            conversations.iter().find(|c| c.id == cid)
        } else {
            None
        }
    }
}