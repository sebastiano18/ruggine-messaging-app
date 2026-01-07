# State Management - Rust Ruggine Chat Client

## Panoramica

Lo state management del client Rust Ruggine è centralizzato in una singola struttura **`AppState`** che rappresenta l'intero stato dell'applicazione. Questo approccio garantisce una **single source of truth** e semplifica il reasoning sul flusso dei dati.

## Architettura Modulare

### Struttura Directory

```
state/
├── mod.rs          # Re-exports pubblici
├── core.rs         # Definizioni struct e costruttore
├── commands.rs     # Azioni utente e comandi server
├── helpers.rs      # Getter e utility functions
└── ui.rs           # Gestione stato UI (toast, modal)
```

### Re-export Pattern (mod.rs)

```rust
// Modulo principale con definizione AppState
pub mod core;

// Moduli separati per responsabilità
pub mod commands;
pub mod ui;
pub mod helpers;

// Re-export pubblico - IL CODICE ESISTENTE NON SI ROMPE
pub use core::{
    AppState,
    PendingDeletion,
    ToastKind,
    Toast,
    STUB_TIMEOUT,
    USER_CHECK_TIMEOUT
};
```

**Vantaggi**:
- Nessuna breaking change: `use crate::state::AppState` continua a funzionare
- Separazione chiara delle responsabilità
- File più piccoli e maintainable (< 500 righe ciascuno)
- Facile navigazione e testing

## Architettura State

### Single Source of Truth

**Principio**: `AppState` è l'unica fonte di verità per l'intera applicazione

**Vantaggi**:
- Predictable state updates
- Easy debugging (inspect single struct)
- No synchronization issues
- Clear data flow (events → handlers → AppState → UI)

**Pattern**:
```
User Action / Network Event
    ↓
UiEvent inviato via channel
    ↓
EventDispatcher (in app/events.rs)
    ↓
Handler muta AppState
    ↓
UI re-renders basandosi su AppState
```

## Struttura AppState (core.rs)

### Definizione Completa

```rust
pub struct AppState {
    // === RUNTIME & NETWORKING ===
    pub rt: Runtime,                    // Tokio async runtime
    pub base: String,                   // Server URL
    pub egui_waker: Arc<dyn Fn() + Send + Sync>, // UI wake callback
    
    // === AUTENTICAZIONE ===
    pub username: String,
    pub password: String,
    pub password_confirm: String,       // Solo per registrazione
    pub token: Option<String>,
    pub user_id: Option<Uuid>,
    pub current_session_id: Option<Uuid>,
    pub login_state: LoginState,
    pub confirm_delete_account: bool,   // Flag conferma eliminazione account
    
    // === NAVIGAZIONE ===
    pub page: Page,                     // Auth | Conversations | Chat
    
    // === CONVERSAZIONE CORRENTE ===
    pub cid: Option<Uuid>,              // Conversation ID corrente
    pub conv_title: String,
    pub input: String,                  // Input field text
    pub messages: Vec<MessageDto>,      // Messaggi conversazione attiva
    
    // === LISTA CONVERSAZIONI ===
    pub conversations: Option<Vec<ConversationDto>>,
    pub conversation_messages: HashMap<Uuid, Vec<MessageDto>>,
    pub conversation_unread_counts: HashMap<Uuid, i64>,
    pub has_more_conversations: bool,
    pub next_cursor: Option<i64>,
    pub fetching_conversations: HashSet<Uuid>,
    pub request_conversations_refresh: bool,
    pub pending_conversations: HashMap<String, ConversationDto>,
    
    // === WEBSOCKET ===
    pub ws_status: WsStatus,            // Connected | Connecting | Disconnected
    pub ws_ctrl: Option<WsControl>,     // Control handle
    pub request_ws_reconnect: bool,     // Flag reconnect
    pub connection_attempt_start: Option<Instant>, // Timestamp inizio connessione
    
    // === CANALI COMUNICAZIONE ===
    pub ui_tx: UnboundedSender<UiEvent>,
    pub ui_rx: UnboundedReceiver<UiEvent>,
    pub ui_to_net_tx: Sender<Outgoing>,
    pub ui_to_net_rx: Receiver<Outgoing>,
    
    // === SINCRONIZZAZIONE SEQUENZE ===
    pub user_sequence_confirmed: u64,   // Ultima sequenza confermata dal server
    pub user_sequence_received: u64,    // Ultima sequenza ricevuta (locale)
    pub conversation_sequences: HashMap<Uuid, u64>,
    pub conversation_sequences_confirmed: HashMap<Uuid, u64>,
    pub user_sequence_shared: Arc<AtomicU64>, // Condivisa con WebSocket thread
    
    // === BUFFER RIORDINAMENTO ===
    pub message_reorder_buffer: BTreeMap<Uuid, BTreeMap<u64, MessageDto>>,
    pub user_event_reorder_buffer: BTreeMap<u64, Vec<serde_json::Value>>,
    
    // === STUB TRACKING ===
    pub dm_stubs: HashMap<Uuid, (String, Instant)>,     // (target_username, created_at)
    pub group_stubs: HashMap<Uuid, (String, Instant)>,  // (group_name, created_at)
    
    // === CONFERME MESSAGGI ===
    pub pending_confirmations: HashMap<String, MessageDto>, // client_msg_id -> MessageDto
    pub confirmation_timeout: Duration,  // Default: 10 secondi
    pub last_confirmation_cleanup: Instant,
    
    // === UI STATE ===
    pub show_account_modal: bool,
    pub show_create_group_modal: bool,
    pub show_invite_popup: bool,
    pub show_group_info_popup: bool,
    pub pending_deletion: Option<PendingDeletion>,
    pub pending_message_deletion: Option<Uuid>,
    pub pending_member_kick: Option<(Uuid, Uuid, String)>, // (cid, user_id, username)
    
    // === POPUP STATES ===
    pub create_group_popup: CreateGroupPopupState,
    pub invite_popup: InvitePopupState,
    
    // === TOAST NOTIFICATIONS ===
    pub toasts: Vec<Toast>,
    pub auth_message: Option<String>,    // Messaggio nella pagina auth
    pub auth_message_is_error: bool,     // true = errore, false = info
    
    // === LOADING STATES ===
    pub is_loading: bool,
    pub is_loading_more: bool,
    pub is_loading_more_conversations: bool,
    pub is_loading_members: bool,
    pub is_initial_load_complete: bool,
    pub has_more_messages: HashMap<Uuid, bool>,
    
    // === RECOVERY STATES ===
    pub is_recovering_user_events: bool,
    pub is_recovering_messages: HashMap<Uuid, bool>,
    pub pending_resume_requests: u32,
    
    // === PING/PONG ===
    pub last_ping_time: Instant,
    pub missed_pings: u32,
    pub max_missed_pings: u32,           // Default: 3
    
    // === STATISTICHE ===
    pub sequence_stats: SequenceStats,
    
    // === MEMBRI GRUPPI ===
    pub members_list: HashMap<Uuid, Vec<ParticipantInfo>>,
    
    // === USER VERIFICATION ===
    pub pending_user_check: Option<String>,      // Username in verifica
    pub user_check_request_id: Option<String>,   // ID richiesta per correlazione
    pub user_check_timestamp: Option<Instant>,   // Timestamp per timeout
    
    // === LEGACY/INVITI (da rifattorizzare) ===
    pub group_name: String,              // DEPRECATED: usa create_group_popup
    pub dm_user_username_input: String,  // DEPRECATED: usa pending_user_check
    pub last_invite_token: Option<String>,
    pub invite_conversation_id: String,
    pub last_created_invite: Option<String>,
    pub dm_username: String,             // Input temporaneo DM
}
```

