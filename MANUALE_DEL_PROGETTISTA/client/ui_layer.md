# UI Layer - Rust Ruggine Chat Client

**Versione**: 3.0
**Data**: 23 Dicembre 2025  
**Framework**: egui (Immediate Mode GUI)

---

## Indice

1. [Panoramica](#panoramica)
2. [Architettura UI](#architettura-ui)
3. [Organizzazione File](#organizzazione-file)
4. [Componenti Layout](#componenti-layout)
5. [Pages](#pages)
6. [Modals e Popup](#modals-e-popup)
7. [Sistema Toast](#sistema-toast)
8. [Palette Colori](#palette-colori)
9. [State Management UI](#state-management-ui)
10. [Performance e Ottimizzazioni](#performance-e-ottimizzazioni)

---

## Panoramica

L'UI del client Rust Ruggine è costruita con **egui**, un framework immediate mode GUI. L'architettura segue rigorosamente il pattern immediate mode dove l'interfaccia viene completamente ricostruita ad ogni frame basandosi sullo stato corrente (`AppState`).

### Caratteristiche Principali

- **Framework**: egui 0.x (immediate mode)
- **Pattern**: State-driven rendering
- **Reattività**: Waker-based updates da eventi async
- **Temi**: Dark/Light mode automatico
- **Icone**: egui_remixicon
- **Responsiveness**: Layout resizable e adaptive
- **Animazioni**: Slide-in toast, spinner, smooth scrolling

### Principi Architetturali

1. **Single Source of Truth**: `AppState` è l'unica fonte di verità per l'UI
2. **No State Retention**: Nessuno stato UI persistito tra frames (eccetto egui memory API per casi specifici)
3. **Declarative**: L'UI descrive come dovrebbe apparire dato lo stato corrente
4. **Event-Driven**: Cambiam enti stato → waker → repaint automatico

---

## Architettura UI

### Immediate Mode Rendering

**Flusso di Rendering Completo**:

```
Frame Tick
    ↓
App::update() chiamato
    ↓
1. ws_manager.ensure_ws_lifecycle(state)
   - Verifica connessione WebSocket
   - Riconnette se necessario
    ↓
2. state.drain_events()
   - Processa tutti gli eventi in coda (UiEvent)
   - Aggiorna AppState di conseguenza
    ↓
3. state.prune_expired_toasts(5s)
   - Rimuove toast scaduti
    ↓
4. Render UI Components:
   a) header_manager.show_header(ctx, state)
      - Sempre visibile
      - Logo, username, WS status
   
   b) if authenticated:
      - show_main_layout():
        * SidePanel (sidebar_manager)
        * CentralPanel (pages::chat o conversations)
      else:
      - show_auth_layout():
        * CentralPanel (pages::auth)
   
   c) toast_renderer.render(ctx, state)
      - Overlay notifiche
   
   d) Modals:
      - account_modal.show_modal(ctx, state)
      - delete_account_modal.show_modal(ctx, state)
    ↓
5. periodic_cleanup()
   - Ogni 5 minuti: cleanup messaggi vecchi, stubs scaduti
    ↓
6. Frame completato
```

### Struttura App Principal

**File**: `src/app.rs`

```rust
pub struct App {
    state: AppState,
    ws_manager: WebSocketManager,
    sidebar_manager: SidebarManager,
    header_manager: HeaderManager,
    account_modal: AccountModal,
    delete_account_modal: DeleteAccountModal,
    toast_renderer: ToastRenderer,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Lifecycle & Event Processing
        self.ws_manager.ensure_ws_lifecycle(&mut self.state);
        self.state.drain_events();
        self.state.prune_expired_toasts(Duration::from_secs(5));

        // 2. Header (sempre visibile)
        self.header_manager.show_header(ctx, &mut self.state);

        // 3. Layout principale
        if self.state.token.is_none() {
            self.show_auth_layout(ctx);
        } else {
            self.show_main_layout(ctx);
        }

        // 4. Toast notifications
        if self.state.token.is_some() && !matches!(self.state.page, Page::Auth) {
            self.toast_renderer.render(ctx, &mut self.state);
        }

        // 5. Modals
        self.account_modal.show_modal(ctx, &mut self.state);
        self.delete_account_modal.show_modal(ctx, &mut self.state);

        // 6. Cleanup periodico
        self.periodic_cleanup();
    }
}
```

**Manager Pattern**: Ogni componente UI principale è un "manager" che:
- Mantiene stato UI-specific locale (se necessario)
- Espone metodo `show()` o `render()` che prende `&mut AppState`
- È responsabile del rendering completo del suo componente
- Mantiene separazione delle responsabilità chiara

---

## Organizzazione File

### Directory Structure Completa

```
src/ui/
├── components/
│   └── toast_renderer.rs         # Sistema notifiche toast
│
├── layout/
│   ├── header.rs                  # Barra superiore app
│   └── sidebar/
│       ├── mod.rs                 # SidebarManager
│       └── conversations.rs       # ConversationsSidebar
│
├── modals/
│   ├── mod.rs                     # Re-export modals
│   ├── account.rs                 # AccountModal (settings, stats)
│   ├── delete_account.rs          # DeleteAccountModal
│   └── conversation_popups/
│       ├── mod.rs                 # Re-export popup functions
│       ├── action_selection.rs   # Scelta tipo conversazione
│       ├── create_dm.rs           # Creazione DM
│       ├── create_group.rs        # Creazione gruppo
│       ├── invite.rs              # Invito membri a gruppo
│       └── delete_confirmation.rs # Conferma eliminazione conv
│
├── pages/
│   ├── mod.rs                     # Re-export pages
│   ├── auth.rs                    # Login/Register page
│   └── chat.rs                    # Chat interface completa
│
└── mod.rs                         # Root module exports
```

### Module Exports

**`ui/mod.rs`**:
```rust
pub mod components;
pub mod layout;
pub mod modals;
pub mod pages;

pub use layout::sidebar::SidebarManager;
pub use layout::header::HeaderManager;
pub use components::toast_renderer::ToastRenderer;
pub use modals::account::AccountModal;
pub use modals::delete_account::DeleteAccountModal;
```

**`ui/modals/mod.rs`**:
```rust
pub mod account;
pub mod delete_account;
pub mod conversation_popups;

pub use account::AccountModal;
pub use delete_account::DeleteAccountModal;
```

**`ui/modals/conversation_popups/mod.rs`**:
```rust
mod action_selection;
mod create_dm;
mod create_group;
mod invite;
mod delete_confirmation;

pub use action_selection::show_action_selection_popup;
pub use create_dm::show_create_dm_popup;
pub use create_group::show_create_group_popup;
pub use invite::show_invite_popup;
pub use delete_confirmation::show_delete_confirmation_popup;
```

**`ui/pages/mod.rs`**:
```rust
pub mod auth;
pub mod chat;
```

---

## Componenti Layout

### 1. HeaderManager

**File**: `src/ui/layout/header.rs`

**Responsabilità**: Barra superiore dell'applicazione, sempre visibile, fornisce identità app e status utente.

**Struttura**:
```rust
pub struct HeaderManager;

impl HeaderManager {
    pub fn show_header(&mut self, ctx: &egui::Context, state: &mut AppState)
}
```

**Layout**:
```
┌─────────────────────────────────────────────────────────────┐
│ [🦀 Ruggine Chat]              [Loading...] [●] [👤 Alberto]│
│  Logo + Title                   Spinner  WS  Account Button │
└─────────────────────────────────────────────────────────────┘
```

**Contenuto Dettagliato**:

1. **Titolo App (sinistra)**:
   - Icona: `CHAT_SMILE_FILL` (size: 30.0, color: primary orange)
   - Testo: "Ruggine Chat" (size: 24.0, strong)

2. **User Section (destra, se autenticato)**:
   - **Loading Indicator** (se `is_loading && !is_initial_load_complete`):
     - Label: "Caricamento..."
     - Spinner animato
   
   - **WebSocket Status Icon**:
     - Connected: `CHECKBOX_CIRCLE_FILL` (verde) - "WebSocket connesso"
     - Connecting: `REFRESH_FILL` (giallo) - "Connessione in corso..."
     - Disconnected: `CLOSE_CIRCLE_FILL` (rosso) - "WebSocket disconnesso"
   
   - **Separator**
   
   - **Account Button**:
     - Icona: `USER_FILL` + username
     - Size: 18.0
     - Fill: `rgb(255, 140, 60)` con opacity 0.06
     - Hover: cursor pointing hand
     - Tooltip: "Impostazioni account"
     - Click: apre `AccountModal`

**Implementazione Key Points**:

```rust
egui::TopBottomPanel::top("header")
    .min_height(50.0)
    .show(ctx, |ui| {
        ui.horizontal(|ui| {
            // Titolo
            show_app_title(ui);
            
            // User info a destra
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if state.is_authenticated() {
                    // Loading, WS status, account button
                }
            });
        });
    });
```

---

### 2. SidebarManager

**File**: `src/ui/layout/sidebar/mod.rs`

**Responsabilità**: Pannello laterale sinistro, delega rendering a `ConversationsSidebar` in base alla pagina corrente.

**Struttura**:
```rust
pub struct SidebarManager {
    conversations_sidebar: ConversationsSidebar,
}

impl SidebarManager {
    pub fn show_sidebar(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.vertical(|ui| {
            match state.page {
                Page::Auth => {
                    // Sulla pagina Auth non mostriamo sidebar
                },
                Page::Conversations | Page::Chat => {
                    self.conversations_sidebar.show(ui, state);
                },
            }
        });
    }
}
```

**Caratteristiche**:
- Visibilità: Solo se autenticato e in Conversations/Chat page
- Resizable: 200-500px (default 300px)
- Persistenza width: Gestita da egui automaticamente

---

### 3. ConversationsSidebar

**File**: `src/ui/layout/sidebar/conversations.rs`

**Responsabilità**: Lista conversazioni con ricerca, filtri, paginazione e gestione popup.

**Struttura Completa**:
```rust
pub struct ConversationsSidebar {
    search_query: String,           // Query ricerca attiva
    dm_username: String,            // Username per DM popup
    show_action_popup: bool,        // Mostra action selection
    show_create_dm_popup: bool,     // Mostra create DM popup
}
```

**Layout**:
```
┌──────────────────────┐
│ Conversazioni    [+] │  ← Header
├──────────────────────┤
│ 🔍 [Cerca...]        │  ← Search bar
├──────────────────────┤
│ ┌──────────────────┐ │
│ │ 👥 Team Alpha  99│ │  ← Conversation entry
│ │ Ultimo msg...  2h│ │
│ ├──────────────────┤ │
│ │ 💬 Mario       ●│ │  ← Entry con unread
│ │ Ciao!      Oggi │ │
│ └──────────────────┘ │
└──────────────────────┘
```

**Sezioni Principali**:

#### A. Header
```rust
ui.horizontal(|ui| {
    ui.label("Conversazioni".strong().size(18.0));
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let create_btn = Button::new(CHAT_NEW_LINE)
            .frame(false)
            .fill(TRANSPARENT);
        if ui.add(create_btn).clicked() {
            self.show_action_popup = true;
        }
    });
});
```

#### B. Search Bar
```rust
Frame::none()
    .fill(background_color)  // Adaptive dark/light
    .inner_margin(symmetric(12.0, 10.0))
    .rounding(8.0)
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(SEARCH_LINE.weak());
            TextEdit::singleline(&mut self.search_query)
                .hint_text("Cerca conversazioni...")
                .show(ui);
        });
    });
```

#### C. Conversations List con Auto-Load

**Features Principali**:
1. **Filtraggio Real-Time**:
```rust
let filtered: Vec<_> = conversations
    .iter()
    .filter(|conv| {
        if search_query.is_empty() {
            true
        } else {
            conv.title.to_lowercase().contains(&search_query.to_lowercase())
                || match conv.kind.as_str() {
                    "group" => "gruppo".contains(&search_query),
                    "dm" => "privata".contains(&search_query) || "dm".contains(&search_query),
                    _ => false,
                }
        })
    .collect();
```

2. **Auto-Load Durante Ricerca**:
   - Se la ricerca non trova risultati E ci sono più conversazioni disponibili
   - Carica automaticamente altre 20 conversazioni
   - Mostra spinner durante il caricamento

```rust
if filtered.is_empty() && !search_query.is_empty() {
    if !state.is_loading_more_conversations && state.has_more_conversations {
        state.is_loading_more_conversations = true;
        let cursor = conversations.last().map(|c| c.last_activity);
        // Spawn task per caricare più conversazioni
    }
    
    if state.is_loading_more_conversations {
        ui.spinner();
        ui.label("Caricamento conversazioni...");
    }
}
```

3. **Scroll State Persistence**:
   - Salva offset scroll in egui memory
   - Ripristina posizione dopo aggiornamenti

```rust
let last_offset_id = ui.id().with("conversations_last_offset");
let last_offset: f32 = ui.data_mut(|d|
    d.get_temp(last_offset_id).unwrap_or(0.0)
);

// Dopo rendering
ui.data_mut(|d| d.insert_temp(last_offset_id, scroll_output.state.offset.y));
```

#### D. Conversation Entry Rendering

**Entry Completo**:
```rust
Frame::none()
    .fill(if is_selected {
        primary_color.linear_multiply(0.2)
    } else if pointer_over {
        primary_color.linear_multiply(0.08)
    } else {
        Color32::TRANSPARENT
    })
    .inner_margin(10.0)
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            // Icona tipo conversazione
            let (icon, icon_color) = match conv.kind {
                "dm" => (CHAT_1_FILL, color_based_on_selection),
                "group" => (TEAM_FILL, color_based_on_selection),
            };
            ui.label(RichText::new(icon).size(16.0).color(icon_color));
            
            ui.vertical(|ui| {
                // Titolo
                ui.label(RichText::new(&conv.title).size(14.0));
                
                // Anteprima ultimo messaggio
                let preview = /* ultimo messaggio o "Nessun messaggio" */;
                ui.label(RichText::new(preview).size(12.0).weak());
            });
            
            // Destra: badge unread + timestamp
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Badge unread count
                if let Some(&count) = state.conversation_unread_counts.get(&conv.id) {
                    if count > 0 {
                        let badge_text = if count > 99 { "99+" } else { count.to_string() };
                        // Cerchio arancione con testo bianco
                        ui.painter().circle_filled(center, 11.0, orange);
                        ui.painter().text(center, CENTER_CENTER, badge_text, font, WHITE);
                    }
                }
                
                // Timestamp formattato
                if let Some(date_text) = format_date_label(last_msg_timestamp) {
                    ui.label(RichText::new(date_text).size(10.0).weak());
                }
            });
        });
    });
```

**Date Formatting**:
```rust
fn format_date_label(timestamp: i64) -> String {
    let dt = DateTime::from_timestamp(timestamp, 0).with_timezone(&Local);
    let today = Local::now().date_naive();
    let msg_date = dt.date_naive();
    
    match (today - msg_date).num_days() {
        0 => "Oggi",
        1 => "Ieri",
        _ => dt.format("%d/%m/%y"),
    }
}
```

#### E. Context Menu

**Click Destro su Entry**:
```rust
response.context_menu(|ui| {
    let label = if conv.kind == "group" {
        if is_owner {
            format!("{} Elimina gruppo", DELETE_BIN_LINE)
        } else {
            format!("{} Esci dal gruppo", LOGOUT_BOX_LINE)
        }
    } else {
        format!("{} Elimina conversazione", DELETE_BIN_LINE)
    };
    
    if ui.button(RichText::new(label).size(14.0)).clicked() {
        state.request_delete_confirmation(conv);
        ui.close_menu();
    }
});
```

#### F. Empty State

```rust
fn show_empty_state(&mut self, ui: &mut egui::Ui, _state: &mut AppState) {
    ui.vertical_centered(|ui| {
        ui.add_space(60.0);
        ui.label(RichText::new(INBOX_LINE).size(48.0).weak());
        ui.add_space(12.0);
        ui.label("Nessuna conversazione");
        ui.label("Le tue conversazioni appariranno qui".weak());
        ui.add_space(12.0);
        ui.label("oppure".weak());
        ui.add_space(12.0);
        
        if ui.button("Crea nuova conversazione").clicked() {
            self.show_action_popup = true;
        }
    });
}
```

#### G. Popup Management

Tutti i popup conversation vengono renderizzati alla fine:
```rust
conversation_popups::show_delete_confirmation_popup(ui.ctx(), state);
conversation_popups::show_action_selection_popup(
    ui.ctx(),
    state,
    &mut self.show_action_popup,
    &mut self.show_create_dm_popup
);
conversation_popups::show_create_dm_popup(
    ui.ctx(),
    state,
    &mut self.show_create_dm_popup,
    &mut self.dm_username
);
conversation_popups::show_create_group_popup(ui.ctx(), state);
```

---

## Pages

### 1. Auth Page

**File**: `src/ui/pages/auth.rs`

**Responsabilità**: Gestisce autenticazione utente (login e registrazione).

**Struttura**:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum AuthView {
    Login,
    Register,
}

pub fn panel(ui: &mut egui::Ui, s: &mut AppState)
```

**State Persistence**: Usa egui memory API per mantenere vista corrente:
```rust
let current_view = ui.data_mut(|d| {
    d.get_temp::<AuthView>(Id::new("auth_view"))
        .unwrap_or(AuthView::Login)
});
```

#### A. Logged In View

Mostra quando `state.token.is_some()`:

```
┌────────────────────────────┐
│  ✓ Autenticato             │
│                             │
│  ┌─────────────────────┐  │
│  │ 👤 Alberto          │  │
│  │ 🔑 a1b2c3d4...      │  │
│  │ 📊 Sequenza: #1234  │  │
│  │ 💬 5 conversazioni  │  │
│  └─────────────────────┘  │
│                             │
│  [   🚪 Logout   ]        │
└────────────────────────────┘
```

**Dettagli**:
- Icona check verde (size: 42.0)
- Frame con border, padding 20.0, min-width 450px
- Info visualizzate:
  - Username con icona USER_LINE (size: 24.0)
  - User ID troncato (primi 8 char) con icona FINGERPRINT_LINE
  - Sequenza utente confermata (se > 0)
  - Conteggio conversazioni attive
- Bottone logout rosso scuro (size: 18.0, min 250x50px)

**Logout Flow**:
```rust
if logout_button.clicked() {
    s.username.clear();
    s.password.clear();
    s.password_confirm.clear();
    
    // Spawn async logout
    let base = s.base.clone();
    let token = s.token.clone().unwrap();
    let tx = s.ui_tx.clone();
    s.rt.spawn(async move {
        let _ = api::auth::logout(&base, &token).await;
        let _ = tx.send(UiEvent::LoggedOut);
    });
}
```

#### B. Login View

```
┌────────────────────────────┐
│    🦀 Ruggine Chat         │
│                             │
│  ┌─────────────────────┐  │
│  │ 👤 Username         │  │
│  │   [input...]        │  │
│  │                      │  │
│  │ 🔒 Password         │  │
│  │   [••••••••]        │  │
│  └─────────────────────┘  │
│                             │
│  [    🔐 Login    ]        │
│                             │
│  Non hai un account?       │
│  [Registrati]              │
└────────────────────────────┘
```

**Features**:
- Titolo grande: "🦀 Ruggine Chat" (size: 56.0, orange)
- **Messaggi Error/Info**:
  - Se `auth_message` presente:
    - Verde per info (`auth_message_is_error == false`)
    - Rosso per errori (`auth_message_is_error == true`)
  - Mostrati centrati sopra il form

- **Form Frame**:
  - Border, padding 30.0, size 500px
  - Username field: icona USER_LINE + TextEdit
  - Password field: icona LOCK_PASSWORD_LINE + TextEdit con `password(true)`
  
- **Enter Key Handling**:
```rust
if password_response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
    if !username.is_empty() && !password.is_empty() && !is_busy {
        start_login(s);
    }
}
```

- **Login Button**:
  - Disabled se campi vuoti o `is_busy`
  - Fill orange se enabled, gray se disabled
  - Size: 18.0, min 400x50px

- **Link Registrazione**:
  - Sotto il form
  - Testo: "Non hai un account?" + link arancione "Registrati"
  - Click: cambia vista a Register

**Login Flow**:
```rust
fn start_login(s: &mut AppState) {
    s.clear_auth_message();
    let tx = s.ui_tx.clone();
    let _ = tx.send(UiEvent::LoginStarted);
    
    s.rt.spawn(async move {
        match api::auth::login(&base, &username, &password).await {
            Ok(login_resp) => {
                let _ = tx.send(UiEvent::Logged(
                    login_resp.token,
                    login_resp.user_id,
                    login_resp.last_sequence
                ));
            }
            Err(e) => {
                let error_type = if e.to_string().contains("401") {
                    ErrorType::Auth("Nome utente o password errati")
                } else {
                    ErrorType::Auth("Errore di connessione al server")
                };
                let _ = tx.send(UiEvent::Error(error_type));
                let _ = tx.send(UiEvent::LoggedOut);
            }
        }
    });
}
```

#### C. Register View

```
┌────────────────────────────┐
│    🦀 Ruggine Chat         │
│                             │
│  ┌─────────────────────┐  │
│  │ 👤 Username         │  │
│  │   [input...]        │  │
│  │                      │  │
│  │ 🔒 Password         │  │
│  │   [••••••••]        │  │
│  │                      │  │
│  │ 🔒 Conferma Pass    │  │
│  │   [••••••••]        │  │
│  │   ✓ Corrispondono  │  │
│  └─────────────────────┘  │
│                             │
│  [  👤 Crea Account  ]     │
│                             │
│  Hai già un account?       │
│  [Torna al Login]          │
│                             │
│  ⏳ Creazione in corso...  │
└────────────────────────────┘
```

**Password Confirmation Validation**:
```rust
if !password_confirm.is_empty() {
    if password == password_confirm {
        ui.label("✓ Le password corrispondono".green());
    } else {
        ui.label("✗ Le password non corrispondono".red());
    }
}
```

**Register Button**:
- Enabled solo se:
  - Username non vuoto
  - Password non vuota
  - Password == Password Confirm
  - Non `is_busy`

**Registration Flow**:
```rust
fn start_registration(s: &mut AppState) {
    s.clear_auth_message();
    let tx = s.ui_tx.clone();
    let _ = tx.send(UiEvent::RegisterStarted);
    
    s.rt.spawn(async move {
        match api::auth::register(&base, &username, &password).await {
            Ok(_) => {
                // Auto-login dopo registrazione
                let _ = tx.send(UiEvent::Info("Registrazione completata, login automatico..."));
                
                match api::auth::login(&base, &username, &password).await {
                    Ok(login_resp) => {
                        let _ = tx.send(UiEvent::Logged(
                            login_resp.token,
                            login_resp.user_id,
                            login_resp.last_sequence
                        ));
                    }
                    Err(_) => {
                        let _ = tx.send(UiEvent::Info(
                            "Account creato ma login automatico fallito"
                        ));
                        let _ = tx.send(UiEvent::LoggedOut);
                    }
                }
            }
            Err(e) => {
                let error_type = if e.to_string().contains("409") {
                    ErrorType::Auth("Nome utente già registrato")
                } else if e.to_string().contains("400") {
                    ErrorType::Auth("Nome utente o password non validi")
                } else {
                    ErrorType::Auth("Errore durante la registrazione")
                };
                let _ = tx.send(UiEvent::Error(error_type));
                let _ = tx.send(UiEvent::LoggedOut);
            }
        }
    });
}
```

**Spinner Durante Busy**:
- Mostrato centrato sotto il form
- Accompagnato da testo: "Creazione account in corso..." (orange)

---

### 2. Chat Page

**File**: `src/ui/pages/chat.rs`

**Responsabilità**: Interfaccia chat completa con messaggi, scroll intelligente, input, e popup.

#### Entry Point

```rust
pub fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    // 1. Verifica autenticazione
    if s.token.is_none() {
        show_empty_state(ui);  // Lock icon + "Login richiesto"
        return;
    }
    
    // 2. Mostra chat se conversazione selezionata
    if let Some(cid) = s.cid {
        show_chat_interface(ui, s, cid);
    } else {
        show_empty_state(ui);  // Vuoto generico
    }
    
    // 3. Popup
    if s.pending_message_deletion.is_some() {
        show_delete_message_confirmation(ui, s);
    }
    if s.pending_member_kick.is_some() {
        show_kick_member_confirmation(ui, s);
    }
}
```

#### A. Chat Interface Layout

**Split Verticale Manuale**:
```
┌──────────────────────────────┐
│ Header: [Titolo] [Invita]   │  ← 50-60px fisso
├──────────────────────────────┤
│                              │
│   Area Messaggi Scrollable   │  ← Cresce (total_h - input_h - 13px)
│                              │
├──────────────────────────────┤
│ [Input Message]     [Send]  │  ← 60px fisso
└──────────────────────────────┘
```

**Calcolo Altezze**:
```rust
let total_h = ui.available_height();
let input_h: f32 = 60.0;
let separator_space: f32 = 13.0;
let messages_h = (total_h - input_h - separator_space).max(120.0);
```

#### B. Conversation Header

```rust
fn show_conversation_header(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    Frame::none()
        .fill(ui.visuals().window_fill())
        .inner_margin(symmetric(20.0, 10.0))
        .stroke(Stroke::new(1.0, ui.visuals().window_stroke().color))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Titolo conversazione
                let (icon, title) = match conv.kind {
                    "dm" => (CHAT_1_FILL, &conv.title),
                    "group" => (TEAM_FILL, &conv.title),
                };
                ui.label(RichText::new(icon).size(20.0).orange());
                ui.label(RichText::new(title).size(18.0).strong());
                
                // Bottoni azione (solo per gruppi)
                if conv.kind == "group" && is_owner {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Bottone invita
                        if ui.button("👥 Invita").clicked() {
                            s.show_invite_popup = true;
                        }
                        
                        // Bottone info gruppo
                        if ui.button("ℹ️ Info").clicked() {
                            s.show_group_info_popup = true;
                        }
                    });
                }
            });
        });
}
```

#### C. Messages Area con Scroll Intelligente

**Features Avanzate**:
1. **Anchor Management**: Mantiene posizione scroll quando nuovi messaggi vengono caricati
2. **Auto-Scroll**: Scroll automatico a bottom per nuovi messaggi
3. **Load More Trigger**: Carica messaggi vecchi scrollando verso l'alto
4. **Date Separators**: Separa messaggi per data

**Stati Persistenti (egui memory)**:
```rust
let anchor_state_id = ui.id().with("anchor_state").with(cid);
let last_offset_id = ui.id().with("last_offset").with(cid);
let messages_count_id = ui.id().with("msg_count").with(cid);

// Recupera anchor message ID
let anchor_message_id: Option<Uuid> = ui.data_mut(|d|
    d.get_temp(anchor_state_id).unwrap_or(None)
);

// Recupera last message count
let last_message_count: usize = ui.data_mut(|d|
    d.get_temp(messages_count_id).unwrap_or(0)
);
```

**ScrollArea Setup**:
```rust
let scroll = ScrollArea::vertical()
    .auto_shrink([false, false])
    .stick_to_bottom(true)  // Auto-scroll per nuovi messaggi
    .max_height(messages_h)
    .id_source("chat_messages_scroll");

let output = scroll.show(ui, |ui| {
    // Margini laterali
    ui.horizontal(|ui| {
        ui.add_space(40.0);  // Margine sinistro
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 40.0);  // Margine destro
            
            // Contenuto messaggi
        });
    });
});
```

**Empty States**:
```rust
// Caricamento iniziale
if s.messages.is_empty() && s.is_loading_more {
    ui.vertical_centered(|ui| {
        ui.add_space(messages_h / 2.0 - 20.0);
        ui.spinner();
        ui.label("Caricamento chat...");
    });
    return;
}

