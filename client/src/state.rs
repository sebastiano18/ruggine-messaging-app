use crate::net::ws::WsControl;
use crate::models::MessageDto;
use serde::{Deserialize, Serialize};
use tokio::{runtime::Runtime, sync::mpsc};
use uuid::Uuid;
use std::collections::HashMap;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationOut {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
}

#[derive(Debug)]
pub enum UiEvent {
    Info(String),
    Error(String),
    LoginStarted,
    RegisterStarted,
    Logged(String /* token */, Uuid /* user_id */),
    Opened(Uuid),
    WsConnected,
    WsDisconnected,
    WsControlReady(WsControl),
    WsError(String),
    WsIncoming(MessageDto), // Changed from String to MessageDto
    RefreshedMsgs(Vec<MessageDto>), // Changed from Vec<String> to Vec<MessageDto>
    ConversationsLoaded(Vec<ConversationOut>),
    LoggedOut,
    InviteCreated(String),

    // NUOVI EVENTI PER PRECARICAMENTO:
    AllMessagesLoaded(HashMap<Uuid, Vec<MessageDto>>), // Changed from Vec<String> to Vec<MessageDto>
    InitialLoadComplete,
    LoadingProgress(String),
    MessageSendFailed(Uuid),
}

pub struct AppState {
    pub rt: Runtime,
    pub base: String,
    pub username: String,
    pub password: String,
    pub token: Option<String>,
    pub user_id: Option<Uuid>,
    pub page: Page,

    pub cid: Option<Uuid>,
    pub conv_title: String,
    pub input: String,
    pub messages: Vec<MessageDto>, // Changed from Vec<String> to Vec<MessageDto>

    pub conversations: Option<Vec<ConversationOut>>,
    pub group_name: String,
    pub dm_user_username_input: String,
    pub last_invite_token: Option<String>,

    // NUOVI CAMPI PER GESTIONE INVITI:
    pub invite_conversation_id: String,
    pub last_created_invite: Option<String>,

    pub ws_status: WsStatus,
    pub request_ws_reconnect: bool,

    pub login_state: LoginState,

    pub ui_tx: mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<UiEvent>,

    pub ws_ctrl: Option<crate::net::ws::WsControl>,

    // NUOVI CAMPI PER PRECARICAMENTO:
    pub conversation_messages: HashMap<Uuid, Vec<MessageDto>>, // Changed from Vec<String> to Vec<MessageDto>
    pub is_initial_load_complete: bool,
    pub is_loading: bool,
}