### Costruttore (core.rs)

```rust
impl AppState {
    pub fn new(waker: Arc<dyn Fn() + Send + Sync>) -> Self {
        let rt = Runtime::new().expect("tokio runtime");
        let (tx, rx) = mpsc::unbounded_channel();
        let (ui_to_net_tx, ui_to_net_rx) = mpsc::channel::<Outgoing>(200);

        Self {
            rt,
            base: std::env::var("RUGGINE_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            username: std::env::var("RUGGINE_USER")
                .unwrap_or_else(|_| "alice".into()),
            password: std::env::var("RUGGINE_PASS")
                .unwrap_or_else(|_| "password".into()),
            password_confirm: String::new(),
            token: None,
            user_id: None,
            page: Page::Auth,
            
            // ... (inizializzazione di tutti i campi con valori di default)
            
            egui_waker: waker,
            user_sequence_shared: Arc::new(AtomicU64::new(0)),
            confirmation_timeout: Duration::from_secs(10),
            max_missed_pings: 3,
            // ...
        }
    }
}
```

**Nota**: Environment variables per sviluppo:
- `RUGGINE_BASE` - Server URL (default: `http://127.0.0.1:8080`)
- `RUGGINE_USER` - Username default
- `RUGGINE_PASS` - Password default

## Struct Ausiliarie (core.rs)

### CreateGroupPopupState

```rust
#[derive(Default)]
pub struct CreateGroupPopupState {
    pub group_name: String,                      // Nome del gruppo
    pub manual_username_input: String,           // Input manuale username da aggiungere
    pub selected_participants: HashSet<String>,  // Username selezionati
    pub search_query: String,                    // Query ricerca partecipanti
    pub pending_user_verification: Option<String>, // Username in verifica esistenza
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
```

**Uso**: Gestisce lo stato del popup di creazione gruppo
**Location**: `state/core.rs`

### InvitePopupState

```rust
#[derive(Default)]
pub struct InvitePopupState {
    pub search_query: String,                    // Query ricerca utenti
    pub selected_users: HashSet<String>,         // Username selezionati
    pub pending_user_verification: Option<String>, // Username in verifica esistenza
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
```

**Uso**: Gestisce lo stato del popup invito membri
**Location**: `state/core.rs`

### PendingDeletion

```rust
#[derive(Debug, Clone)]
pub struct PendingDeletion {
    pub conversation: ConversationDto,
}
```

**Uso**: Mantiene riferimento alla conversazione in attesa di conferma eliminazione
**Location**: `state/core.rs`

### SequenceStats

```rust
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
```

**Uso**: Statistiche debug per monitoraggio sequenze e recovery
**Location**: `state/core.rs`

### Toast

```rust
#[derive(Debug, Clone)]
pub struct Toast {
    pub id: Uuid,
    pub message: String,
    pub kind: ToastKind,           // Info | Error
    pub created: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}
```