// Chat vuota
if s.messages.is_empty() {
    ui.vertical_centered(|ui| {
        ui.add_space(messages_h / 2.0 - 40.0);
        ui.label("— Chat vuota —");
        ui.add_space(10.0);
        ui.label("Invia il primo messaggio per iniziare!");
    });
    return;
}
```

**Messages Rendering Loop**:
```rust
// Indicatore inizio conversazione
if !*s.has_more_messages.get(&cid).unwrap_or(&true) {
    ui.vertical_centered(|ui| {
        ui.label("— Inizio conversazione —");
    });
}

// Spinner caricamento
if s.is_loading_more {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label("Caricamento messaggi precedenti...");
    });
    ui.separator();
}

let messages = s.messages.clone();
let message_count = messages.len();
let mut last_date: Option<NaiveDate> = None;

for i in 0..message_count {
    let message = &messages[i];
    
    // Date separator
    if let Some(current_date) = get_date_from_timestamp(message.created_at) {
        let should_show = match last_date {
            None => true,
            Some(prev_date) => prev_date != current_date,
        };
        
        if should_show {
            show_date_separator(ui, message.created_at);
            ui.add_space(8.0);
        }
        last_date = Some(current_date);
    }
    
    // Anchor scrolling
    if message_count > last_message_count && Some(message.id) == anchor_message_id {
        ui.scroll_to_cursor(Some(Align::TOP));
        ui.data_mut(|d| d.insert_temp(anchor_state_id, None::<Uuid>));
    }
    
    show_message(ui, s, message);
    
    if i < message_count - 1 {
        ui.add_space(6.0);
    }
}

