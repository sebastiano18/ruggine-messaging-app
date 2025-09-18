use crate::api::ws::WsControl;
use crate::models::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::{runtime::Runtime, sync::mpsc};
use tracing::{debug, error, warn};
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

    // Sistema di sequenze - SEMPLIFICATO
    pub last_sequence_received: u64,      // La più alta ricevuta (per stats)
    pub last_sequence_confirmed: u64,     // L'ultima confermata senza gap
    pub ping_interval: Duration,
    pub last_ping_time: Instant,
    pub missed_pings: u32,
    pub max_missed_pings: u32,
    pub ping_timeout: Duration,
    pub is_recovering_sequence: bool,
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

            // Sistema sequenze - INIZIALIZZATO
            last_sequence_received: 0,
            last_sequence_confirmed: 0,  // NUOVO: inizializzato a 0
            ping_interval: Duration::from_secs(30),
            last_ping_time: Instant::now(),
            missed_pings: 0,
            max_missed_pings: 3,
            ping_timeout: Duration::from_secs(45),
            is_recovering_sequence: false,
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

    // === Sequence System Methods ===

    pub fn send_ping(&mut self) {
        debug!(
            "Sending ping with last_sequence_confirmed: {} (highest received: {})",
            self.last_sequence_confirmed, self.last_sequence_received
        );

        // Aggiorna statistiche PRIMA di inviare
        self.sequence_stats.ping_count += 1;

        // IMPORTANTE: Usa last_sequence_confirmed, non last_sequence_received
        self.send_via_websocket(Outgoing::Ping {
            last_sequence: self.last_sequence_confirmed,
        });
    }

    pub fn update_sequence(&mut self, sequence: u64) {
        // Aggiorna sempre la più alta ricevuta per statistiche
        if sequence > self.last_sequence_received {
            self.last_sequence_received = sequence;
            self.sequence_stats.total_events_received += 1;
        }

        // Aggiorna la sequenza confermata solo se è consecutiva
        if sequence == self.last_sequence_confirmed + 1 {
            // Sequenza consecutiva, nessun gap
            self.last_sequence_confirmed = sequence;
            debug!("Sequence {} confirmed (continuous)", sequence);

        } else if sequence > self.last_sequence_confirmed + 1 {
            // Gap rilevato
            let gap_size = sequence - self.last_sequence_confirmed - 1;
            warn!(
                "Sequence gap detected! Expected {}, got {} (missing {} events)",
                self.last_sequence_confirmed + 1, sequence, gap_size
            );

            // Aggiorna statistiche di gap
            self.sequence_stats.gaps_detected += 1;
            self.sequence_stats.last_gap_time = Some(Instant::now());

            // Calcola media gap size
            let total_gap_size = self.sequence_stats.average_gap_size
                * (self.sequence_stats.gaps_detected - 1) as f64;
            self.sequence_stats.average_gap_size =
                (total_gap_size + gap_size as f64) / self.sequence_stats.gaps_detected as f64;

            // NON aggiornare last_sequence_confirmed!
            // Il server se ne accorgerà al prossimo ping e manderà gli eventi mancanti

        } else {
            // Sequenza vecchia o duplicata (possibile recovery)
            debug!(
                "Old/duplicate sequence {} (confirmed: {}, highest: {})",
                sequence, self.last_sequence_confirmed, self.last_sequence_received
            );

            // Se è una sequenza di recovery che riempie un gap, aggiorna confirmed
            if sequence > self.last_sequence_confirmed && sequence <= self.last_sequence_received {
                // Potrebbe essere un evento di recovery che riempie un gap
                debug!("Possible recovery sequence {}", sequence);
            }
        }
    }

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
        self.is_recovering_sequence = false;
        self.last_ping_time = Instant::now();
        // Mantieni last_sequence_confirmed per continuità
        debug!(
            "Sequence system reset, preserving confirmed sequence: {}",
            self.last_sequence_confirmed
        );
    }

    pub fn reset_sequence_on_disconnect(&mut self) {
        // Mantieni sequence per continuità tra disconnessioni
        debug!(
            "WebSocket disconnected, sequence preserved - confirmed: {}, highest: {}",
            self.last_sequence_confirmed, self.last_sequence_received
        );
        self.reset_sequence_system();
    }

    // === Health and Statistics ===

    pub fn get_sequence_health(&self) -> f64 {
        if self.sequence_stats.ping_count == 0 {
            return 1.0;
        }

        // Calcola tasso di successo pong
        let pong_rate =
            self.sequence_stats.pong_count as f64 / self.sequence_stats.ping_count as f64;

        // Penalità per gap di sequenza
        let gap_penalty = (self.sequence_stats.gaps_detected as f64 * 0.1).min(0.5);

        // Penalità per ping mancanti
        let missed_penalty = (self.missed_pings as f64 / self.max_missed_pings as f64) * 0.3;

        // Penalità per gap non risolti
        let unresolved_gap = if self.last_sequence_received > self.last_sequence_confirmed {
            0.2
        } else {
            0.0
        };

        (pong_rate - gap_penalty - missed_penalty - unresolved_gap).max(0.0)
    }

    pub fn get_total_cached_messages(&self) -> usize {
        self.conversation_messages.values().map(|v| v.len()).sum()
    }

    pub fn get_debug_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();

        info.insert("ws_status".to_string(), format!("{:?}", self.ws_status));
        info.insert(
            "last_sequence_confirmed".to_string(),
            self.last_sequence_confirmed.to_string(),
        );
        info.insert(
            "last_sequence_received".to_string(),
            self.last_sequence_received.to_string(),
        );

        let gap_size = if self.last_sequence_received > self.last_sequence_confirmed {
            self.last_sequence_received - self.last_sequence_confirmed
        } else {
            0
        };
        info.insert("current_gap".to_string(), gap_size.to_string());

        info.insert(
            "sequence_health".to_string(),
            format!("{:.2}", self.get_sequence_health()),
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
            "events_recovered".to_string(),
            self.sequence_stats.events_recovered.to_string(),
        );
        info.insert(
            "conversations".to_string(),
            self.conversations
                .as_ref()
                .map_or(0, |c| c.len())
                .to_string(),
        );
        info.insert(
            "cached_messages".to_string(),
            self.get_total_cached_messages().to_string(),
        );
        info.insert("dm_stubs".to_string(), self.dm_stubs.len().to_string());

        if self.sequence_stats.gaps_detected > 0 {
            info.insert(
                "avg_gap_size".to_string(),
                format!("{:.1}", self.sequence_stats.average_gap_size),
            );
        }

        info
    }

    // === Authentication ===

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some() && self.user_id.is_some()
    }

    // === DM stub management ===

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
        debug!(
            "DM stub added successfully. Total stubs: {}",
            self.dm_stubs.len()
        );
    }

    pub fn remove_dm_stub(&mut self, conversation_id: Uuid) {
        if let Some(target) = self.dm_stubs.remove(&conversation_id) {
            debug!("Removed DM stub: {} -> {}", conversation_id, target);
        } else {
            debug!(
                "Attempted to remove non-existent DM stub: {}",
                conversation_id
            );
        }
    }

    pub fn is_dm_stub(&self, conversation_id: Uuid) -> bool {
        self.dm_stubs.contains_key(&conversation_id)
    }

    // === Cleanup Methods ===

    pub fn cleanup_old_data(&self) {
        let total_messages = self.get_total_cached_messages();
        let conversation_count = self.conversation_messages.len();

        debug!(
            "Cleanup check: {} messages across {} conversations",
            total_messages, conversation_count
        );

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
            for (&stub_id, target_username) in &self.dm_stubs {
                if conversations.iter().any(|c| c.id == stub_id) {
                    debug!(
                    "DM stub {} -> {} now has real conversation, marking for cleanup",
                    stub_id, target_username
                );
                    to_remove.push(stub_id);
                }
            }
        }

        let removed_count = to_remove.len();

        for id in to_remove {
            self.dm_stubs.remove(&id);
        }

        if removed_count > 0 {
            debug!("Cleaned up {} old DM stubs", removed_count);
        }
    }
}