**Uso**: Notifiche temporanee in-app
**Location**: `state/core.rs`

## Costanti (core.rs)

```rust
pub const STUB_TIMEOUT: Duration = Duration::from_secs(30);
pub const USER_CHECK_TIMEOUT: Duration = Duration::from_secs(10);
```

**`STUB_TIMEOUT`**:
- Timeout per stub DM e gruppi
- Dopo 30 secondi senza conferma dal server, lo stub viene rimosso
- Previene stub orfani in caso di errori di rete

**`USER_CHECK_TIMEOUT`**:
- Timeout per verifica esistenza utente
- Dopo 10 secondi senza risposta, la verifica fallisce
- Previene attese infinite su rete lenta

## Metodi: Commands (commands.rs)

### User Verification

#### `request_dm_creation`

```rust
pub fn request_dm_creation(&mut self, target_username: String)
```

**Scopo**: Richiede la creazione di un DM verificando prima che l'utente esista

**Flusso**:
1. ✅ **CHECK connessione**: Verifica `ws_status == WsStatus::Connected`
2. ✅ **Validazione input**: Username non vuoto, non se stesso
3. ✅ **CHECK duplicati**: Controlla se esiste già una conversazione
4. ✅ **CHECK pending**: Verifica se c'è già un check in corso
5. 🚀 **Genera request_id**: Uuid per correlare richiesta/risposta
6. 📝 **Salva pending state**: `pending_user_check`, `user_check_request_id`, `user_check_timestamp`
7. 📡 **Invia via WebSocket**: `Outgoing::CheckUser`

**Guardrails**:
- Blocca se non connesso → `UiEvent::Error(ErrorType::Connection)`
- Blocca se username vuoto → errore generico
- Blocca se username == self.username → errore
- Blocca se conversazione già esistente → errore
- Blocca se check già in corso → return silenzioso

**Example**:
```rust
state.request_dm_creation("bob".to_string());
// → Invia CheckUser request
// → Attende UserCheckResult event
// → Se OK: apre UI temporanea con cid temporaneo
// → Primo messaggio crea effettivamente il DM
```

### Conversation Deletion

#### `request_delete_confirmation`

```rust
pub fn request_delete_confirmation(&mut self, conversation: &ConversationDto)
```

**Scopo**: Salva la conversazione in `pending_deletion` per conferma utente

**Flusso**:
```rust
self.pending_deletion = Some(PendingDeletion {
    conversation: conversation.clone(),
});
```

#### `cancel_delete_confirmation`

```rust
pub fn cancel_delete_confirmation(&mut self)
```

**Scopo**: Annulla l'eliminazione cancellando il pending state

**Flusso**:
```rust
self.pending_deletion = None;
```

#### `execute_pending_deletion`

```rust
pub fn execute_pending_deletion(&mut self)
```

**Scopo**: Esegue l'eliminazione effettiva della conversazione

**Flusso**:
1. Prende `pending_deletion` e lo consuma
2. **Se è uno stub DM**:
   - Rimuove da `dm_stubs`
   - Rimuove dalla lista conversazioni
   - Rimuove messaggi dalla cache
   - Se era la conversazione attiva → torna a Conversations
3. **Se è conversazione reale**:
   - **Se gruppo + non owner** → `Outgoing::LeaveGroup`
   - **Altrimenti** → `Outgoing::DeleteConversation`

**Logica Owner/Member**:
```rust
let is_owner = self.user_id.map_or(false, |uid| uid == conversation.owner_id);

if conversation.kind == "group" && !is_owner {
    // Partecipante che esce
    Outgoing::LeaveGroup { cid }
} else {
    // Owner che elimina o eliminazione DM
    Outgoing::DeleteConversation { cid }
}
```

### WebSocket Communication

#### `send_via_websocket`

```rust
pub fn send_via_websocket(&self, outgoing: Outgoing)
```

**Scopo**: Invia un messaggio via WebSocket con error handling automatico

**Error Mapping**:
```rust
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
    Outgoing::CheckUser { .. } => ErrorType::Connection,
    _ => return, // Silenzioso per Ping, Typing, etc.
};
```

**Flusso**:
1. Determina tipo di errore basato sul messaggio
2. Tenta invio via `ui_to_net_tx.try_send()`
3. Se fallisce → invia `UiEvent::Error(error_type)`

#### `send_chat_message_ws`

```rust
pub fn send_chat_message_ws(&self, content: String, client_msg_id: Option<String>)
```

**Scopo**: Invia un messaggio di chat nella conversazione corrente

**Logica Stub DM**:
```rust
let target_username = self.dm_stubs.get(&cid).map(|(username, _)| username.clone());

self.send_via_websocket(Outgoing::ChatMessage {
    cid,
    content,
    target_username,        // Some("bob") se è uno stub DM
    target_usernames: None,
    client_msg_id,
});
```

**Importante**: Il primo messaggio a uno stub DM include `target_username` che triggera la creazione effettiva del DM sul server

#### `send_invite_users`

```rust
pub fn send_invite_users(&self, cid: Uuid, usernames: Vec<String>)
```

**Scopo**: Invita utenti a un gruppo

```rust
self.send_via_websocket(Outgoing::InviteUser { cid, usernames });
```