// Salva message count per prossimo frame
ui.data_mut(|d| d.insert_temp(messages_count_id, message_count));
```

**Load More Trigger Logic**:
```rust
// Recupera ultimo offset
let last_offset: f32 = ui.data_mut(|d|
    d.get_temp(last_offset_id).unwrap_or(f32::MAX)
);

// Trigger threshold (3/4 superiori)
let trigger_threshold = messages_h * 0.75;
let near_top = output.state.offset.y <= trigger_threshold;

// Detect scroll up
let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
let scrolling_up_with_wheel = scroll_delta > 0.0;
let offset_decreased = output.state.offset.y < last_offset;

// Trigger fetch se:
// - Near top
// - Scrolling up (qualsiasi metodo)
// - Non già loading
// - Ci sono più messaggi
if near_top 
    && (scrolling_up_with_wheel || offset_decreased)
    && !s.is_loading_more
    && *s.has_more_messages.get(&cid).unwrap_or(&true)
{
    // Salva anchor (primo messaggio visibile)
    if let Some(first_msg) = s.messages.first() {
        ui.data_mut(|d| d.insert_temp(anchor_state_id, Some(first_msg.id)));
    }
    
    // Trigger fetch
    let _ = s.ui_tx.send(UiEvent::LoadMoreMessages(cid));
}

