use crate::api::ws::WsControl;
use crate::models::*;
use std::collections::HashMap;
use tokio::{runtime::Runtime, sync::mpsc};
use uuid::Uuid;

use super::data_loader::DataLoader;
use super::events::EventHandler;

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
    pub request_conversations_refresh: bool,

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

    // DM stub tracking - conversation_id -> target_username
    pub dm_stubs: HashMap<Uuid, String>,
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
            request_conversations_refresh: false,
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
            ui_to_net_rx,

            conversation_messages: HashMap::new(),
            is_initial_load_complete: false,
            is_loading: false,

            // Inizializza il tracking degli stub DM
            dm_stubs: HashMap::new(),
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

    // === WebSocket helpers ===

    pub fn send_via_websocket(&self, outgoing: Outgoing) {
        if let Err(_) = self.ui_to_net_tx.try_send(outgoing) {
            let _ = self
                .ui_tx
                .send(UiEvent::Error("Impossibile inviare messaggio".to_string()));
        }
    }

    pub fn send_chat_message_ws(&self, content: String) {
        if let Some(cid) = self.cid {
            // Controlla se questa conversazione è uno stub DM appena creato
            let target_username = self.dm_stubs.get(&cid).cloned();

            self.send_via_websocket(Outgoing::ChatMessage {
                cid,
                content,
                target_username
            });
        }
    }

    // === DM stub management ===

    pub fn add_dm_stub(&mut self, conversation_id: Uuid, target_username: String) {
        self.dm_stubs.insert(conversation_id, target_username);
    }

    pub fn remove_dm_stub(&mut self, conversation_id: Uuid) {
        self.dm_stubs.remove(&conversation_id);
    }

    pub fn is_dm_stub(&self, conversation_id: Uuid) -> bool {
        self.dm_stubs.contains_key(&conversation_id)
    }
}