### Message Management

#### `delete_message`

```rust
pub fn delete_message(&mut self, message_id: Uuid)
```

**Scopo**: Richiede eliminazione di un messaggio

**Flusso**:
1. ✅ **CHECK connessione**: `ws_status == WsStatus::Connected`
2. 📡 **Invia request**: `Outgoing::DeleteMessage { mid: message_id }`
3. ❌ **Se non connesso**: Errore `ErrorType::Connection`

### Group Management

#### `create_group_with_participants`

```rust
pub fn create_group_with_participants(&mut self)
```

**Scopo**: Crea un nuovo gruppo con partecipanti selezionati

**Flusso Completo**:

1. **CHECK connessione**:
```rust
if self.ws_status != WsStatus::Connected {
    let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
    return;
}
```

2. **Prepara dati**:
```rust
let group_name = self.create_group_popup.group_name.trim().to_string();
let participants: Vec<String> = self.create_group_popup
    .selected_participants.iter().cloned().collect();
```

3. **Crea stub**:
```rust
let stub_id = Uuid::new_v4();
let now = chrono::Utc::now().timestamp();

let stub_conversation = ConversationDto {
    id: stub_id,
    kind: "group".to_string(),
    title: group_name.clone(),
    owner_id: self.user_id.unwrap_or(Uuid::nil()),
    created_at: now,
    last_read_sequence: 0,
    last_activity: now,
    last_msg_seq: 0,
};
```

4. **Traccia stub**:
```rust
self.group_stubs.insert(stub_id, (group_name.clone(), Instant::now()));
```

5. **Crea members_list per lo stub**:
```rust
let mut stub_members = Vec::new();

// Owner (utente corrente)
stub_members.push(ParticipantInfo {
    user_id: self.user_id.unwrap(),
    username: self.username.clone(),
    role: "owner".to_string(),
    joined_at: Some(now),
});

// Partecipanti selezionati
for username in &participants {
    stub_members.push(ParticipantInfo {
        user_id: Uuid::nil(), // Placeholder
        username: username.clone(),
        role: "member".to_string(),
        joined_at: Some(now),
    });
}

self.members_list.insert(stub_id, stub_members);
```

6. **Apri gruppo stub**:
```rust
self.cid = Some(stub_id);
self.page = Page::Chat;
self.conv_title = group_name.clone();
```

7. **Invia al server**:
```rust
let outgoing = Outgoing::CreateGroupWithParticipants {
    group_name,
    participant_usernames: participants,
    client_temp_id: Some(stub_id.to_string()),
};

if let Err(_) = self.ui_to_net_tx.try_send(outgoing) {
    // CLEANUP in caso di errore
    if let Some(ref mut convs) = self.conversations {
        convs.retain(|c| c.id != stub_id);
    }
    self.group_stubs.remove(&stub_id);
    self.conversation_messages.remove(&stub_id);
    self.members_list.remove(&stub_id);
    self.messages.clear();
    self.cid = None;
    self.page = Page::Conversations;
    
    let _ = self.ui_tx.send(UiEvent::Error(ErrorType::GroupCreate));
    return;
}
```

8. **Reset popup**:
```rust
self.create_group_popup.reset();
```

**Guardrails**:
- ✅ CHECK connessione all'inizio
- ✅ Cleanup completo se invio fallisce
- ✅ Stub timeout (30s) se server non risponde
- ✅ Toast notification per errori

#### `create_dm_stub`

```rust
pub fn create_dm_stub(&mut self, target_username: String) -> Option<Uuid>
```

**Scopo**: Crea uno stub per iniziare una nuova chat privata

**Flusso**:
1. ✅ **CHECK connessione**: `ws_status == WsStatus::Connected`
2. 🆔 **Genera stub_id**: `Uuid::new_v4()`
3. 📝 **Traccia stub**: `dm_stubs.insert(stub_id, (username, Instant::now()))`
4. ✅ **Ritorna ID**: `Some(stub_id)`

**Importante**: 
- Lo stub NON viene aggiunto alla lista conversazioni qui
- La conversazione viene aperta dalla UI che chiama questo metodo
- Il primo messaggio crea effettivamente il DM sul server

### Message Loading

#### `load_older_messages`

```rust
pub fn load_older_messages(&mut self)
```

**Scopo**: Carica messaggi più vecchi per la conversazione corrente (paginazione)

**Flusso**:

1. **Guards**:
```rust
let Some(cid) = self.cid else { return };
let Some(ref token) = self.token else { return };

if self.is_loading_more { return; }  // Evita caricamenti simultanei

if !*self.has_more_messages.get(&cid).unwrap_or(&true) { return; }
```

2. **Determina punto di partenza**:
```rust
let before_seq = self.messages
    .first()
    .and_then(|m| m.sequence_num)
    .map(|seq| seq as i64);

// Se il primo messaggio ha sequence 1, siamo all'inizio
if let Some(seq) = before_seq {
    if seq <= 1 {
        self.has_more_messages.insert(cid, false);
        return;
    }
}
```

3. **Marca loading**:
```rust
self.is_loading_more = true;
```