// Salva offset corrente
ui.data_mut(|d| d.insert_temp(last_offset_id, output.state.offset.y));
```

#### D. Date Separator

```rust
fn show_date_separator(ui: &mut egui::Ui, timestamp: i64) {
    let date_text = format_date_label(timestamp);
    
    ui.add_space(6.0);
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.label(
            RichText::new(date_text)
                .size(12.0)
                .color(Color32::from_rgb(140, 140, 140))
        );
    });
    ui.add_space(6.0);
}

fn format_date_label(timestamp: i64) -> String {
    let dt = DateTime::from_timestamp(timestamp, 0).with_timezone(&Local);
    let today = Local::now().date_naive();
    let msg_date = dt.date_naive();
    
    match (today - msg_date).num_days() {
        0 => "Oggi",
        1 => "Ieri",
        _ => dt.format("%d/%m/%y"),
    }
}
```

#### E. Message Rendering

```rust
fn show_message(ui: &mut egui::Ui, s: &AppState, message: &MessageDto) {
    let is_own = s.user_id.map_or(false, |uid| uid == message.user_id);
    let align = if is_own { Align::Max } else { Align::Min };
    
    ui.with_layout(Layout::top_down(align), |ui| {
        let max_width = ui.available_width() * 0.7;
        
        // Container messaggio
        Frame::none()
            .fill(if is_own {
                Color32::from_rgb(200, 100, 40)  // Orange per propri
            } else {
                ui.visuals().widgets.noninteractive.bg_fill  // Tema per altri
            })
            .inner_margin(12.0)
            .rounding(12.0)
            .show(ui, |ui| {
                ui.set_max_width(max_width);
                
                // Username (solo se non proprio e non DM)
                if !is_own && s.conversations.as_ref()
                    .and_then(|c| c.iter().find(|conv| conv.id == message.conversation_id))
                    .map_or(false, |conv| conv.kind == "group")
                {
                    ui.label(
                        RichText::new(&message.username)
                            .size(11.0)
                            .strong()
                            .color(if is_own { Color32::WHITE } else { Color32::from_rgb(200, 100, 40) })
                    );
                    ui.add_space(4.0);
                }
                
                // Contenuto messaggio
                ui.label(
                    RichText::new(&message.content)
                        .size(14.0)
                        .color(if is_own { Color32::WHITE } else { ui.visuals().text_color() })
                );
                
                // Footer: timestamp + status
                ui.horizontal(|ui| {
                    // Timestamp
                    let time_str = format_timestamp_short(message.created_at);
                    ui.label(
                        RichText::new(time_str)
                            .size(10.0)
                            .color(if is_own {
                                Color32::from_white_alpha(180)
                            } else {
                                ui.visuals().weak_text_color()
                            })
                    );
                    
                    // Status (solo per propri messaggi)
                    if is_own {
                        match message.status {
                            MessageStatus::Pending => {
                                ui.label(RichText::new(TIMER_LINE).size(12.0).color(Color32::YELLOW));
                            }
                            MessageStatus::Confirmed => {
                                ui.label(RichText::new(CHECK_DOUBLE_LINE).size(12.0).color(Color32::GREEN));
                            }
                            MessageStatus::Failed => {
                                ui.label(RichText::new(ERROR_WARNING_LINE).size(12.0).color(Color32::RED));
                            }
                        }
                    }
                });
            }).response
            .context_menu(|ui| {
                // Context menu per eliminazione (solo owner o sender)
                if s.user_id.map_or(false, |uid| uid == message.user_id) {
                    if ui.button(format!("{} Elimina messaggio", DELETE_BIN_LINE)).clicked() {
                        s.pending_message_deletion = Some((message.conversation_id, message.id));
                        ui.close_menu();
                    }
                }
            });
    });
}

