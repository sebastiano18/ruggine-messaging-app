use crate::api::ws::WsControl;
use crate::models::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::{runtime::Runtime, sync::mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::data_loader::DataLoader;

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

            dm_stubs: HashMap::new(),

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
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            crate::app::events::EventDispatcher::handle_event(self, ev);
        }
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
            });
        }
    }
    

    // === Enhanced Ping System ===

    pub fn send_enhanced_ping(&mut self) {
        // SEMPRE invia user_sequence se > 0
        // Usa il MAX tra confirmed e received per user sequence
        let user_seq = {
            let seq = self.user_sequence_confirmed.max(self.user_sequence_received);
            if seq > 0 { Some(seq) } else { None }
        };

        // Solo se c'è una conversazione attiva, invia la sua sequence
        let (conv_seq, active_conv) = if let Some(cid) = self.cid {
            // Prendi la sequence solo se è stata inizializzata
            // Check se la conversazione ha una sequence tracciata
            let seq = self.conversation_sequences.get(&cid).copied();

            debug!(
                "Conversation {} sequences - confirmed: {:?}, received: {:?}, sending: {:?}",
                cid, 
                self.conversation_sequences_confirmed.get(&cid),
                self.conversation_sequences.get(&cid),
                seq
            );

            // IMPORTANTE: invia la sequence anche se è 0 (se presente nella HashMap)
            // None significa "non sto ancora tracciando questa conversazione"
            // Some(0) significa "sto tracciando ma non ho ancora messaggi"
            (seq, Some(cid))
        } else {
            // Nessuna conversazione selezionata
            (None, None)
        };

        debug!(
            "Sending enhanced ping - user_seq: {:?}, conv_seq: {:?}, active_conv: {:?}",
            user_seq, conv_seq, active_conv
        );

        self.sequence_stats.ping_count += 1;

        // IMPORTANTE: Invia SEMPRE tutti i campi opzionali quando disponibili
        self.send_via_websocket(Outgoing::EnhancedPing {
            user_sequence: user_seq,
            conversation_sequence: conv_seq,
            active_conversation_id: active_conv,
        });
    }

    // === Sequence Update Methods ===

    pub fn update_user_sequence(&mut self, sequence: u64) {
        if sequence > self.user_sequence_received {
            self.user_sequence_received = sequence;
            self.sequence_stats.total_events_received += 1;
        }

        if sequence == self.user_sequence_confirmed + 1 {
            self.user_sequence_confirmed = sequence;
            debug!("User sequence {} confirmed (continuous)", sequence);
        } else if sequence > self.user_sequence_confirmed + 1 {
            let gap_size = sequence - self.user_sequence_confirmed - 1;
            warn!(
                "User sequence gap detected! Expected {}, got {} (missing {} events)",
                self.user_sequence_confirmed + 1, sequence, gap_size
            );
            self.sequence_stats.gaps_detected += 1;
            self.sequence_stats.last_gap_time = Some(Instant::now());
        }
    }

    pub fn update_conversation_sequence(&mut self, conversation_id: Uuid, sequence: u64) {
        let current = self.conversation_sequences.get(&conversation_id).copied().unwrap_or(0);

        if sequence > current {
            self.conversation_sequences.insert(conversation_id, sequence);
        }

        let confirmed = self.conversation_sequences_confirmed.get(&conversation_id).copied().unwrap_or(0);

        if sequence == confirmed + 1 {
            self.conversation_sequences_confirmed.insert(conversation_id, sequence);
            debug!("Conversation {} sequence {} confirmed", conversation_id, sequence);
        } else if sequence > confirmed + 1 {
            let gap_size = sequence - confirmed - 1;
            warn!(
                "Conversation {} sequence gap! Expected {}, got {} (missing {})",
                conversation_id, confirmed + 1, sequence, gap_size
            );
        }
    }

    // === Resume Request Methods ===

    pub fn request_user_events_resume(&mut self, from_sequence: u64) {
        if self.is_recovering_user_events {
            debug!("User events resume already in progress");
            return;
        }

        self.is_recovering_user_events = true;
        self.pending_resume_requests += 1;

        self.send_via_websocket(Outgoing::RequestUserResume {
            from_sequence,
            limit: 100,
        });

        info!("Requested user events resume from sequence {}", from_sequence);
    }

    pub fn request_messages_resume(&mut self, conversation_id: Uuid, from_sequence: u64) {
        if *self.is_recovering_messages.get(&conversation_id).unwrap_or(&false) {
            debug!("Messages resume already in progress for {}", conversation_id);
            return;
        }

        self.is_recovering_messages.insert(conversation_id, true);
        self.pending_resume_requests += 1;

        self.send_via_websocket(Outgoing::RequestMessagesResume {
            conversation_id,
            from_sequence,
            limit: 100,
        });

        info!("Requested messages resume for {} from sequence {}", conversation_id, from_sequence);
    }

    // === Gap Detection ===

    pub fn check_for_gaps(&mut self) -> (bool, bool) {
        let user_gap = self.user_sequence_received > self.user_sequence_confirmed;

        let conversation_gap = if let Some(cid) = self.cid {
            let received = self.conversation_sequences.get(&cid).copied().unwrap_or(0);
            let confirmed = self.conversation_sequences_confirmed.get(&cid).copied().unwrap_or(0);
            received > confirmed
        } else {
            false
        };

        (user_gap, conversation_gap)
    }

    // === Ping/Pong Management ===

    pub fn should_send_ping(&self) -> bool {
        self.ws_status == WsStatus::Connected && self.last_ping_time.elapsed() >= self.ping_interval
    }

    pub fn update_ping_time(&mut self) {
        self.last_ping_time = Instant::now();
    }

    pub fn handle_pong_timeout(&mut self) {
        self.missed_pings += 1;
        warn!(
            "Pong timeout! Missed pings: {}/{}",
            self.missed_pings, self.max_missed_pings
        );

        if self.missed_pings >= self.max_missed_pings {
            error!(
                "Too many missed pongs ({}), forcing reconnection",
                self.missed_pings
            );
            self.request_ws_reconnect = true;
            self.missed_pings = 0;
        }
    }

    pub fn reset_sequence_system(&mut self) {
        self.missed_pings = 0;
        self.is_recovering_user_events = false;
        self.is_recovering_messages.clear();
        self.pending_resume_requests = 0;
        self.last_ping_time = Instant::now();
        debug!("Sequence system reset");
    }

    pub fn reset_sequence_on_disconnect(&mut self) {
        debug!(
            "WebSocket disconnected, preserving sequences - user: {}, conversations: {}",
            self.user_sequence_confirmed,
            self.conversation_sequences.len()
        );
        self.reset_sequence_system();
    }

    // === Health and Statistics ===

    pub fn get_sequence_health(&self) -> f64 {
        if self.sequence_stats.ping_count == 0 {
            return 1.0;
        }

        let pong_rate = self.sequence_stats.pong_count as f64 / self.sequence_stats.ping_count as f64;
        let gap_penalty = (self.sequence_stats.gaps_detected as f64 * 0.1).min(0.5);
        let missed_penalty = (self.missed_pings as f64 / self.max_missed_pings as f64) * 0.3;

        (pong_rate - gap_penalty - missed_penalty).max(0.0)
    }

    pub fn get_total_cached_messages(&self) -> usize {
        self.conversation_messages.values().map(|v| v.len()).sum()
    }

    pub fn get_debug_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();

        info.insert("ws_status".to_string(), format!("{:?}", self.ws_status));
        info.insert("user_seq_confirmed".to_string(), self.user_sequence_confirmed.to_string());
        info.insert("user_seq_received".to_string(), self.user_sequence_received.to_string());
        info.insert("active_conversation".to_string(), self.cid.map_or("none".to_string(), |id| id.to_string()));

        if let Some(cid) = self.cid {
            let conv_seq = self.conversation_sequences.get(&cid).copied().unwrap_or(0);
            let conv_seq_confirmed = self.conversation_sequences_confirmed.get(&cid).copied().unwrap_or(0);
            info.insert("conv_seq".to_string(), conv_seq.to_string());
            info.insert("conv_seq_confirmed".to_string(), conv_seq_confirmed.to_string());
        }

        info.insert("sequence_health".to_string(), format!("{:.2}", self.get_sequence_health()));
        info.insert("ping_count".to_string(), self.sequence_stats.ping_count.to_string());
        info.insert("pong_count".to_string(), self.sequence_stats.pong_count.to_string());
        info.insert("missed_pings".to_string(), format!("{}/{}", self.missed_pings, self.max_missed_pings));
        info.insert("gaps_detected".to_string(), self.sequence_stats.gaps_detected.to_string());
        info.insert("pending_resume".to_string(), self.pending_resume_requests.to_string());

        info
    }

    // === Authentication ===

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some() && self.user_id.is_some()
    }

    // === DM stub management ===

    pub fn add_dm_stub(&mut self, conversation_id: Uuid, target_username: String) {
        debug!("Adding DM stub: {} -> {}", conversation_id, target_username);

        // Verifica duplicati prima di aggiungere
        if self.dm_stubs.contains_key(&conversation_id) {
            warn!("DM stub already exists for conversation {}", conversation_id);
            return;
        }
        println!("aaaaaaaa{:?}", self.conversations);
        if let Some(convs) = &mut self.conversations {
    // remove the first record (index 0)
    convs.remove(0);
}
        println!("eeeeeeee{:?}", self.conversations);
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

    // === Cleanup Methods ===

    pub fn cleanup_old_data(&self) {
        let total_messages = self.get_total_cached_messages();
        if total_messages > 50000 {
            warn!("High memory usage detected: {} cached messages", total_messages);
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
}