4. **Spawn async task**:
```rust
self.rt.spawn(async move {
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    
    match crate::api::chat::get_messages_paginated(
        &base, &token, cid, Some(30), before_seq
    ).await {
        Ok(messages) => {
            let _ = tx.send(UiEvent::OlderMessagesLoaded(messages));
            waker();
        }
        Err(e) => {
            let _ = tx.send(UiEvent::LoadingError);
            waker();
        }
    }
});
```

**Parametri API**:
- `limit`: 30 messaggi per batch
- `before_seq`: Sequence number del primo messaggio corrente

**Event Handling**: L'handler di `UiEvent::OlderMessagesLoaded` (in `app/events.rs`) gestirà:
- Reset di `is_loading_more = false`
- Inserimento messaggi all'inizio di `self.messages`
- Update di `has_more_messages` se arrivano meno di 30 messaggi

## Metodi: Helpers (helpers.rs)

### Authentication

#### `is_authenticated`

```rust
pub fn is_authenticated(&self) -> bool
```

**Scopo**: Verifica se l'utente ha un token valido

```rust
self.token.is_some()
```

### Debug & Monitoring

#### `get_total_cached_messages`

```rust
pub fn get_total_cached_messages(&self) -> usize
```

**Scopo**: Conta il numero totale di messaggi in cache

```rust
self.conversation_messages.values().map(|v| v.len()).sum()
```

#### `get_debug_info`

```rust
pub fn get_debug_info(&self) -> HashMap<String, String>
```

**Scopo**: Ottiene snapshot dello stato per debugging

**Output Example**:
```rust
{
    "conversations": "15",
    "cached_messages": "1240",
    "dm_stubs": "2",
    "group_stubs": "1",
    "pending_user_check": "false",
}
```

### Event Processing

#### `drain_events`

```rust
pub fn drain_events(&mut self)
```

**Scopo**: Processa tutti gli eventi in coda dal channel

**Flusso**:
1. **Drena channel**:
```rust
while let Ok(ev) = self.ui_rx.try_recv() {
    crate::app::events::EventDispatcher::handle_event(self, ev);
}
```

2. **Cleanup periodico** (ogni 5 secondi):
```rust
if self.last_confirmation_cleanup.elapsed() > Duration::from_secs(5) {
    self.cleanup_pending_confirmations();
    self.cleanup_pending_user_check();
    self.last_confirmation_cleanup = Instant::now();
}
```

**Importante**: Chiamato a ogni frame da `app.rs::update()`

### User Verification

#### `is_checking_user`

```rust
pub fn is_checking_user(&self) -> bool
```

**Scopo**: Verifica se un check utente è in corso

```rust
self.pending_user_check.is_some()
```

#### `handle_user_check_result`

```rust
pub fn handle_user_check_result(
    &mut self,
    username: String,
    exists: bool,
    _user_id: Option<Uuid>,
    request_id: String,
)
```

**Scopo**: Callback quando riceviamo la risposta del check utente

**Flusso**:

1. **Verifica correlazione**:
```rust
if self.user_check_request_id.as_ref() != Some(&request_id) {
    warn!("Received stale user check response");
    return;
}
```

2. **Pulisci stato pending**:
```rust
self.pending_user_check = None;
self.user_check_request_id = None;
self.user_check_timestamp = None;
```

3. **Gestione per popup creazione gruppo**:
```rust
if let Some(pending) = &self.create_group_popup.pending_user_verification {
    if pending.to_lowercase() == username.to_lowercase() {
        if exists {
            self.create_group_popup.selected_participants.insert(username.clone());
        } else {
            let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
                format!("Utente '{}' non trovato", username)
            )));
        }
        self.create_group_popup.pending_user_verification = None;
        return;
    }
}
```

4. **Gestione per popup invito membri**:
```rust
if let Some(pending) = &self.invite_popup.pending_user_verification {
    if pending.to_lowercase() == username.to_lowercase() {
        if exists {
            self.invite_popup.selected_users.insert(username.clone());
        } else {
            let _ = self.ui_tx.send(UiEvent::Error(...));
        }
        self.invite_popup.pending_user_verification = None;
        return;
    }
}
```

5. **Gestione per creazione DM** (se non era per i popup):
```rust
if exists {
    let temp_cid = Uuid::new_v4();
    self.cid = Some(temp_cid);
    self.page = Page::Chat;
    self.conv_title = username.clone();
    self.messages.clear();
} else {
    let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
        format!("Utente '{}' non esiste", username)
    )));
}
```

**Importante**: 
- Supporta 3 use case: creazione gruppo, invito membri, creazione DM
- Usa `pending_user_verification` nei popup per distinguere i contesti
- Per DM crea un `temp_cid` temporaneo (NON uno stub tracciato)

### Stub Management

#### `add_dm_stub`

```rust
pub fn add_dm_stub(&mut self, stub_id: Uuid, target_username: String)
```

**Scopo**: Aggiunge un DM stub alla tracking map

```rust
self.dm_stubs.insert(stub_id, (target_username.clone(), Instant::now()));
```

**Location**: `state/helpers.rs` (linea 74)

#### `remove_dm_stub`

```rust
pub fn remove_dm_stub(&mut self, conversation_id: Uuid)
```

**Scopo**: Rimuove un DM stub dalla tracking map

```rust
if let Some((target, _)) = self.dm_stubs.remove(&conversation_id) {
    debug!("Removed DM stub: {} -> {}", conversation_id, target);
}
```

