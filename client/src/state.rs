use tokio::{runtime::Runtime, sync::mpsc};

#[derive(Debug, Clone)]
#[derive(PartialEq)]
pub enum Page { Auth, Groups, Chat }

#[derive(Debug)]
pub enum UiEvent {
    Info(String),
    Error(String),
    Logged(String),      // token
    Opened(i64),         // conversation id
    WsConnected,
    WsIncoming(String),
    RefreshedMsgs(Vec<String>), // semplificato (stringhe)
}

pub struct AppState {
    pub rt: Runtime,
    pub base: String,
    pub username: String,
    pub password: String,
    pub token: Option<String>,
    pub page: Page,

    pub cid: Option<i64>,
    pub conv_title: String,
    pub input: String,
    pub messages: Vec<String>,

    pub group_name: String,
    pub last_invite_token: Option<String>,

    pub ui_tx: mpsc::UnboundedSender<UiEvent>,
    pub ui_rx: mpsc::UnboundedReceiver<UiEvent>,
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
            page: Page::Auth,

            cid: None,
            conv_title: String::new(),
            input: String::new(),
            messages: vec![],

            group_name: String::new(),
            last_invite_token: None,

            ui_tx: tx,
            ui_rx: rx,
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(ev) = self.ui_rx.try_recv() {
            match ev {
                UiEvent::Info(s) => self.messages.push(format!("ℹ️ {}", s)),
                UiEvent::Error(s) => self.messages.push(format!("❌ {}", s)),
                UiEvent::Logged(t) => { self.token = Some(t); self.messages.push("Login OK".into()); self.page = Page::Groups; }
                UiEvent::Opened(cid) => { self.cid = Some(cid); self.page = Page::Chat; }
                UiEvent::WsConnected => self.messages.push("WS connesso".into()),
                UiEvent::WsIncoming(t) => self.messages.push(format!("WS <- {t}")),
                UiEvent::RefreshedMsgs(list) => { self.messages = list; }
            }
        }
    }
}