fn format_timestamp_short(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_default()
}
```

#### F. Input Area

```rust
fn show_input_area(ui: &mut egui::Ui, s: &mut AppState, cid: Uuid) {
    Frame::none()
        .fill(ui.visuals().window_fill())
        .inner_margin(symmetric(20.0, 10.0))
        .stroke(Stroke::new(1.0, ui.visuals().window_stroke().color))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // TextEdit
                let input_response = ui.add(
                    TextEdit::multiline(&mut s.input_text)
                        .desired_width(ui.available_width() - 120.0)
                        .desired_rows(1)
                        .hint_text("Scrivi un messaggio...")
                );
                
                // Enter senza shift = invia
                if input_response.has_focus() {
                    ui.input(|i| {
                        if i.key_pressed(Key::Enter) && !i.modifiers.shift {
                            if !s.input_text.trim().is_empty() && s.ws_status == WsStatus::Connected {
                                let _ = s.ui_tx.send(UiEvent::SendMessage);
                            }
                        }
                    });
                }
                
                // Bottone Send
                let can_send = !s.input_text.trim().is_empty() 
                    && s.ws_status == WsStatus::Connected;
                
                let send_btn = Button::new(
                    RichText::new(format!("{} Invia", SEND_PLANE_FILL))
                        .size(14.0)
                        .color(if can_send { Color32::WHITE } else { Color32::GRAY })
                )
                .fill(if can_send {
                    Color32::from_rgb(200, 100, 40)
                } else {
                    Color32::from_gray(100)
                })
                .min_size(vec2(100.0, 40.0));
                
                if ui.add_enabled(can_send, send_btn).clicked() {
                    let _ = s.ui_tx.send(UiEvent::SendMessage);
                }
            });
        });
}
```

#### G. Group Info Popup

Mostra quando `s.show_group_info_popup == true`:

```
┌──────────────────────────┐
│  👥 Info Gruppo          │
│                          │
│  Nome: Team Alpha        │
│  Owner: 👑 Alberto       │
│  Membri: 5               │
│                          │
│  ┌────────────────────┐ │
│  │ 👤 Mario (member) │ │
│  │ 👤 Luigi (member) │ │
│  │ 👤 ... [Espelli]  │ │ ← Solo se owner
│  └────────────────────┘ │
│                          │
│  [🗑️ Elimina Gruppo]    │ ← Solo owner
│  [🚪 Esci dal Gruppo]   │ ← Solo participant
└──────────────────────────┘
```

**Features**:
- Header con info gruppo
- ScrollArea membri (max height 280px)
- Badge corona oro per owner
- Bottone "Espelli" per ogni membro (owner only)
- Bottone azione bottom (Elimina/Esci in base al ruolo)

#### H. Delete Message Confirmation

```rust
fn show_delete_message_confirmation(ui: &mut egui::Ui, s: &mut AppState) {
    let Some((conv_id, msg_id)) = s.pending_message_deletion else { return; };
    
    Window::new("")
        .title_bar(false)
        .fixed_size([400.0, 200.0])
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            ui.vertical_centered(|ui| {
                ui.label("🗑️ Elimina Messaggio".strong().size(20.0));
                ui.add_space(12.0);
                ui.label("Sei sicuro di voler eliminare questo messaggio?");
                ui.add_space(8.0);
                ui.label("Questa azione è irreversibile.".weak());
            });
            
            ui.add_space(20.0);
            
            ui.horizontal(|ui| {
                if ui.button("Annulla").clicked() {
                    s.pending_message_deletion = None;
                }
                
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Elimina").clicked() {
                        let _ = s.ui_tx.send(UiEvent::DeleteMessage(conv_id, msg_id));
                        s.pending_message_deletion = None;
                    }
                });
            });
        });
}
```

#### I. Kick Member Confirmation

Simile a delete message, mostra conferma per espulsione membro da gruppo.

---

## Modals e Popup

### 1. Account Modal

**File**: `src/ui/modals/account.rs`

**Dimensioni**: 500x580px, centrato

**Struttura**:
```rust
pub struct AccountModal;

impl AccountModal {
    pub fn show_modal(&mut self, ctx: &Context, state: &mut AppState)
}
```

**Sezioni**:

#### A. User Info
```
👤 Username
   Alberto
```

#### B. Statistics
```
👥 Gruppi        💬 Chat Private
   3                5
```

#### C. Server Config
```
🌐 Server
   http://localhost:8080
```

#### D. WebSocket Status
```
📡 WebSocket
   ● Connesso
   [🔄]  ← Bottone riconnetti se disconnesso
```

#### E. Sync Status
```
🔄 Sincronizzazione
   95% salute
   Ultimo evento: #1234
   ⚠️ Gap: 2 eventi  ← Solo se presente
```

**Implementazione Completa**:
```rust
fn show_sync_status(&self, ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(egui_remixicon::icons::REFRESH_FILL)
                .size(28.0)
                .color(egui::Color32::from_rgb(200, 100, 40))
        );
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Sincronizzazione")
                    .size(12.0)
                    .color(ui.visuals().weak_text_color())
            );

            // Calcolo health
            let health = SequenceHandler::get_sequence_health(&state);
            let health_color = if health > 0.9 {
                egui::Color32::GREEN
            } else if health > 0.7 {
                egui::Color32::YELLOW
            } else {
                egui::Color32::RED
            };

            // Mostra percentuale health
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:.0}%", health * 100.0))
                        .size(16.0)
                        .strong()
                        .color(health_color)
                );
                ui.label(
                    egui::RichText::new("salute")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                );
            });

            // Mostra ultimo evento confermato
            if state.user_sequence_confirmed > 0 {
                ui.label(
                    egui::RichText::new(format!("Ultimo evento: #{}", state.user_sequence_confirmed))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color())
                );
            }

            // Mostra gap se presente
            let user_gap = if state.user_sequence_received > state.user_sequence_confirmed {
                state.user_sequence_received - state.user_sequence_confirmed
            } else {
                0
            };
            
            if user_gap > 0 {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(egui_remixicon::icons::ERROR_WARNING_LINE)
                            .size(12.0)
                            .color(egui::Color32::YELLOW)
                    );
                    ui.label(
                        egui::RichText::new(format!("Gap: {} eventi", user_gap))
                            .size(11.0)
                            .color(egui::Color32::YELLOW)
                    );
                });
            }
        });
    });
}
```

#### F. Action Buttons

```
[  🚪 Logout  ]                [  🗑️ Elimina Account  ]
  Orange (180x40)               Red (180x40)
  Left aligned                  Right aligned
```

**Layout Implementazione**:
```rust
fn show_action_buttons(&self, ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(40.0, 0.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Logout a sinistra
                let logout_btn = egui::Button::new(
                    egui::RichText::new(format!("{} Logout", egui_remixicon::icons::LOGOUT_BOX_R_FILL))
                        .size(14.0)
                        .color(egui::Color32::WHITE)
                )
                    .fill(egui::Color32::from_rgb(200, 100, 40))
                    .min_size(egui::vec2(180.0, 40.0));

                if ui.add(logout_btn).clicked() {
                    // Logout flow
                }

                // Elimina account a destra (usa Layout::right_to_left)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let delete_btn = egui::Button::new(
                        egui::RichText::new(format!("{} Elimina Account", egui_remixicon::icons::DELETE_BIN_FILL))
                            .size(14.0)
                            .color(egui::Color32::WHITE)
                    )
                        .fill(egui::Color32::from_rgb(180, 50, 50))
                        .min_size(egui::vec2(180.0, 40.0));

                    if ui.add(delete_btn).clicked() {
                        let _ = state.ui_tx.send(UiEvent::DeleteAccountStart);
                    }
                });
            });
        });
}
```

**Logout Flow**:
```rust
if logout_btn.clicked() {
    if let Some(token) = &state.token {
        let base = state.base.clone();
        let token = token.clone();
        let tx = state.ui_tx.clone();
        state.rt.spawn(async move {
            if let Err(e) = api::auth::logout(&base, &token).await {
                let _ = tx.send(UiEvent::Info(format!("logout note: {e}")));
            }
            let _ = tx.send(UiEvent::LoggedOut);
        });
    }
}
```

**Delete Account Trigger**:
```rust
if delete_btn.clicked() {
    let _ = state.ui_tx.send(UiEvent::DeleteAccountStart);
}
```

---

### 2. Delete Account Modal

**File**: `src/ui/modals/delete_account.rs`

**Dimensioni**: 480x260px, centrato

**Trigger**: `state.confirm_delete_account == true`

**Layout**:
```
┌──────────────────────────────────────┐
│      ⚠️ Elimina Account              │
│                                      │
│  Sei sicuro di voler eliminare      │
│  il tuo account?                     │
│                                      │
│  Questa azione è irreversibile.     │
│  Tutti i tuoi dati verranno         │
│  eliminati permanentemente.          │
│                                      │
│  [  ✖ Annulla  ]  [  🗑️ Elimina  ] │
└──────────────────────────────────────┘
```

**Colors**:
- Title: `rgb(220, 60, 60)` (rosso allarme)
- Confirm button: `rgb(200, 50, 50)` (rosso scuro)

**Actions**:
```rust
// Annulla
if cancel_btn.clicked() {
    let _ = state.ui_tx.send(UiEvent::DeleteAccountCancel);
}

// Conferma
if confirm_btn.clicked() {
    let _ = state.ui_tx.send(UiEvent::DeleteAccountConfirm);
}
```

---

### 3. Action Selection Popup

**File**: `src/ui/modals/conversation_popups/action_selection.rs`

**Dimensioni**: 450x320px, centrato

**Layout**:
```
┌──────────────────────────────────────┐
│          💬 Nuova Conversazione      │
│  Scegli il tipo di conversazione   │
│                                      │
│  ┌────────────────────────────────┐ │
│  │ 👥 Crea Gruppo                 │ │ ← Hover → border orange
│  │ Conversazione con più          │ │
│  │ partecipanti                   │ │
│  └────────────────────────────────┘ │
│                                      │
│  ┌────────────────────────────────┐ │
│  │ 💬 Messaggio Privato           │ │
│  │ Messaggio diretto a un         │ │
│  │ singolo utente                 │ │
│  └────────────────────────────────┘ │
└──────────────────────────────────────┘
```

**Interaction**:
- Ogni card è cliccabile
- Hover: border orange spesso (2.0px)
- Click Gruppo → `state.show_create_group_modal = true`
- Click DM → `show_create_dm_popup = true`

**Frame Styling**:
```rust
let frame = Frame::none()
    .stroke(Stroke::new(1.0, orange.linear_multiply(0.4)))
    .rounding(12.0)
    .inner_margin(20.0);