#### `is_dm_stub`

```rust
pub fn is_dm_stub(&self, conversation_id: Uuid) -> bool
```

**Scopo**: Verifica se un ID è uno stub DM

```rust
self.dm_stubs.contains_key(&conversation_id)
```

#### `is_group_stub`

```rust
pub fn is_group_stub(&self, conversation_id: Uuid) -> bool
```

**Scopo**: Verifica se un ID è uno stub di gruppo

```rust
self.group_stubs.contains_key(&conversation_id)
```

### Cleanup Methods

#### `cleanup_old_data`

```rust
pub fn cleanup_old_data(&self)
```

**Scopo**: Monitora l'uso della memoria e logga warning se alto

```rust
let total_messages = self.get_total_cached_messages();
if total_messages > 50000 {
    warn!("High memory usage detected: {} cached messages", total_messages);
}
```

**Nota**: Attualmente solo monitoring, eviction non implementata

#### `cleanup_expired_stubs`

```rust
pub fn cleanup_expired_stubs(&mut self)
```

**Scopo**: Rimuove stub (DM e gruppi) scaduti dopo `STUB_TIMEOUT` (30s)

**Flusso per Group Stubs**:
```rust
let expired_groups: Vec<Uuid> = self.group_stubs
    .iter()
    .filter(|(_, (_, created))| now.duration_since(*created) > STUB_TIMEOUT)
    .map(|(id, _)| *id)
    .collect();

for stub_id in expired_groups {
    if let Some((name, _)) = self.group_stubs.remove(&stub_id) {
        // 1. Rimuovi dalla lista conversazioni
        if let Some(ref mut convs) = self.conversations {
            convs.retain(|c| c.id != stub_id);
        }
        
        // 2. Rimuovi messaggi cached
        self.conversation_messages.remove(&stub_id);
        
        // 3. Se era la conversazione attiva, torna alla lista
        if self.cid == Some(stub_id) {
            self.cid = None;
            self.page = Page::Conversations;
            self.messages.clear();
            self.conv_title.clear();
        }
        
        // 4. Notifica l'utente
        let _ = self.ui_tx.send(UiEvent::Error(ErrorType::GroupCreate));
    }
}
```

**Flusso per DM Stubs**: Identico, ma con `ErrorType::MessageSend`

**Chiamato da**: 
- `app.rs::periodic_cleanup()` solo se `!dm_stubs.is_empty() || !group_stubs.is_empty()`

#### `cleanup_pending_confirmations`

```rust
pub fn cleanup_pending_confirmations(&mut self)
```

**Scopo**: Pulisce conferme messaggi scadute (timeout: 10s)

**Flusso**:
1. **Identifica scaduti**:
```rust
let now = chrono::Utc::now().timestamp();
let timeout_secs = self.confirmation_timeout.as_secs() as i64;

let mut expired = Vec::new();
for (client_id, msg) in &self.pending_confirmations {
    if now - msg.created_at > timeout_secs {
        expired.push(client_id.clone());
    }
}
```

2. **Per ogni scaduto**:
```rust
for client_id in expired {
    if let Some(msg) = self.pending_confirmations.remove(&client_id) {
        // Notifica UI
        let _ = self.ui_tx.send(UiEvent::MessageSendFailed(msg.id));
        
        // Marca come fallito in cache
        if let Some(messages) = self.conversation_messages.get_mut(&msg.conversation_id) {
            for m in messages.iter_mut() {
                if m.id == msg.id {
                    m.is_confirmed = Some(false);
                    break;
                }
            }
        }
        
        // Marca come fallito nella lista corrente
        for m in self.messages.iter_mut() {
            if m.id == msg.id {
                m.is_confirmed = Some(false);
                break;
            }
        }
    }
}
```

**Chiamato da**: `drain_events()` ogni 5 secondi

#### `cleanup_pending_user_check`

```rust
pub fn cleanup_pending_user_check(&mut self)
```

**Scopo**: Pulisce verifiche utente in timeout (timeout: 10s)

**Flusso**:
```rust
if let Some(timestamp) = self.user_check_timestamp {
    if timestamp.elapsed() > USER_CHECK_TIMEOUT {
        // Mostra errore
        let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Generic(
            "Timeout verifica utente. Riprova.".to_string()
        )));
        
        // Pulisci stato
        self.pending_user_check = None;
        self.user_check_request_id = None;
        self.user_check_timestamp = None;
    }
}
```

**Chiamato da**: `drain_events()` ogni 5 secondi

## Metodi: UI (ui.rs)

### Auth Message Handling

#### `set_auth_message`

```rust
pub fn set_auth_message(&mut self, msg: String, is_error: bool)
```

**Scopo**: Imposta il messaggio mostrato nella pagina di autenticazione

```rust
self.auth_message = Some(msg);
self.auth_message_is_error = is_error;
```

#### `clear_auth_message`

```rust
pub fn clear_auth_message(&mut self)
```

**Scopo**: Cancella il messaggio di autenticazione

```rust
self.auth_message = None;
self.auth_message_is_error = false;
```

#### `set_message_info`

```rust
pub fn set_message_info(&mut self, msg: String)
```

**Scopo**: Mostra messaggio info (auth message se non autenticato, toast altrimenti)

