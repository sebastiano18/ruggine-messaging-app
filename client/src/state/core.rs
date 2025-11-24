use crate::api::ws::WsControl;
use crate::app::events::sequence_handler::SequenceHandler;
use crate::models::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::{runtime::Runtime, sync::mpsc};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Timeout per gli stub (gruppi e DM) - se non confermati entro questo tempo, vengono rimossi
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
    }
}

#[derive(Default)]
pub struct InvitePopupState {
    pub search_query: String,
    pub selected_users: HashSet<String>,
}

impl InvitePopupState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.search_query.clear();
        self.selected_users.clear();
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

    // DM stub tracking - conversation_id -> (target_username, created_at)
    pub dm_stubs: HashMap<Uuid, (String, Instant)>,

    // Group stub tracking - conversation_id -> (group_name, created_at)
    pub group_stubs: HashMap<Uuid, (String, Instant)>,

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
    pub toasts: Vec<Toast>,           // Toast notifications
    pub auth_message: Option<String>, // Messaggi solo per pagina auth
    pub auth_message_is_error: bool,  // true = errore (rosso), false = info (verde)

    // Reorder Buffers for messages and events
    pub message_reorder_buffer: BTreeMap<Uuid, BTreeMap<u64, MessageDto>>,
    pub user_event_reorder_buffer: BTreeMap<u64, Vec<serde_json::Value>>,

    //Unread message counter
    pub conversation_unread_counts: HashMap<Uuid, i64>,

    // Invite popup state
    pub show_invite_popup: bool,
    pub invite_popup: InvitePopupState,

    // Members popup state
    pub show_group_info_popup: bool,
    pub members_list: HashMap<Uuid, Vec<ParticipantInfo>>,
    pub is_loading_members: bool,

    // Create group popup state
    pub show_create_group_modal: bool,
    pub create_group_popup: CreateGroupPopupState,

    // DM management
    pub dm_username: String,

    // Message deletion confirmation
    pub pending_message_deletion: Option<Uuid>,

    // Member kick confirmation - (conversation_id, user_id, username)
    pub pending_member_kick: Option<(Uuid, Uuid, String)>,

    // NUOVO: User check state per validazione DM
    pub pending_user_check: Option<String>,        // username in verifica
    pub user_check_request_id: Option<String>,     // request_id per correlare
    pub user_check_timestamp: Option<Instant>,     // per timeout
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
            invite_popup: InvitePopupState::new(),

            // Create group popup
            show_create_group_modal: false,
            create_group_popup: CreateGroupPopupState::new(),

            // DM management
            dm_username: String::new(),

            show_group_info_popup: false,
            members_list: HashMap::new(),
            is_loading_members: false,

            // Toasts
            toasts: Vec::new(),

            // Message deletion confirmation
            pending_message_deletion: None,

            // Member kick confirmation
            pending_member_kick: None,

            // NUOVO: User check state
            pending_user_check: None,
            user_check_request_id: None,
            user_check_timestamp: None,
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            crate::app::events::EventDispatcher::handle_event(self, ev);
        }

        // Cleanup periodico delle conferme
        if self.last_confirmation_cleanup.elapsed() > Duration::from_secs(5) {
            self.cleanup_pending_confirmations();
            self.cleanup_pending_user_check(); // NUOVO: cleanup user check timeout
            self.last_confirmation_cleanup = Instant::now();
        }
    }

    // ============================================
    // NUOVE FUNZIONI PER VALIDAZIONE UTENTE
    // ============================================

    /// Richiede la creazione di un DM - prima verifica che l'utente esista
    pub fn request_dm_creation(&mut self, target_username: String) {
        // CHECK CONNESSIONE
        if self.ws_status != WsStatus::Connected {
            warn!("Cannot create DM: WebSocket not connected");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            return;
        }

        // Pulisci username
        let username = target_username.trim().to_string();
        if username.is_empty() {
            let _ = self.ui_tx.send(UiEvent::Error(
                ErrorType::Generic("Username non può essere vuoto".to_string())
            ));
            return;
        }

        // Non permettere DM con se stessi
        if username.to_lowercase() == self.username.to_lowercase() {
            let _ = self.ui_tx.send(UiEvent::Error(
                ErrorType::Generic("Non puoi chattare con te stesso".to_string())
            ));
            return;
        }

        // Controlla se esiste già una conversazione con questo utente
        if let Some(ref convs) = self.conversations {
            if convs.iter().any(|c| c.kind == "dm" && c.title.to_lowercase() == username.to_lowercase()) {
                let _ = self.ui_tx.send(UiEvent::Error(
                    ErrorType::Generic("Esiste già una chat con questo utente".to_string())
                ));
                return;
            }
        }

        // Controlla se c'è già un check in corso
        if self.pending_user_check.is_some() {
            warn!("User check already in progress");
            return;
        }

        // Genera request_id per correlare richiesta/risposta
        let request_id = Uuid::new_v4().to_string();

        // Salva stato pending
        self.pending_user_check = Some(username.clone());
        self.user_check_request_id = Some(request_id.clone());
        self.user_check_timestamp = Some(Instant::now());

        info!("Checking if user exists: {}", username);

        // Invia richiesta di verifica via WebSocket
        self.send_via_websocket(Outgoing::CheckUser {
            username,
            request_id,
        });
    }

    /// Callback quando riceviamo la risposta del check utente
    pub fn handle_user_check_result(
        &mut self,
        username: String,
        exists: bool,
        user_id: Option<Uuid>,
        request_id: String,
    ) {
        // Verifica che sia la risposta che aspettavamo
        if self.user_check_request_id.as_ref() != Some(&request_id) {
            warn!("Received stale user check response for request_id: {}", request_id);
            return;
        }

        // Pulisci stato pending
        self.pending_user_check = None;
        self.user_check_request_id = None;
        self.user_check_timestamp = None;

        if exists {
            info!("User '{}' exists (id: {:?}), creating DM stub", username, user_id);

            // Utente esiste! Crea lo stub
            if let Some(stub_id) = self.create_dm_stub(username.clone()) {
                let _ = self.ui_tx.send(UiEvent::DmStubCreated(stub_id, username));
            }
        } else {
            info!("User '{}' not found", username);

            // Utente non esiste - mostra errore
            let _ = self.ui_tx.send(UiEvent::Error(
                ErrorType::Generic(format!("Utente '{}' non trovato", username))
            ));
        }
    }

    /// Verifica se c'è un check utente in timeout e lo pulisce
    pub fn cleanup_pending_user_check(&mut self) {
        if let Some(timestamp) = self.user_check_timestamp {
            if timestamp.elapsed() > USER_CHECK_TIMEOUT {
                warn!(
                    "User check timeout for username: {:?}",
                    self.pending_user_check
                );

                // Mostra errore
                let _ = self.ui_tx.send(UiEvent::Error(
                    ErrorType::Generic("Timeout verifica utente. Riprova.".to_string())
                ));

                // Pulisci stato
                self.pending_user_check = None;
                self.user_check_request_id = None;
                self.user_check_timestamp = None;
            }
        }
    }

    /// Verifica se un check utente è in corso
    pub fn is_checking_user(&self) -> bool {
        self.pending_user_check.is_some()
    }

    // ============================================
    // FINE NUOVE FUNZIONI
    // ============================================

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
                // Determina se l'utente è owner o partecipante
                let is_owner = self
                    .user_id
                    .map_or(false, |uid| uid == conversation.owner_id);

                if conversation.kind == "group" && !is_owner {
                    // Partecipante che vuole uscire dal gruppo
                    self.send_via_websocket(Outgoing::LeaveGroup { cid });
                    let _ = self
                        .ui_tx
                        .send(UiEvent::Info("Uscita dal gruppo...".into()));
                } else {
                    // Owner che elimina il gruppo o eliminazione di DM
                    self.send_via_websocket(Outgoing::DeleteConversation { cid });

                    if conversation.kind == "group" {
                        let _ = self.ui_tx.send(UiEvent::Info("Gruppo eliminato".into()));
                    } else {
                        let _ = self
                            .ui_tx
                            .send(UiEvent::Info("Conversazione eliminata".into()));
                    }
                }
            } else {
                // Connessione non attiva
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            }
        }
    }

    // === WebSocket helpers ===

    pub fn send_via_websocket(&self, outgoing: Outgoing) {
        let error_type = match &outgoing {
            Outgoing::ChatMessage { .. } => ErrorType::MessageSend,
            Outgoing::DeleteConversation { .. } => ErrorType::ConversationDelete,
            Outgoing::InviteUser { .. } => ErrorType::Invite,
            Outgoing::LeaveGroup { .. } => ErrorType::GroupLeave,
            Outgoing::DeleteMessage { .. } => ErrorType::MessageDelete,
            Outgoing::CreateGroup { .. } | Outgoing::CreateGroupWithParticipants { .. } => {
                ErrorType::GroupCreate
            }
            Outgoing::RequestUserResume { .. } | Outgoing::RequestMessagesResume { .. } => {
                ErrorType::DataRecovery
            }
            Outgoing::CheckUser { .. } => ErrorType::Connection, // NUOVO
            _ => return, // Silenzioso per Ping, Typing, etc.
        };

        if let Err(_) = self.ui_to_net_tx.try_send(outgoing) {
            warn!("Failed to send message to WebSocket channel");
            let _ = self.ui_tx.send(UiEvent::Error(error_type));
        }
    }

    pub fn send_chat_message_ws(&self, content: String, client_msg_id: Option<String>) {
        if let Some(cid) = self.cid {
            // Estrai solo lo username dalla tupla (username, created_at)
            let target_username = self.dm_stubs.get(&cid).map(|(username, _)| username.clone());

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

    pub fn send_invite_users(&self, cid: Uuid, usernames: Vec<String>) {
        info!(
            "Sending invite for {} users to conversation {}",
            usernames.len(),
            cid
        );
        self.send_via_websocket(Outgoing::InviteUser { cid, usernames });
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
            if let Some(msg) = self.pending_confirmations.remove(&client_id) {
                warn!("Message confirmation timeout for {}", client_id);

                // Marca come fallito il messaggio nella UI
                let _ = self.ui_tx.send(UiEvent::MessageSendFailed(msg.id));

                // Aggiorna il messaggio nella cache
                if let Some(messages) = self.conversation_messages.get_mut(&msg.conversation_id) {
                    for m in messages.iter_mut() {
                        if m.id == msg.id {
                            m.is_confirmed = Some(false);
                            break;
                        }
                    }
                }

                // Aggiorna nella lista messaggi corrente
                for m in self.messages.iter_mut() {
                    if m.id == msg.id {
                        m.is_confirmed = Some(false);
                        break;
                    }
                }

                info!("Marked message {} as failed after timeout", msg.id);
            }
        }
    }

    pub fn add_dm_stub(&mut self, stub_id: Uuid, target_username: String) {
        self.dm_stubs.insert(stub_id, (target_username.clone(), Instant::now()));
        debug!("Added DM stub: {} -> {}", stub_id, target_username);
    }

    pub fn add_group_stub(&mut self, stub_id: Uuid, group_name: String) {
        self.group_stubs.insert(stub_id, (group_name.clone(), Instant::now()));
        debug!("Added group stub: {} -> {}", stub_id, group_name);
    }

    pub fn remove_dm_stub(&mut self, conversation_id: Uuid) {
        if let Some((target, _)) = self.dm_stubs.remove(&conversation_id) {
            debug!("Removed DM stub: {} -> {}", conversation_id, target);
        }
    }

    pub fn remove_group_stub(&mut self, conversation_id: Uuid) {
        if let Some((name, _)) = self.group_stubs.remove(&conversation_id) {
            debug!("Removed group stub: {} -> {}", conversation_id, name);
        }
    }

    pub fn is_dm_stub(&self, conversation_id: Uuid) -> bool {
        self.dm_stubs.contains_key(&conversation_id)
    }

    pub fn is_group_stub(&self, conversation_id: Uuid) -> bool {
        self.group_stubs.contains_key(&conversation_id)
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

    /// Pulisce gli stub scaduti (sia DM che gruppi) e quelli già confermati
    pub fn cleanup_expired_stubs(&mut self) {
        let now = Instant::now();

        // === Cleanup group stubs scaduti ===
        let expired_groups: Vec<Uuid> = self.group_stubs
            .iter()
            .filter(|(_, (_, created))| now.duration_since(*created) > STUB_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();

        for stub_id in expired_groups {
            if let Some((name, _)) = self.group_stubs.remove(&stub_id) {
                warn!("Removing expired group stub: {} ({})", name, stub_id);

                // Rimuovi dalla lista conversazioni
                if let Some(ref mut convs) = self.conversations {
                    convs.retain(|c| c.id != stub_id);
                }

                // Rimuovi messaggi cached
                self.conversation_messages.remove(&stub_id);

                // Se era la conversazione attiva, torna alla lista
                if self.cid == Some(stub_id) {
                    self.cid = None;
                    self.page = Page::Conversations;
                    self.messages.clear();
                    self.conv_title.clear();
                }

                // Notifica l'utente
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::GroupCreate));
            }
        }

        // === Cleanup DM stubs scaduti ===
        let expired_dms: Vec<Uuid> = self.dm_stubs
            .iter()
            .filter(|(_, (_, created))| now.duration_since(*created) > STUB_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();

        for stub_id in expired_dms {
            if let Some((username, _)) = self.dm_stubs.remove(&stub_id) {
                warn!("Removing expired DM stub: {} ({})", username, stub_id);

                // Rimuovi dalla lista conversazioni
                if let Some(ref mut convs) = self.conversations {
                    convs.retain(|c| c.id != stub_id);
                }

                // Rimuovi messaggi cached
                self.conversation_messages.remove(&stub_id);

                // Se era la conversazione attiva, torna alla lista
                if self.cid == Some(stub_id) {
                    self.cid = None;
                    self.page = Page::Conversations;
                    self.messages.clear();
                    self.conv_title.clear();
                }

                // Notifica l'utente
                let _ = self.ui_tx.send(UiEvent::Error(ErrorType::MessageSend));
            }
        }

        // === Cleanup stub già confermati (conversazioni reali con stesso ID) ===
        let mut confirmed_dm_stubs = Vec::new();
        let mut confirmed_group_stubs = Vec::new();

        if let Some(ref conversations) = self.conversations {
            for (&stub_id, _) in &self.dm_stubs {
                if conversations.iter().any(|c| c.id == stub_id && c.last_msg_seq > 0) {
                    confirmed_dm_stubs.push(stub_id);
                }
            }
            for (&stub_id, _) in &self.group_stubs {
                if conversations.iter().any(|c| c.id == stub_id && c.last_msg_seq > 0) {
                    confirmed_group_stubs.push(stub_id);
                }
            }
        }

        for id in confirmed_dm_stubs {
            if let Some((username, _)) = self.dm_stubs.remove(&id) {
                debug!("Cleaned up confirmed DM stub: {} -> {}", id, username);
            }
        }
        for id in confirmed_group_stubs {
            if let Some((name, _)) = self.group_stubs.remove(&id) {
                debug!("Cleaned up confirmed group stub: {} -> {}", id, name);
            }
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

    pub fn load_conversation_members(&mut self, conversation_id: Uuid, token: String) {
        if self.is_loading_members {
            return;
        }

        self.is_loading_members = true;
        let base = self.base.clone();
        let tx = self.ui_tx.clone();

        self.rt.spawn(async move {
            match crate::api::conversation::get_conversation_members(&base, &token, conversation_id)
                .await
            {
                Ok(members) => {
                    let _ = tx.send(UiEvent::MembersLoaded(conversation_id, members));
                }
                Err(e) => {
                    error!("Failed to load members: {}", e);
                    let _ = tx.send(UiEvent::Error(ErrorType::Generic(
                        format!("Errore caricamento membri: {}", e)
                    )));
                }
            }
        });
    }

    pub fn kick_member(&mut self, conversation_id: Uuid, user_id: Uuid) {
        let Some(ref token) = self.token else { return };

        let base = self.base.clone();
        let token = token.clone();

        self.rt.spawn(async move {
            match crate::api::conversation::kick_member(&base, &token, conversation_id, user_id)
                .await
            {
                Ok(_) => {
                    info!("Member kicked successfully - will receive update via WebSocket");
                }
                Err(e) => {
                    error!("Failed to kick member: {}", e);
                }
            }
        });
    }

    pub fn delete_message(&mut self, message_id: Uuid) {
        if self.ws_status == WsStatus::Connected {
            self.send_via_websocket(Outgoing::DeleteMessage { mid: message_id });
            info!(
                "Sent delete request for message {} via WebSocket",
                message_id
            );
        } else {
            error!("Cannot delete message, WebSocket is not connected.");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
        }
    }

    // UI Message handling
    pub fn set_auth_message(&mut self, msg: String, is_error: bool) {
        self.auth_message = Some(msg);
        self.auth_message_is_error = is_error;
    }

    pub fn clear_auth_message(&mut self) {
        self.auth_message = None;
        self.auth_message_is_error = false;
    }

    pub fn set_message_info(&mut self, msg: String) {
        if self.token.is_none() {
            self.set_auth_message(msg, false);
        } else {
            self.push_toast(ToastKind::Info, msg);
        }
    }

    pub fn set_message_error(&mut self, msg: String) {
        if self.token.is_none() {
            self.set_auth_message(msg, true);
        } else {
            self.push_toast(ToastKind::Error, msg);
        }
    }

    // === Toast helpers ===
    pub fn push_toast(&mut self, kind: ToastKind, message: String) {
        if matches!(self.page, Page::Auth) || self.token.is_none() {
            return;
        }
        self.toasts.push(Toast {
            id: Uuid::new_v4(),
            message,
            kind,
            created: Instant::now(),
        });
        if self.toasts.len() > 5 {
            self.toasts.drain(0..self.toasts.len() - 5);
        }
    }

    /// Helper per creare un gruppo con partecipanti
    pub fn create_group_with_participants(&mut self) {
        use crate::models::{ConversationDto, MessageDto, Outgoing, Page};
        use uuid::Uuid;

        // CHECK CONNESSIONE: Blocca subito se non connesso
        if self.ws_status != WsStatus::Connected {
            warn!("Cannot create group: WebSocket not connected");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            return;
        }

        let group_name = self.create_group_popup.group_name.trim().to_string();
        let participants: Vec<String> = self
            .create_group_popup
            .selected_participants
            .iter()
            .cloned()
            .collect();

        info!(
            "Creating group '{}' with {} participants via WebSocket: {:?}",
            group_name,
            participants.len(),
            participants
        );

        // Crea stub per il gruppo
        let stub_id = Uuid::new_v4();

        let stub_conversation = ConversationDto {
            id: stub_id,
            kind: "group".to_string(),
            title: group_name.clone(),
            owner_id: self.user_id.unwrap_or(Uuid::nil()),
            created_at: chrono::Utc::now().timestamp(),
            last_read_sequence: 0,
            last_activity: chrono::Utc::now().timestamp(),
            last_msg_seq: 0,
        };

        // Aggiungi stub alla lista conversazioni
        if let Some(ref mut convs) = self.conversations {
            convs.insert(0, stub_conversation);
        }

        // Traccia lo stub CON TIMESTAMP
        self.group_stubs.insert(stub_id, (group_name.clone(), Instant::now()));

        // Apri il gruppo stub
        self.cid = Some(stub_id);
        self.page = Page::Chat;
        self.conv_title = group_name.clone();

        // Invia al server
        let outgoing = Outgoing::CreateGroupWithParticipants {
            group_name,
            participant_usernames: participants,
            client_temp_id: Some(stub_id.to_string()),
        };

        if let Err(_e) = self.ui_to_net_tx.try_send(outgoing) {
            // Cleanup in caso di errore
            if let Some(ref mut convs) = self.conversations {
                convs.retain(|c| c.id != stub_id);
            }
            self.group_stubs.remove(&stub_id);
            self.conversation_messages.remove(&stub_id);
            self.messages.clear();
            self.cid = None;
            self.page = Page::Conversations;

            // Notifica l'utente dell'errore
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::GroupCreate));

            return;
        }

        info!("Created group stub {} and opened it", stub_id);
        self.create_group_popup.reset();
    }

    /// Crea un DM stub per iniziare una nuova chat privata
    pub fn create_dm_stub(&mut self, target_username: String) -> Option<Uuid> {
        // CHECK CONNESSIONE: Blocca subito se non connesso
        if self.ws_status != WsStatus::Connected {
            warn!("Cannot create DM: WebSocket not connected");
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
            return None;
        }

        let stub_id = Uuid::new_v4();

        info!(
            "Creating DM stub for {} with temp ID: {}",
            target_username, stub_id
        );

        // Traccia lo stub CON TIMESTAMP
        self.dm_stubs.insert(stub_id, (target_username.clone(), Instant::now()));

        Some(stub_id)
    }

    pub fn prune_expired_toasts(&mut self, lifetime: Duration) {
        let now = Instant::now();
        self.toasts
            .retain(|t| now.duration_since(t.created) < lifetime);
    }

    // === Getter methods ===

    pub fn get_total_cached_messages(&self) -> usize {
        self.conversation_messages.values().map(|v| v.len()).sum()
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    pub fn get_debug_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        info.insert(
            "conversations".to_string(),
            self.conversations.as_ref().map_or(0, |c| c.len()).to_string(),
        );
        info.insert(
            "cached_messages".to_string(),
            self.get_total_cached_messages().to_string(),
        );
        info.insert("dm_stubs".to_string(), self.dm_stubs.len().to_string());
        info.insert("group_stubs".to_string(), self.group_stubs.len().to_string());
        info.insert("pending_user_check".to_string(), self.pending_user_check.is_some().to_string());
        info
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