impl AppState {
    pub fn new() -> Self {
        let rt = Runtime::new().expect("tokio runtime");
        let (tx, rx) = mpsc::unbounded_channel();

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

            login_state: LoginState::Idle,

            ui_tx: tx,
            ui_rx: rx,

            ws_ctrl: None,

            conversation_messages: HashMap::new(),
            is_initial_load_complete: false,
            is_loading: false,
        }
    }

    // Helper function to create system message
    fn create_system_message(&self, content: String) -> MessageDto {
        MessageDto {
            id: Uuid::new_v4(),
            author_id: Uuid::nil(), // Use nil UUID for system messages
            author_username: "system".to_string(),
            content,
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            match ev {
                UiEvent::Info(s) => {
                    self.messages.push(self.create_system_message(s));
                }
                UiEvent::Error(s) => {
                    self.messages.push(self.create_system_message(format!("⚠ {}", s)));
                    self.login_state = LoginState::Idle;
                    self.is_loading = false;
                }
                UiEvent::LoginStarted => {
                    self.login_state = LoginState::LoggingIn;
                    self.messages.push(self.create_system_message("🔄 Effettuando login...".into()));
                }
                UiEvent::RegisterStarted => {
                    self.login_state = LoginState::Registering;
                    self.messages.push(self.create_system_message("🔄 Registrando utente...".into()));
                }
                UiEvent::Logged(token, user_id) => {
                    self.token = Some(token.clone());
                    self.user_id = Some(user_id);
                    self.login_state = LoginState::LoggedIn;
                    self.messages.push(self.create_system_message("✅ Login effettuato con successo".into()));
                    self.page = Page::Conversations;

                    // Avvia precaricamento completo dopo il login
                    self.preload_all_data(token);
                }
                UiEvent::Opened(cid) => {
                    self.cid = Some(cid);
                    if let Some(ref conversations) = self.conversations {
                        if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                            self.conv_title = conv.title.clone();
                        }
                    }
                    self.page = Page::Chat;

                    // Carica messaggi dalla cache invece che dalla rete
                    if let Some(cached_messages) = self.conversation_messages.get(&cid) {
                        self.messages = cached_messages.clone();
                    } else if self.is_initial_load_complete {
                        // Se il caricamento iniziale è completo ma non abbiamo questi messaggi,
                        // probabilmente è una conversazione nuova - carica dalla rete
                        self.load_single_conversation_messages(cid);
                    } else {
                        // Se il caricamento iniziale non è completo, svuota i messaggi
                        self.messages.clear();
                    }
                }
                UiEvent::WsControlReady(ctrl) => {
                    self.ws_ctrl = Some(ctrl);
                }
                UiEvent::WsConnected => {
                    self.ws_status = WsStatus::Connected;
                    self.messages.push(self.create_system_message("🟢 WebSocket connesso".into()));
                }
                UiEvent::WsDisconnected => {
                    self.ws_status = WsStatus::Disconnected;
                    self.messages.push(self.create_system_message("🔴 WebSocket disconnesso".into()));
                }
                UiEvent::WsError(error) => {
                    self.ws_status = WsStatus::Disconnected;
                    self.messages.push(self.create_system_message(format!("⚠ WebSocket errore: {}", error)));
                }
                UiEvent::WsIncoming(msg) => {
                    // Aggiungi ai messaggi correnti se siamo nella chat
                    self.messages.push(msg.clone());

                    // Aggiungi anche alla cache se abbiamo una conversazione corrente
                    if let Some(cid) = self.cid {
                        self.conversation_messages
                            .entry(cid)
                            .or_insert_with(Vec::new)
                            .push(msg);
                    }
                }
                UiEvent::RefreshedMsgs(list) => {
                    self.messages = list.clone();
                    // Aggiorna anche la cache se abbiamo una conversazione corrente
                    if let Some(cid) = self.cid {
                        self.conversation_messages.insert(cid, list);
                    }
                }
                UiEvent::ConversationsLoaded(conversations) => {
                    self.conversations = Some(conversations);
                    self.messages.push(self.create_system_message("📋 Conversazioni caricate".into()));
                }
                // Gestione eventi di precaricamento
                UiEvent::AllMessagesLoaded(messages_map) => {
                    self.conversation_messages = messages_map;
                    // Se c'è una conversazione corrente, carica i suoi messaggi
                    if let Some(cid) = self.cid {
                        if let Some(msgs) = self.conversation_messages.get(&cid) {
                            self.messages = msgs.clone();
                        }
                    }
                }
                UiEvent::InitialLoadComplete => {
                    self.is_initial_load_complete = true;
                    self.is_loading = false;
                    self.messages.push(self.create_system_message("✅ Tutti i dati caricati!".into()));
                }
                UiEvent::LoadingProgress(progress) => {
                    self.messages.push(self.create_system_message(format!("📊 {}", progress)));
                }
                UiEvent::InviteCreated(token) => {
                    self.last_created_invite = Some(token.clone());
                    self.messages.push(self.create_system_message("🎉 Invito creato con successo".into()));
                }
                UiEvent::MessageSendFailed(failed_message_id) => {
                    // Remove the failed optimistic message from both current messages and cache
                    self.messages.retain(|msg| msg.id != failed_message_id);

                    if let Some(cid) = self.cid {
                        if let Some(messages) = self.conversation_messages.get_mut(&cid) {
                            messages.retain(|msg| msg.id != failed_message_id);
                        }
                    }
                }
                UiEvent::LoggedOut => {
                    self.token = None;
                    self.user_id = None;
                    self.page = Page::Auth;
                    self.login_state = LoginState::Idle;
                    self.password.clear();
                    self.cid = None;
                    self.conv_title.clear();
                    self.input.clear();
                    self.messages.clear();
                    self.conversations = None;
                    self.ws_status = WsStatus::Disconnected;
                    self.request_ws_reconnect = false;

                    // Pulizia campi inviti
                    self.invite_conversation_id.clear();
                    self.last_created_invite = None;

                    // Pulizia cache precaricamento
                    self.conversation_messages.clear();
                    self.is_initial_load_complete = false;
                    self.is_loading = false;

                    self.messages.push(self.create_system_message("👋 Logout effettuato".into()));
                }
            }
        }
    }

    // Precarica tutti i dati all'avvio
    fn preload_all_data(&mut self, token: String) {
        self.is_loading = true;

        let base = self.base.clone();
        let tx = self.ui_tx.clone();

        self.rt.spawn(async move {
            let _ = tx.send(UiEvent::LoadingProgress("Caricamento conversazioni...".into()));

            // 1. Carica tutte le conversazioni
            let conversations = match crate::net::conversation::get_conversations(&base, &token).await {
                Ok(convs) => {
                    let _ = tx.send(UiEvent::ConversationsLoaded(convs.clone()));
                    convs
                }
                Err(e) => {
                    let _ = tx.send(UiEvent::Error(format!("Caricamento conversazioni fallito: {}", e)));
                    return;
                }
            };

            // 2. Per ogni conversazione, carica i messaggi
            let total_conversations = conversations.len();
            let mut all_messages = HashMap::new();

            for (i, conv) in conversations.iter().enumerate() {
                let _ = tx.send(UiEvent::LoadingProgress(
                    format!("Caricamento messaggi ({}/{}): {}", i + 1, total_conversations, conv.title)
                ));

                match crate::net::chat::get_messages(&base, &token, conv.id).await {
                    Ok(messages) => {
                        all_messages.insert(conv.id, messages);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load messages for conversation {}: {}", conv.id, e);
                        // Continua con le altre conversazioni invece di fallire tutto
                        let error_msg = MessageDto {
                            id: Uuid::new_v4(),
                            author_id: Uuid::nil(),
                            author_username: "system".to_string(),
                            content: format!("⚠️ Errore nel caricamento messaggi: {}", e),
                            created_at: chrono::Utc::now().timestamp(),
                        };
                        all_messages.insert(conv.id, vec![error_msg]);
                    }
                }
            }

            // 3. Invia tutti i messaggi in una volta
            let _ = tx.send(UiEvent::AllMessagesLoaded(all_messages));
            let _ = tx.send(UiEvent::InitialLoadComplete);
        });
    }

    // Carica messaggi di una singola conversazione (per conversazioni nuove)
    fn load_single_conversation_messages(&self, cid: Uuid) {
        if let Some(ref token) = self.token {
            let base = self.base.clone();
            let token = token.clone();
            let tx = self.ui_tx.clone();

            self.rt.spawn(async move {
                match crate::net::chat::get_messages(&base, &token, cid).await {
                    Ok(messages) => {
                        let _ = tx.send(UiEvent::RefreshedMsgs(messages));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!("Caricamento messaggi fallito: {}", e)));
                    }
                }
            });
        }
    }

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

    pub fn current_conversation_info(&self) -> Option<&ConversationOut> {
        if let (Some(cid), Some(ref conversations)) = (self.cid, &self.conversations) {
            conversations.iter().find(|c| c.id == cid)
        } else {
            None
        }
    }
}