```rust
if self.token.is_none() {
    self.set_auth_message(msg, false);
} else {
    self.push_toast(ToastKind::Info, msg);
}
```

#### `set_message_error`

```rust
pub fn set_message_error(&mut self, msg: String)
```

**Scopo**: Mostra messaggio errore (auth message se non autenticato, toast altrimenti)

```rust
if self.token.is_none() {
    self.set_auth_message(msg, true);
} else {
    self.push_toast(ToastKind::Error, msg);
}
```

### Toast Notifications

#### `push_toast`

```rust
pub fn push_toast(&mut self, kind: ToastKind, message: String)
```

**Scopo**: Aggiunge una notifica toast

**Flusso**:
```rust
// Guard: non mostrare toast in pagina auth o se non autenticato
if matches!(self.page, Page::Auth) || self.token.is_none() {
    return;
}

self.toasts.push(Toast {
    id: Uuid::new_v4(),
    message,
    kind,
    created: Instant::now(),
});

// Mantieni max 5 toast
if self.toasts.len() > 5 {
    self.toasts.drain(0..self.toasts.len() - 5);
}
```

#### `prune_expired_toasts`

```rust
pub fn prune_expired_toasts(&mut self, lifetime: Duration)
```

**Scopo**: Rimuove toast più vecchi del lifetime specificato

```rust
let now = Instant::now();
self.toasts.retain(|t| now.duration_since(t.created) < lifetime);
```

**Chiamato da**: `app.rs::update()` con `lifetime = 5 secondi`

## Integration: App.rs

### Lifecycle Management

```rust
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Gestione lifecycle WebSocket
        self.ws_manager.ensure_ws_lifecycle(&mut self.state);

        // 2. Processa eventi in coda (drain_events chiama EventDispatcher)
        self.state.drain_events();
        
        // 3. Rimuovi toast scaduti
        self.state.prune_expired_toasts(std::time::Duration::from_secs(5));

        // 4. Render UI...
        
        // 5. Cleanup periodico
        self.periodic_cleanup();
    }
}
```

### Periodic Cleanup

```rust
fn periodic_cleanup(&mut self) {
    static mut LAST_GENERAL_CLEANUP: Option<std::time::Instant> = None;

    // 1. Cleanup stub: SOLO se ci sono stub in attesa (check leggero)
    if !self.state.dm_stubs.is_empty() || !self.state.group_stubs.is_empty() {
        self.state.cleanup_expired_stubs();
    }

    // 2. Cleanup generale: ogni 5 minuti (300 secondi)
    let should_general_cleanup = unsafe {
        LAST_GENERAL_CLEANUP.map_or(true, |last| {
            last.elapsed() > std::time::Duration::from_secs(300)
        })
    };

    if should_general_cleanup {
        self.state.cleanup_old_data();
        
        unsafe {
            LAST_GENERAL_CLEANUP = Some(std::time::Instant::now());
        }
        
        tracing::debug!("Periodic cleanup completed");
    }
}
```

**Ottimizzazioni**:
- Stub cleanup: solo se necessario (check veloce)
- General cleanup: ogni 5 minuti (non ogni frame)

## Common Patterns

### Pattern 1: Check-Send-Handle

```rust
// 1. CHECK prerequisites
if self.ws_status != WsStatus::Connected {
    let _ = self.ui_tx.send(UiEvent::Error(ErrorType::Connection));
    return;
}

// 2. SEND request
self.send_via_websocket(Outgoing::ChatMessage { ... });

// 3. HANDLE in event processor (later)
// UiEvent::MessageConfirmed -> mark as confirmed
```

**Uso**: Tutti i comandi che richiedono WebSocket

### Pattern 2: Optimistic Update + Confirmation

```rust
// 1. Create optimistic state
let client_id = Uuid::new_v4().to_string();
let message = MessageDto {
    is_confirmed: None,  // Pending
    // ...
};

// 2. Update UI immediately
self.messages.push(message.clone());
self.pending_confirmations.insert(client_id.clone(), message);

// 3. Send to server
self.send_via_websocket(Outgoing::ChatMessage { 
    client_msg_id: Some(client_id), 
    ... 
});

// 4. Wait for confirmation (or timeout)
// UiEvent::MessageConfirmed -> is_confirmed = Some(true)
// Timeout (10s) -> is_confirmed = Some(false)
```

**Uso**: Invio messaggi

### Pattern 3: Stub + Replace

```rust
// 1. Create stub
let stub_id = Uuid::new_v4();
self.dm_stubs.insert(stub_id, (username, Instant::now()));

// 2. Update UI with stub
self.cid = Some(stub_id);
self.page = Page::Chat;

// 3. Send creation request (first message)
self.send_via_websocket(Outgoing::ChatMessage { 
    target_username: Some(username), 
    ... 
});

// 4. Server responds with real ID
// UiEvent::NewConversation(real_cid, ...) -> replace stub
// (handled in app/events.rs)
```

**Uso**: Creazione DM e gruppi

### Pattern 4: Request + Pending + Result

```rust
// 1. Set pending flag
self.pending_user_check = Some(username.clone());
self.user_check_request_id = Some(request_id.clone());
self.user_check_timestamp = Some(Instant::now());

// 2. Send request
self.send_via_websocket(Outgoing::CheckUser { username, request_id });

// 3. Wait for result
// UiEvent::UserCheckResult -> handle_user_check_result()

// 4. Clear pending state
self.pending_user_check = None;
self.user_check_request_id = None;
self.user_check_timestamp = None;
```