// Hover detection
if ui.rect_contains_pointer(frame_rect) {
    ui.painter().rect_stroke(
        frame_rect,
        12.0,
        Stroke::new(2.0, orange)
    );
    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
}
```

---

### 4. Create DM Popup

**File**: `src/ui/modals/conversation_popups/create_dm.rs`

**Dimensioni**: 450x280px, centrato

**Layout**:
```
┌──────────────────────────────────────┐
│     💬 Nuova Conversazione Privata   │
│                                      │
│  👤 Username destinatario *          │
│     [Inserisci username...]          │
│                                      │
│                                      │
│  [  ✖ Annulla  ]      [ ✉ Crea ]   │
│                                      │
│  ⏳ Verifica 'mario'...              │ ← Se checking
└──────────────────────────────────────┘
```

**Flusso Completo di Validazione**:

Il popup implementa una logica sofisticata che:
1. Verifica se esiste già un DM con quell'utente (case-insensitive)
2. Se esiste, apre quello esistente
3. Altrimenti, valida l'utente via WebSocket prima di creare

**Implementazione**:
```rust
// 1. Verifica duplicati (case-insensitive)
let username = dm_username.trim().to_string();

let existing_dm = state.conversations.as_ref().and_then(|convs| {
    convs.iter().find(|conv| {
        conv.kind == "dm" && conv.title.to_lowercase() == username.to_lowercase()
    })
});

// 2. Se esiste: apri conversazione esistente
if let Some(existing_conv) = existing_dm {
    let _ = state.ui_tx.send(UiEvent::Opened(existing_conv.id));
    dm_username.clear();
    *show_create_dm_popup = false;
} 
// 3. Se non esiste: valida utente via WebSocket
else {
    // Usa request_dm_creation che valida l'utente via WebSocket
    // Lo stub verrà creato automaticamente se l'utente esiste
    state.request_dm_creation(username);
    dm_username.clear();
    *show_create_dm_popup = false;
}
```

**Stato Durante Validazione**:
```rust
// Se stiamo verificando l'utente, mostra spinner
if state.is_checking_user() {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(
            RichText::new(format!(
                "Verifica '{}'...",
                state.pending_user_check.as_ref().unwrap_or(&String::new())
            ))
                .size(14.0)
                .color(egui::Color32::from_rgb(200, 100, 40))
        );
    });
} else {
    // Bottone Crea (abilitato solo se username non vuoto)
    let can_create = !dm_username.trim().is_empty();
    let create_btn = egui::Button::new(...)
        .fill(if can_create {
            egui::Color32::from_rgb(200, 100, 40)
        } else {
            egui::Color32::from_gray(100)
        });
}
```

**Sequenza Completa**:
1. **Utente inserisce username** → campo testo attivo
2. **Click "Crea"** → verifica duplicati locali
3. **Se non duplicato** → `state.request_dm_creation(username)`
4. **WebSocket Check** → stato `is_checking_user()` = true
5. **UI aggiorna** → mostra spinner "Verifica 'username'..."
6. **Server risponde**:
   - ✅ Utente valido → crea stub DM locale, apre chat, toast "DM creato"
   - ❌ Utente invalido → toast errore "Utente non trovato"
7. **Cleanup** → `is_checking_user()` = false, popup si chiude

**Button Sizing**:
- Annulla: 120x36px
- Crea: 140x36px
- Border radius: 8.0px

---

### 5. Create Group Popup

**File**: `src/ui/modals/conversation_popups/create_group.rs`

**Dimensioni**: 500x520px, centrato

**Layout**:
```
┌────────────────────────────────────────┐
│        👥 Crea Nuovo Gruppo            │
│                                        │
│  📝 Nome del gruppo *                  │
│     [Es: Team Alpha]                   │
│                                        │
│  🔍 Cerca o aggiungi utenti            │
│     [Cerca nei contatti...]  [+ Add]  │
│     💡 'mario' non trovato, aggiungi  │
│                                        │
│  ┌──────────────────────────────────┐ │
│  │ ✅ Mario (selezionato)           │ │
│  │    Luigi                          │ │
│  │    Giovanni                       │ │
│  └──────────────────────────────────┘ │
│                                        │
│  Partecipanti aggiunti                 │
│  ┌──────────────────────────────────┐ │
│  │ [x] 👤 Mario                     │ │
│  │ [x] 👤 Luigi                     │ │
│  └──────────────────────────────────┘ │
│                                        │
│  [  ✖ Annulla  ]    [ 💬 Crea Gruppo]│
└────────────────────────────────────────┘
```

**State Management**:
```rust
// In AppState
pub struct CreateGroupPopupState {
    pub group_name: String,
    pub search_query: String,
    pub selected_participants: HashSet<String>,
    pub pending_user_verification: Option<String>,
}
```

**Workflow**:

1. **Nome Gruppo** (obbligatorio):
   - Validazione: non vuoto
   - Border rosso se vuoto e changed

2. **Barra Ricerca con Bottone Aggiungi**:
   - Cerca nei DM esistenti (contatti)
   - Se username non trovato: mostra bottone "Aggiungi"
   - Click "Aggiungi" o Enter: verifica utente via `request_dm_creation`
   - Durante verifica: spinner
   - Se valido: aggiunge a contatti E seleziona

3. **Lista Contatti Disponibili**:
   - ScrollArea (max 150px)
   - Filtra in base a search query
   - Checkbox per selezione
   - Selected: background orange
   - Escludi membri già selezionati

4. **Lista Partecipanti Selezionati**:
   - ScrollArea fisso (100px, sempre scrollbar visible)
   - Badge orange per ogni utente
   - Bottone [x] per rimuovere
   - Empty state: "Nessun partecipante aggiunto"

5. **Bottone Crea**:
   - Enabled solo se nome non vuoto
   - Click: chiama `state.create_group_with_participants()`

**Add Button Logic**:
```rust
let show_add_button = !search_text.is_empty()
    && !found_in_contacts
    && !state.create_group_popup.selected_participants.contains(&search_text);

if show_add_button {
    ui.label("💡 '{}' non trovato nei contatti, clicca Aggiungi o premi Invio".orange());
    
    // Bottone Aggiungi
    if add_button.clicked() || (search_response.lost_focus() && enter_pressed) {
        state.create_group_popup.pending_user_verification = Some(search_text.clone());
        state.request_dm_creation(search_text.clone());
        state.create_group_popup.search_query.clear();
    }
}
```

---

### 6. Invite Popup

**File**: `src/ui/modals/conversation_popups/invite.rs`

**Dimensioni**: 500x520px, centrato

**Parametro**: `cid: Uuid` (conversazione gruppo)

**Layout** (quasi identico a Create Group, ma senza nome):
```
┌────────────────────────────────────────┐
│      👤 Aggiungi Membri al Gruppo      │
│  Seleziona gli utenti da invitare     │
│                                        │
│  🔍 Cerca o aggiungi utenti            │
│     [Cerca nei contatti...]  [+ Add]  │
│                                        │
│  ┌──────────────────────────────────┐ │
│  │ ✅ Mario (selezionato)           │ │
│  │    Luigi                          │ │
│  └──────────────────────────────────┘ │
│                                        │
│  Utenti da invitare                    │
│  ┌──────────────────────────────────┐ │
│  │ [x] 👤 Mario                     │ │
│  │ [x] 👤 Luigi                     │ │
│  └──────────────────────────────────┘ │
│                                        │
│  [  ✖ Annulla  ]   [ ✉ Invita Utenti]│
└────────────────────────────────────────┘
```

**Differenze da Create Group**:
1. Recupera membri già presenti nel gruppo:
```rust
let existing_members: HashSet<String> = state.members_list
    .get(&cid)
    .map(|members| members.iter().map(|m| m.username.clone()).collect())
    .unwrap_or_default();
```

2. Non mostra bottone "Aggiungi" se username è già membro

3. Bottone finale chiama:
```rust
if invite_button.clicked() {
    let users: Vec<String> = state.invite_popup.selected_users.iter().cloned().collect();
    state.send_invite_users(cid, users);
    state.invite_popup.reset();
}
```

---

### 7. Delete Confirmation Popup

**File**: `src/ui/modals/conversation_popups/delete_confirmation.rs`

**Trigger**: `state.pending_deletion.is_some()`

**Dimensioni**: 450x240px, centrato

**Context-Aware Messages**: Il popup ha **4 casi distinti** in base al tipo di conversazione e al ruolo dell'utente.

#### Logica di Determinazione

```rust
let is_stub = state.is_dm_stub(conversation.id);
let is_owner = state.user_id.map_or(false, |uid| uid == conversation.owner_id);

