use crate::net::ws::WsControl;
use serde::{Deserialize, Serialize};
use tokio::{runtime::Runtime, sync::mpsc};
use uuid::Uuid;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConversationOut {
    pub id: Uuid, // <- UUID nativo
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
    Opened(Uuid), // <- UUID nativo per conversation id
    WsConnected,
    WsDisconnected,
    WsControlReady(WsControl),
    WsError(String),
    WsIncoming(String),
    RefreshedMsgs(Vec<String>),
    ConversationsLoaded(Vec<ConversationOut>),
    LoggedOut,
    InviteCreated(String), // <- NUOVO: Token di invito creato
}

pub struct AppState {
    pub rt: Runtime,
    pub base: String,
    pub username: String,
    pub password: String,
    pub token: Option<String>,
    pub user_id: Option<Uuid>,
    pub page: Page,

    pub cid: Option<Uuid>, // <- conversation id corrente (UUID)
    pub conv_title: String,
    pub input: String,
    pub messages: Vec<String>,

    pub conversations: Option<Vec<ConversationOut>>,
    pub group_name: String,
    pub dm_user_username_input: String, // input testuale per UUID DM target
    pub last_invite_token: Option<String>,

    // NUOVI CAMPI PER GESTIONE INVITI:
    pub invite_conversation_id: String, // Input manuale UUID per creare invito
    pub last_created_invite: Option<String>, // Ultimo token di invito creato

    pub ws_status: WsStatus,
    pub request_ws_reconnect: bool,

    pub login_state: LoginState,

    pub ui_tx: mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<UiEvent>,

    pub ws_ctrl: Option<crate::net::ws::WsControl>,
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

            // INIZIALIZZAZIONE NUOVI CAMPI:
            invite_conversation_id: String::new(),
            last_created_invite: None,

            ws_status: WsStatus::Disconnected,
            request_ws_reconnect: false,

            login_state: LoginState::Idle,

            ui_tx: tx,
            ui_rx: rx,

            ws_ctrl: None,
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            match ev {
                UiEvent::Info(s) => self.messages.push(format!("ℹ️ {}", s)),
                UiEvent::Error(s) => {
                    self.messages.push(format!("❌ {}", s));
                    self.login_state = LoginState::Idle;
                }
                UiEvent::LoginStarted => {
                    self.login_state = LoginState::LoggingIn;
                    self.messages.push("🔄 Effettuando login...".into());
                }
                UiEvent::RegisterStarted => {
                    self.login_state = LoginState::Registering;
                    self.messages.push("🔄 Registrando utente...".into());
                }
                UiEvent::Logged(token, user_id) => {
                    self.token = Some(token);
                    self.user_id = Some(user_id);
                    self.login_state = LoginState::LoggedIn;
                    self.messages
                        .push("✅ Login effettuato con successo".into());
                    self.page = Page::Conversations;

                    // Auto-carica le conversazioni dopo il login
                    self.auto_load_conversations();
                }
                UiEvent::Opened(cid) => {
                    self.cid = Some(cid);
                    if let Some(ref conversations) = self.conversations {
                        if let Some(conv) = conversations.iter().find(|c| c.id == cid) {
                            self.conv_title = conv.title.clone();
                        }
                    }
                    self.page = Page::Chat;
                    self.messages.clear();
                }
                UiEvent::WsControlReady(ctrl) => {
                    self.ws_ctrl = Some(ctrl);
                }
                UiEvent::WsConnected => {
                    self.ws_status = WsStatus::Connected;
                    self.messages.push("🟢 WebSocket connesso".into());
                }
                UiEvent::WsDisconnected => {
                    self.ws_status = WsStatus::Disconnected;
                    self.messages.push("🔴 WebSocket disconnesso".into());
                }
                UiEvent::WsError(error) => {
                    self.ws_status = WsStatus::Disconnected;
                    self.messages
                        .push(format!("❌ WebSocket errore: {}", error));
                }
                UiEvent::WsIncoming(msg) => {
                    // Messaggio WS in JSON: { "author_id": "<uuid>", "author_username": "alice", "content": "..." , ... }
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg) {
                        let content = json.get("content").and_then(|c| c.as_str()).unwrap_or(&msg);
                        
                        let author_name = json
                            .get("author_username") 
                            .and_then(|a| a.as_str())
                            .unwrap_or("anon");

                        self.messages.push(format!("[{}] {}", author_name, content));
                    } else {
                        self.messages.push(format!("WS <- {}", msg));
                    }
                }
                UiEvent::RefreshedMsgs(list) => {
                    self.messages = list;
                }
                UiEvent::ConversationsLoaded(conversations) => {
                    self.conversations = Some(conversations);
                    self.messages.push("📋 Conversazioni caricate".into());
                }
                // NUOVO: Gestione invito creato
                UiEvent::InviteCreated(token) => {
                    self.last_created_invite = Some(token.clone());
                    self.messages.push(format!("🎉 Invito creato con successo"));
                    // Mostra una notifica più user-friendly invece di mostrare direttamente il token
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

                    self.messages.push("👋 Logout effettuato".into());
                }
            }
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

    // NUOVO: Metodo helper per auto-caricare le conversazioni
    fn auto_load_conversations(&self) {
        if let Some(ref token) = self.token {
            let base = self.base.clone();
            let token_clone = token.clone();
            let tx = self.ui_tx.clone();

            self.rt.spawn(async move {
                match crate::net::conversation::get_conversations(&base, &token_clone).await {
                    Ok(conversations) => {
                        let _ = tx.send(UiEvent::ConversationsLoaded(conversations));
                    }
                    Err(e) => {
                        let _ = tx.send(UiEvent::Error(format!(
                            "Auto-caricamento conversazioni fallito: {}",
                            e
                        )));
                    }
                }
            });
        }
    }

    // NUOVO: Metodo helper per ottenere l'UUID della conversazione corrente come stringa
    pub fn current_conversation_id_string(&self) -> Option<String> {
        self.cid.map(|id| id.to_string())
    }

    // NUOVO: Metodo helper per verificare se c'è una conversazione aperta
    pub fn has_current_conversation(&self) -> bool {
        self.cid.is_some()
    }

    // NUOVO: Metodo helper per ottenere informazioni sulla conversazione corrente
    pub fn current_conversation_info(&self) -> Option<&ConversationOut> {
        if let (Some(cid), Some(ref conversations)) = (self.cid, &self.conversations) {
            conversations.iter().find(|c| c.id == cid)
        } else {
            None
        }
    }
}