**Uso**: Verifica esistenza utente

### Pattern 5: Buffering + Flush

```rust
// 1. Receive out-of-order event
if received_seq > expected_seq {
    // Buffer it
    self.user_event_reorder_buffer
        .entry(received_seq)
        .or_insert_with(Vec::new)
        .push(event);
    return;
}

// 2. Process in-order event
self.process_event(event);
self.user_sequence_confirmed = received_seq;

// 3. Flush buffer if possible
while let Some(buffered_events) = 
    self.user_event_reorder_buffer.remove(&(self.user_sequence_confirmed + 1)) 
{
    for ev in buffered_events {
        self.process_event(ev);
        self.user_sequence_confirmed += 1;
    }
}
```

**Uso**: Gestione gap nelle sequenze (implementato in `app/events.rs`)

## Performance Considerations

### Memory Usage

**Current Typical Usage**:
- AppState: ~1KB base
- Conversations: ~50 * 200 bytes = 10KB
- Cached messages: ~1000 * 500 bytes = 500KB
- Buffers: ~10KB
- **Total**: ~500KB-1MB

**Growth Factors**:
- Messages accumulate over time
- No automatic eviction (yet)
- Buffers grow with gaps

**Monitoring**:
```rust
if self.get_total_cached_messages() > 50000 {
    warn!("Memory usage high: {} messages", count);
}
```

### Channel Capacity

```rust
// Unbounded for UI events (shouldn't backpressure UI)
mpsc::unbounded_channel::<UiEvent>()

// Bounded for network (prevents memory explosion)
mpsc::channel::<Outgoing>(200)  // Max 200 pending outgoing
```

**Why 200?**: 
- Typical user sends 1-2 messages/second
- 200 = ~100 seconds of buffer
- Prevents OOM if network stalls

### Atomic Operations

```rust
pub user_sequence_shared: Arc<AtomicU64>,
```

**Performance**:
- `Ordering::Relaxed` for non-critical reads
- `Ordering::SeqCst` when ordering matters
- Lock-free, very fast

**Usage**:
```rust
// Update (main thread)
self.user_sequence_shared.store(new_seq, Ordering::Relaxed);

// Read (WebSocket thread)
let seq = user_sequence_shared.load(Ordering::Relaxed);
```

## Testing Recommendations

### Unit Tests (per module)

**state/core.rs**:
```rust
#[test]
fn test_appstate_new() {
    let waker = Arc::new(|| {});
    let state = AppState::new(waker);
    assert!(state.token.is_none());
    assert_eq!(state.page, Page::Auth);
}
```

**state/helpers.rs**:
```rust
#[test]
fn test_is_dm_stub() {
    let mut state = /* ... */;
    let stub_id = Uuid::new_v4();
    state.add_dm_stub(stub_id, "bob".into());
    assert!(state.is_dm_stub(stub_id));
}
```

**state/commands.rs**:
```rust
#[test]
fn test_request_dm_creation_requires_connection() {
    let mut state = /* ... */;
    state.ws_status = WsStatus::Disconnected;
    state.request_dm_creation("bob".into());
    // Assert error event sent
}
```

### Integration Tests

**Stub lifecycle**:
```rust
#[test]
fn test_dm_stub_timeout() {
    let mut state = /* ... */;
    let stub_id = state.create_dm_stub("bob".into()).unwrap();
    
    // Simulate 31 seconds passing
    std::thread::sleep(Duration::from_secs(31));
    state.cleanup_expired_stubs();
    
    assert!(!state.is_dm_stub(stub_id));
}
```

**Message confirmation**:
```rust
#[test]
fn test_message_confirmation_timeout() {
    let mut state = /* ... */;
    let client_id = "test-123".to_string();
    let msg = /* create message */;
    
    state.pending_confirmations.insert(client_id.clone(), msg);
    
    // Simulate 11 seconds passing
    std::thread::sleep(Duration::from_secs(11));
    state.cleanup_pending_confirmations();
    
    // Check message marked as failed
}
```



## Conclusioni

Lo state management del client Rust Ruggine implementa:

- ✅ **Single source of truth** con `AppState` centralizzato
- ✅ **Clear organization** con separazione moduli per responsabilità
- ✅ **Type-safe state** con Rust type system
- ✅ **Reactive updates** via event-driven architecture
- ✅ **Efficient cleanup** con timeout automatici e periodic cleanup
- ✅ **Race condition prevention** con flag atomici e immediate reset
- ✅ **Debug support** con tracing completo e debug info
- ✅ **Memory efficiency** con cleanup automatico e limits
- ✅ **Robust recovery** con sequence tracking e gap detection
- ✅ **User verification** con timeout e correlation IDs
- ✅ **Optimistic UI** con stub system e confirmations

Il design permette:
- Facile reasoning sul flusso dati
- Debugging semplificato (single struct inspection)
- Testing granulare (isolated state mutations)
- Estensione sicura (type-checked additions)
- Performance predicibile (no hidden state)
- Maintainability (clear separation of concerns)

---

**Document Version**: 2.0  
**Last Updated**: 2025-01-23  