let (icon, title, main_message, detail_message, confirm_label) = 
    if is_stub {
        // CASO 1: DM Stub (locale non sincronizzato)
    } else if conversation.kind == "group" {
        if is_owner {
            // CASO 2: Gruppo - Utente è Owner
        } else {
            // CASO 3: Gruppo - Utente NON è Owner
        }
    } else {
        // CASO 4: DM Standard
    };
```

#### CASO 1: DM Stub (Locale)

```rust
icon: egui_remixicon::icons::DELETE_BIN_FILL
title: "Elimina chat locale"
main_message: format!("Eliminare la chat privata \"{}\"?", conversation.title)
detail_message: "Si tratta di uno stub locale: verrà semplicemente rimosso dalla tua lista."
confirm_label: "Elimina"
```

**Layout**:
```
┌────────────────────────────────────────┐
│     🗑️ Elimina chat locale             │
│                                        │
│  Eliminare la chat privata "Mario"?   │
│                                        │
│  Si tratta di uno stub locale:         │
│  verrà semplicemente rimosso dalla     │
│  tua lista.                            │
│                                        │
│  [  ✖ Annulla  ]      [ 🗑️ Elimina ] │
└────────────────────────────────────────┘
```

#### CASO 2: Gruppo - Owner

```rust
icon: egui_remixicon::icons::DELETE_BIN_FILL
title: "Elimina gruppo"
main_message: format!("Eliminare il gruppo \"{}\"?", conversation.title)
detail_message: "Attenzione: eliminando il gruppo, questo verrà rimosso per TUTTI i partecipanti. L'azione è irreversibile."
confirm_label: "Elimina per tutti"
```

**Layout**:
```
┌────────────────────────────────────────┐
│        🗑️ Elimina gruppo               │
│                                        │
│  Eliminare il gruppo "Team Alpha"?    │
│                                        │
│  Attenzione: eliminando il gruppo,     │
│  questo verrà rimosso per TUTTI i      │
│  partecipanti. L'azione è irreversibile│
│                                        │
│  [  ✖ Annulla  ]  [ 🗑️ Elimina per tutti ]│
└────────────────────────────────────────┘
```

#### CASO 3: Gruppo - Non Owner

```rust
icon: egui_remixicon::icons::LOGOUT_BOX_LINE
title: "Esci dal gruppo"
main_message: format!("Uscire dal gruppo \"{}\"?", conversation.title)
detail_message: "Uscirai dal gruppo e non potrai più vedere i messaggi. Potrai rientrare solo se verrai invitato nuovamente."
confirm_label: "Esci dal gruppo"
```

**Layout**:
```
┌────────────────────────────────────────┐
│      🚪 Esci dal gruppo                 │
│                                        │
│  Uscire dal gruppo "Team Beta"?       │
│                                        │
│  Uscirai dal gruppo e non potrai più  │
│  vedere i messaggi. Potrai rientrare  │
│  solo se verrai invitato nuovamente.  │
│                                        │
│  [  ✖ Annulla  ]  [ 🚪 Esci dal gruppo ]│
└────────────────────────────────────────┘
```

#### CASO 4: DM Standard

```rust
icon: egui_remixicon::icons::DELETE_BIN_FILL
title: "Elimina conversazione"
main_message: format!("Eliminare la chat privata \"{}\"?", conversation.title)
detail_message: "L'eliminazione rimuoverà definitivamente la conversazione. L'azione è irreversibile."
confirm_label: "Elimina"
```

**Layout**:
```
┌────────────────────────────────────────┐
│     🗑️ Elimina conversazione           │
│                                        │
│  Eliminare la chat privata "Giovanni"?│
│                                        │
│  L'eliminazione rimuoverà              │
│  definitivamente la conversazione.     │
│  L'azione è irreversibile.             │
│                                        │
│  [  ✖ Annulla  ]      [ 🗑️ Elimina ] │
└────────────────────────────────────────┘
```

#### Actions

```rust
// Annulla
if cancel_button.clicked() {
    state.cancel_delete_confirmation();
}

// Conferma
if confirm_button.clicked() {
    state.execute_pending_deletion();
}
```

**Button Styling**:
- Annulla: width 140px, height 36px, color: inactive_bg_fill
- Conferma: width 160px, height 36px, color: orange `rgb(200, 100, 40)`
- Spacing: Annulla a sinistra, Conferma a destra con `Layout::right_to_left`

---

## Sistema Toast

**File**: `src/ui/components/toast_renderer.rs`

**Struttura**:
```rust
pub struct ToastRenderer;

impl ToastRenderer {
    pub fn render(&self, ctx: &Context, state: &mut AppState)
}
```

### Caratteristiche

- **Posizione**: Top-right con padding
- **Stacking**: Verticale, toast più recenti in cima
- **Animazione**: Slide-in da destra con easing cubic
- **Chiusura**: Automatica dopo 5s o manuale con bottone X
- **Colori**: Adaptive dark/light mode

### Costanti

```rust
const TOP_MARGIN: f32 = 100.0;
const SLIDE_IN_DURATION: f32 = 0.7;  // secondi
const SLIDE_IN_OFFSET: f32 = 40.0;   // px
const RIGHT_PADDING: f32 = 48.0;
const TOAST_SPACING: f32 = 8.0;
const MAX_WIDTH: f32 = 200.0;
```

### Colori per Tipo

**Info - Dark Mode**:
```rust
bg: rgb(28, 100, 28)
text: rgb(240, 255, 240)
icon: INFORMATION_LINE
icon_color: rgb(120, 220, 120)
```

**Info - Light Mode**:
```rust
bg: rgb(225, 245, 225)
text: rgb(20, 80, 20)
icon: INFORMATION_LINE
icon_color: rgb(30, 130, 30)
```

**Error - Dark Mode**:
```rust
bg: rgb(120, 35, 35)
text: rgb(255, 240, 240)
icon: ERROR_WARNING_LINE
icon_color: rgb(255, 120, 120)
```

**Error - Light Mode**:
```rust
bg: rgb(200, 80, 80)
text: rgb(255, 255, 255)
icon: ERROR_WARNING_LINE
icon_color: rgb(255, 230, 230)
```

**Close Button Colors**:
```rust
// Dark Mode
close_color: rgba(220, 220, 220, 200)

// Light Mode
close_color: rgba(80, 80, 80, 180)
```

### Animazione Slide-In

```rust
let ttl = toast.created.elapsed().as_secs_f32();
let slide_t = (ttl / SLIDE_IN_DURATION).clamp(0.0, 1.0);
let slide_t = 1.0 - (1.0 - slide_t).powi(3);  // Ease-out cubic
let slide_offset = SLIDE_IN_OFFSET * (1.0 - slide_t);
let pos_y = TOP_MARGIN + y_offset - slide_offset;
```

### Rendering Loop

```rust
let mut y_offset: f32 = 0.0;
let mut to_remove = HashSet::new();

for toast in state.toasts.iter().rev() {  // Reverse: più recenti in cima
    // Calcola colori e animazione
    
    Area::new(toast_id)
        .order(Foreground)
        .anchor(RIGHT_TOP, vec2(-RIGHT_PADDING, pos_y))
        .show(ctx, |ui| {
            Frame::none()
                .fill(bg)
                .stroke(Stroke::new(1.0, from_black_alpha(30)))
                .rounding(8.0)
                .inner_margin(symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.set_max_width(MAX_WIDTH);
                    ui.horizontal(|ui| {
                        // Icona
                        ui.label(icon.size(18.0).color(icon_color));
                        
                        // Testo
                        ui.label(toast.message.color(text_color).wrap());
                        
                        // Bottone X
                        ui.with_layout(right_to_left, |ui| {
                            if ui.button(CLOSE_LINE.size(14.0)).clicked() {
                                to_remove.insert(toast.id);
                            }
                        });
                    });
                });
        });
    
    y_offset += response.rect.height() + TOAST_SPACING;
}

