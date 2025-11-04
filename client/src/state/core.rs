use crate::api::ws::WsControl;
use crate::app::events::sequence_handler::SequenceHandler;
use crate::models::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::{runtime::Runtime, sync::mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;


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
}

impl CreateGroupPopupState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.group_name.clear();
        self.manual_username_input.clear();
        self.selected_participants.clear();
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

    // DM stub tracking - conversation_id -> target_username
    pub dm_stubs: HashMap<Uuid, String>,

    // Group stub tracking - conversation_id -> group_name
    pub group_stubs: HashMap<Uuid, String>,

    // Message confirmation tracking
    pub pending_confirmations: HashMap<String, MessageDto>, // client_msg_id -> messaggio ottimistico
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

    // User Manag
    pub confirm_delete_account: bool,

    // UI Modals
    pub show_account_modal: bool,

    // UI Messages - separati per pagina
    pub ui_message: Option<String>,        // Messaggi per pagine interne (dopo login)
    pub auth_message: Option<String>,      // Messaggi solo per pagina auth
    pub auth_message_is_error: bool,       // true = errore (rosso), false = info (verde)

    // Reorder Buffers for messages and events
    pub message_reorder_buffer: BTreeMap<Uuid, BTreeMap<u64, MessageDto>>,
    pub user_event_reorder_buffer: BTreeMap<u64, Vec<serde_json::Value>>,

    //Unread message counter
    pub conversation_unread_counts: HashMap<Uuid, i64>,

    // Invite popup state
    pub show_invite_popup: bool,
    pub invite_username_input: String,

    // Create group popup state
    pub show_create_group_modal: bool,
    pub create_group_popup: CreateGroupPopupState,

    // DM management
    pub dm_username: String,
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

            ui_message: None,
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

            // Message confirmation
            pending_confirmations: HashMap::new(),
            confirmation_timeout: Duration::from_secs(10),
            last_confirmation_cleanup: Instant::now(),

            // Dual sequence system
            user_sequence_confirmed: 0,
            user_sequence_received: 0,
            conversation_sequences: HashMap::new(),
            conversation_sequences_confirmed: HashMap::new(),

            ping_interval: Duration::from_secs(30),
            last_ping_time: Instant::now(),
            missed_pings: 0,
            max_missed_pings: 3,
            ping_timeout: Duration::from_secs(45),

            is_recovering_user_events: false,
            is_recovering_messages: HashMap::new(),
            pending_resume_requests: 0,

            sequence_stats: SequenceStats::default(),

            is_loading_more: false,
            has_more_messages: HashMap::new(),
            pending_conversations: HashMap::new(),

            message_reorder_buffer: BTreeMap::new(),
            user_event_reorder_buffer: BTreeMap::new(),

            conversation_unread_counts: HashMap::new(),

            show_invite_popup: false,
            invite_username_input: String::new(),

            // Create group popup
            show_create_group_modal: false,
            create_group_popup: CreateGroupPopupState::new(),

            // DM management
            dm_username: String::new(),
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            crate::app::events::EventDispatcher::handle_event(self, ev);
        }

        // Cleanup periodico delle conferme
        if self.last_confirmation_cleanup.elapsed() > Duration::from_secs(5) {
            self.cleanup_pending_confirmations();
            self.last_confirmation_cleanup = Instant::now();
        }
    }


    pub fn request_delete_confirmation(&mut self, conversation: &ConversationDto) {
        self.pending_deletion = Some(PendingDeletion {
            conversation: conversation.clone(),
        });
    }

    pub fn cancel_delete_confirmation(&mut self) {
        self.pending_deletion = None;
    }

    pub fn execute_pending_deletion(&mut self) {
        let Some(pending) = self.pending_deletion.take() else {
            return;
        };

        let conversation = pending.conversation;
        let cid = conversation.id;

        if self.is_dm_stub(cid) {
            self.remove_dm_stub(cid);
            if let Some(ref mut list) = self.conversations {
                list.retain(|c| c.id != cid);
            }
            self.conversation_messages.remove(&cid);

            if self.cid == Some(cid) {
                self.cid = None;
                self.conv_title.clear();
                self.messages.clear();
                self.page = Page::Conversations;
            }

            let _ = self
                .ui_tx
                .send(UiEvent::Info("Chat privata rimossa (locale)".into()));
        } else {
            if self.ws_status == WsStatus::Connected {
                self.send_via_websocket(Outgoing::DeleteConversation { cid });

                let _ = self
                    .ui_tx
                    .send(UiEvent::Info("Eliminazione conversazione...".into()));
            } else {
                let _ = self
                    .ui_tx
                    .send(UiEvent::Error("Errore di connessione".into()));
            }
        }
    }

    // === WebSocket helpers ===

    pub fn send_via_websocket(&self, outgoing: Outgoing) {
        if let Err(_) = self.ui_to_net_tx.try_send(outgoing) {
            let _ = self
                .ui_tx
                .send(UiEvent::Error("Impossibile inviare messaggio".to_string()));
        }
    }

    pub fn send_chat_message_ws(&self, content: String, client_msg_id: Option<String>) {
        if let Some(cid) = self.cid {
            let target_username = self.dm_stubs.get(&cid).cloned();

            if target_username.is_some() {
                debug!(
                    "Sending message for DM stub {} with target: {:?}",
                    cid, target_username
                );
            }

            self.send_via_websocket(Outgoing::ChatMessage {
                cid,
                content,
                target_username,
                target_usernames: None,
                client_msg_id,
            });
        }
    }

    pub fn send_invite_user(&self, cid: Uuid, username: String) {
        info!("Sending invite for user '{}' to conversation {}", username, cid);
        self.send_via_websocket(Outgoing::InviteUser { cid, username });
    }

    // === Message Confirmation Methods ===

    pub fn cleanup_pending_confirmations(&mut self) {
        let now = chrono::Utc::now().timestamp();
        let timeout_secs = self.confirmation_timeout.as_secs() as i64;

        let mut expired = Vec::new();
        for (client_id, msg) in &self.pending_confirmations {
            if now - msg.created_at > timeout_secs {
                expired.push(client_id.clone());
            }
        }

        for client_id in expired {
            if let Some(_msg) = self.pending_confirmations.remove(&client_id) {
                warn!("Message confirmation timeout for {}", client_id);

                // Marca il messaggio come fallito nell'UI
                for ui_msg in &mut self.messages {
                    if ui_msg.client_msg_id == Some(client_id.clone()) {
                        ui_msg.is_confirmed = Some(false);
                        break;
                    }
                }

                // Notifica l'utente
                let _ = self.ui_tx.send(UiEvent::Info(
                    "⚠️ Messaggio potrebbe non essere stato inviato".into(),
                ));
            }
        }
    }

    // ===Ping System ===

    pub fn get_total_cached_messages(&self) -> usize {
        self.conversation_messages.values().map(|v| v.len()).sum()
    }

    pub fn get_debug_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();

        info.insert("ws_status".to_string(), format!("{:?}", self.ws_status));
        info.insert(
            "user_seq_confirmed".to_string(),
            self.user_sequence_confirmed.to_string(),
        );
        info.insert(
            "user_seq_received".to_string(),
            self.user_sequence_received.to_string(),
        );
        info.insert(
            "active_conversation".to_string(),
            self.cid.map_or("none".to_string(), |id| id.to_string()),
        );
        info.insert(
            "pending_confirmations".to_string(),
            self.pending_confirmations.len().to_string(),
        );

        if let Some(cid) = self.cid {
            let conv_seq = self.conversation_sequences.get(&cid).copied().unwrap_or(0);
            let conv_seq_confirmed = self
                .conversation_sequences_confirmed
                .get(&cid)
                .copied()
                .unwrap_or(0);
            info.insert("conv_seq".to_string(), conv_seq.to_string());
            info.insert(
                "conv_seq_confirmed".to_string(),
                conv_seq_confirmed.to_string(),
            );
        }

        info.insert(
            "sequence_health".to_string(),
            format!("{:.2}", SequenceHandler::get_sequence_health(self)),
        );
        info.insert(
            "ping_count".to_string(),
            self.sequence_stats.ping_count.to_string(),
        );
        info.insert(
            "pong_count".to_string(),
            self.sequence_stats.pong_count.to_string(),
        );
        info.insert(
            "missed_pings".to_string(),
            format!("{}/{}", self.missed_pings, self.max_missed_pings),
        );
        info.insert(
            "gaps_detected".to_string(),
            self.sequence_stats.gaps_detected.to_string(),
        );
        info.insert(
            "pending_resume".to_string(),
            self.pending_resume_requests.to_string(),
        );

        info
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some() && self.user_id.is_some()
    }

    pub fn add_dm_stub(&mut self, conversation_id: Uuid, target_username: String) {
        debug!("Adding DM stub: {} -> {}", conversation_id, target_username);

        if self.dm_stubs.contains_key(&conversation_id) {
            warn!(
                "DM stub already exists for conversation {}",
                conversation_id
            );
            return;
        }

        self.dm_stubs.insert(conversation_id, target_username);
    }

    pub fn remove_dm_stub(&mut self, conversation_id: Uuid) {
        if let Some(target) = self.dm_stubs.remove(&conversation_id) {
            debug!("Removed DM stub: {} -> {}", conversation_id, target);
        }
    }

    pub fn is_dm_stub(&self, conversation_id: Uuid) -> bool {
        self.dm_stubs.contains_key(&conversation_id)
    }

    pub fn cleanup_old_data(&self) {
        let total_messages = self.get_total_cached_messages();
        if total_messages > 50000 {
            warn!(
                "High memory usage detected: {} cached messages",
                total_messages
            );
        }
    }

    pub fn cleanup_dm_stubs(&mut self) {
        let mut to_remove = Vec::new();

        if let Some(ref conversations) = self.conversations {
            for (&stub_id, _) in &self.dm_stubs {
                if conversations.iter().any(|c| c.id == stub_id) {
                    to_remove.push(stub_id);
                }
            }
        }

        for id in to_remove {
            self.dm_stubs.remove(&id);
        }
    }

    pub fn load_older_messages(&mut self) {
        let Some(cid) = self.cid else { return };
        let Some(ref token) = self.token else { return };

        if self.is_loading_more {
            return;
        }

        if !*self.has_more_messages.get(&cid).unwrap_or(&true) {
            return;
        }

        let before_seq = self
            .messages
            .first()
            .and_then(|m| m.sequence_num)
            .map(|seq| seq as i64);

        if let Some(seq) = before_seq {
            if seq <= 1 {
                self.has_more_messages.insert(cid, false);
                info!("First message has sequence 1, no older messages to load");
                return;
            }
        }

        self.is_loading_more = true;

        let base = self.base.clone();
        let token = token.clone();
        let tx = self.ui_tx.clone();

        self.rt.spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            match crate::api::chat::get_messages_paginated(&base, &token, cid, Some(30), before_seq)
                .await
            {
                Ok(messages) => {
                    let _ = tx.send(UiEvent::OlderMessagesLoaded(messages));
                }
                Err(e) => {
                    error!("Failed to load messages: {}", e);
                    let _ = tx.send(UiEvent::LoadingError);
                }
            }
        });
    }

    // UI Message handling
    pub fn set_ui_message(&mut self, msg: String) {
        // Messaggi per le pagine interne (dopo login)
        if self.token.is_some() {
            self.ui_message = Some(msg);
        }
    }

    pub fn clear_ui_message(&mut self) {
        self.ui_message = None;
    }

    pub fn set_auth_message(&mut self, msg: String, is_error: bool) {
        // Messaggi per la pagina di autenticazione
        self.auth_message = Some(msg);
        self.auth_message_is_error = is_error;
    }

    pub fn clear_auth_message(&mut self) {
        self.auth_message = None;
        self.auth_message_is_error = false;
    }

    /// Instrada automaticamente il messaggio alla categoria giusta
    pub fn set_message_info(&mut self, msg: String) {
        if self.token.is_none() {
            self.set_auth_message(msg, false); // Info = non errore
        } else {
            self.set_ui_message(msg);
        }
    }

    pub fn set_message_error(&mut self, msg: String) {
        if self.token.is_none() {
            self.set_auth_message(msg, true); // Error = errore
        } else {
            self.set_ui_message(msg);
        }
    }

}