// Rimuovi toast chiusi
state.toasts.retain(|t| !to_remove.contains(&t.id));
```

### Repaint Optimization

```rust
if !state.toasts.is_empty() {
    let has_animating = state.toasts.iter().any(|t| {
        t.created.elapsed().as_secs_f32() < SLIDE_IN_DURATION
    });
    
    if has_animating {
        ctx.request_repaint_after(Duration::from_millis(16));  // 60 FPS
    }
}
```

---

## Palette Colori

### Colore Primario

```rust
const PRIMARY_ORANGE: Color32 = Color32::from_rgb(200, 100, 40);
```

**Uso**: Titoli, icone primarie, bottoni azione, selezioni

**Variazioni**:
- Hover: `linear_multiply(0.08)` → `rgb(16, 8, 3)`
- Selected: `linear_multiply(0.2)` → `rgb(40, 20, 8)`
- Disabled: `from_gray(100)`

### Altri Colori Tematici

**Rosso Errore**:
```rust
Color32::from_rgb(220, 60, 60)  // Allarme
Color32::from_rgb(200, 50, 50)  // Conferma delete
Color32::from_rgb(180, 50, 50)  // Kick member
Color32::from_rgb(180, 40, 40)  // Logout
```

**Verde Success**:
```rust
Color32::GREEN                   // WS connected, checkmarks
Color32::from_rgb(40, 180, 80)  // Password match success
Color32::from_rgb(60, 180, 60)  // Info message
```

**Giallo Warning**:
```rust
Color32::YELLOW                  // WS connecting, pending status
```

**Oro**:
```rust
Color32::from_rgb(255, 215, 0)  // Crown icon per owner
```

**Grigio/Weak**:
```rust
ui.visuals().weak_text_color()   // Secondary text
Color32::GRAY                     // Disabled states
Color32::from_rgb(140, 140, 140) // Date separators
```

### Adaptive Colors

I seguenti colori si adattano al tema dark/light:

```rust
ui.visuals().text_color()               // Testo principale
ui.visuals().weak_text_color()          // Testo secondario
ui.visuals().window_fill()              // Background panels
ui.visuals().extreme_bg_color()         // Search bar bg (dark)
ui.visuals().widgets.inactive.bg_fill   // Bottoni disabled
ui.visuals().error_fg_color             // Errori form
```

---

## State Management UI

### Local State (nel componente)

**Quando usare**:
- Stato che vive solo durante il rendering
- Variabili temporanee per calcoli
- Flag locali non persistiti

**Esempio**:
```rust
pub fn show_popup(ctx: &Context, state: &mut AppState) {
    let mut open = true;  // Local, non persiste
    
    Window::new("Popup")
        .open(&mut open)
        .show(ctx, |ui| {
            // ...
        });
    
    if !open {
        state.show_popup = false;  // Sincronizza con AppState
    }
}
```

### Persistent State (egui memory)

**Quando usare**:
- UI state che deve persistere tra frames
- Scroll positions
- View selections
- Anchor points

**API**:
```rust
// Write
ui.data_mut(|d| {
    d.insert_temp(Id::new("my_key"), value);
});

// Read
let value: Option<T> = ui.data_mut(|d| {
    d.get_temp(Id::new("my_key"))
});
```

**Esempi Reali**:

1. **Auth View Selection**:
```rust
let current_view = ui.data_mut(|d| {
    d.get_temp::<AuthView>(Id::new("auth_view"))
        .unwrap_or(AuthView::Login)
});

// Change view
ui.data_mut(|d| d.insert_temp(Id::new("auth_view"), AuthView::Register));
```

2. **Scroll Offset**:
```rust
let last_offset: f32 = ui.data_mut(|d|
    d.get_temp(ui.id().with("last_offset")).unwrap_or(0.0)
);

// Save new offset
ui.data_mut(|d| 
    d.insert_temp(ui.id().with("last_offset"), new_offset)
);
```

3. **Message Count Tracking**:
```rust
let last_message_count: usize = ui.data_mut(|d|
    d.get_temp(messages_count_id).unwrap_or(0)
);

// Detect new messages
if current_count > last_message_count {
    // New messages arrived
}

ui.data_mut(|d| d.insert_temp(messages_count_id, current_count));
```

### AppState Fields

**Quando usare**:
- Stato applicativo globale
- Dati persistiti o sincronizzati
- Modal visibility flags
- Input buffers

**UI-Specific Fields in AppState**:
```rust
// Modal visibility
pub show_account_modal: bool,
pub confirm_delete_account: bool,
pub show_create_group_modal: bool,
pub show_invite_popup: bool,
pub show_group_info_popup: bool,

// Popup states
pub create_group_popup: CreateGroupPopupState,
pub invite_popup: InvitePopupState,

// Pending actions
pub pending_deletion: Option<PendingDeletion>,
pub pending_message_deletion: Option<(Uuid, Uuid)>,
pub pending_member_kick: Option<(Uuid, Uuid, String)>,

// Input
pub input_text: String,

// Loading states
pub is_loading: bool,
pub is_initial_load_complete: bool,
pub is_loading_more: bool,
pub is_loading_more_conversations: bool,

// Toast
pub toasts: Vec<Toast>,
```

---

## Performance e Ottimizzazioni

### Immediate Mode Efficiency

**Caratteristiche**:
- UI non conserva widget tree tra frames
- Ogni frame ricrea completamente da zero
- AppState è single source of truth
- egui ottimizza internamente (caching, clipping)

**Vantaggi**:
- Semplice: No sincronizzazione UI ↔ State
- Predicibile: UI riflette sempre stato corrente
- No memory leaks UI-side

### Repaint Strategy

**Demand-Driven Updates**:
```rust
// WebSocket message arrived
ws_rx.recv() => {
    state.update_from_ws_message(msg);
    (state.egui_waker)();  // Force repaint
}

// UI event processed
state.drain_events() {
    for event in events {
        state.handle_event(event);
    }
    // Waker chiamato automaticamente se necessario
}
```

**Idle Optimization**:
- Se nessun input E nessuna animazione → NO repaint
- egui gestisce automaticamente

**Animation Repaints**:
```rust
// Toast animations
if has_animating_toasts {
    ctx.request_repaint_after(Duration::from_millis(16));  // 60 FPS
}

// Spinner
if state.is_loading {
    ctx.request_repaint();  // Continuo
}
```

### Virtual Scrolling

**ScrollArea Automatic**:
- egui implementa virtual scrolling automaticamente
- Renderizza solo elementi visibili
- Clipping hardware-accelerated

**Best Practices**:
```rust
ScrollArea::vertical()
    .auto_shrink([false, false])  // Non shrink
    .id_source("unique_id")       // ID stabile
    .show(ui, |ui| {
        for item in &items {
            render_item(ui, item);
        }
    });
```

### Data Management

**Message Caching**:
```rust
const MAX_MESSAGES_PER_CONVERSATION: usize = 500;

fn cleanup_old_data(&mut self) {
    for (conv_id, messages) in &mut self.cached_messages {
        if messages.len() > MAX_MESSAGES_PER_CONVERSATION {
            // Rimuovi messaggi più vecchi
            messages.drain(0..messages.len() - MAX_MESSAGES_PER_CONVERSATION);
        }
    }
}
```

**Lazy Loading**:
- Load more messages on scroll up
- Paginated conversations loading
- Anchor-based scroll preservation

### Memory Management

**Periodic Cleanup**:
```rust
fn periodic_cleanup(&mut self) {
    static mut LAST_CLEANUP: Option<Instant> = None;
    
    let should_cleanup = unsafe {
        LAST_CLEANUP.map_or(true, |last| {
            last.elapsed() > Duration::from_secs(300)  // 5 minuti
        })
    };
    
    if should_cleanup {
        self.state.cleanup_old_data();
        self.state.cleanup_expired_stubs();
        
        unsafe {
            LAST_CLEANUP = Some(Instant::now());
        }
    }
}
```

**Stub Expiration**:
```rust
pub fn cleanup_expired_stubs(&mut self) {
    let now = Instant::now();
    
    self.dm_stubs.retain(|_, stub| {
        now.duration_since(stub.created_at) < Duration::from_secs(30)
    });
    
    self.group_stubs.retain(|_, stub| {
        now.duration_since(stub.created_at) < Duration::from_secs(30)
    });
}
```

### Cloning Optimization

**When to Clone**:
- Quando serve ownership per async tasks
- Per evitare borrow checker issues nel loop UI

**Esempio Chat Messages**:
```rust
// Clone per evitare problemi con borrow di `s`
let messages = s.messages.clone();

for i in 0..messages.len() {
    let message = &messages[i];
    show_message(ui, s, message);  // `s` può essere borrowed qui
}
```

**Trade-off**:
- Pro: Semplifica borrow checker
- Con: Overhead clone
- Decision: Ok per liste moderate (<1000 items)

---

## Conclusioni

L'UI Layer di Rust Ruggine Chat implementa un'architettura immediate mode completa e robusta con:

**Architettura**:
- Pattern immediate mode con egui
- State-driven rendering completo
- Manager pattern per separazione responsabilità
- Organizzazione modulare chiara

**Componenti Principali**:
- HeaderManager: Identità app e status
- SidebarManager + ConversationsSidebar: Navigazione conversazioni
- Pages: Auth, Chat
- Modals: Account, DeleteAccount
- Popup Sistema: Action selection, Create DM/Group, Invite, Delete confirmation
- ToastRenderer: Notifiche animate

**Features Avanzate**:
- Scroll intelligente con anchor preservation
- Auto-load conversazioni e messaggi
- User validation flow con spinner
- Context-aware delete confirmation
- Date separators
- Message status indicators
- Dark/Light mode adaptive
- Responsive layout

**Performance**:
- Waker-driven updates
- Virtual scrolling automatico
- Periodic cleanup
- Animation optimization
- Cloning strategico

**UX**:
- Visual feedback consistente
- Loading indicators appropriati
- Error handling chiaro
- Keyboard shortcuts (Enter to send/submit)
- Context menus
- Hover effects
- Tooltips informativi

---

**Fine Documentazione UI Layer**

