# Manuale Progettista - Ruggine Chat

## Indice

1. [Introduzione](#introduzione)
2. [Architettura del Sistema](#architettura-del-sistema)
   - [Panoramica Generale](#panoramica-generale)
   - [Stack Tecnologico](#stack-tecnologico)
   - [Struttura delle Directory](#struttura-delle-directory)
3. [Backend (Server)](#backend-server)
   - [Architettura Layered](#architettura-layered)
   - [Database Schema](#database-schema)
   - [Autenticazione e Sicurezza](#autenticazione-e-sicurezza)
   - [WebSocket e Real-Time](#websocket-e-real-time)
   - [Event Sourcing e Sequencing](#event-sourcing-e-sequencing)
4. [Frontend (Client)](#frontend-client)
   - [Architettura MVVM](#architettura-mvvm)
   - [State Management](#state-management)
   - [WebSocket Manager](#websocket-manager)
   - [Event Dispatching](#event-dispatching)
5. [Flussi di Dati Principali](#flussi-di-dati-principali)
6. [API Reference](#api-reference)
7. [Deployment e Configurazione](#deployment-e-configurazione)
8. [Performance e Scalabilità](#performance-e-scalabilità)
9. [Testing](#testing)
10. [Estensioni Future](#estensioni-future)

---

## Introduzione

**Ruggine** è un'applicazione di messaggistica istantanea sviluppata interamente in **Rust**, utilizzando un'architettura client-server con comunicazione HTTP REST e WebSocket per la sincronizzazione in tempo reale.

### Obiettivi del Progetto

- Implementare un sistema di chat sicuro e performante
- Utilizzare tecnologie Rust moderne (Axum, egui, SQLx, tokio)
- Garantire consistenza dei dati tramite event sourcing e sequencing
- Fornire un'interfaccia grafica nativa cross-platform

### Caratteristiche Tecniche Principali

- **Backend**: Axum framework, SQLite con SQLx, JWT authentication, WebSocket con tokio-tungstenite
- **Frontend**: egui/eframe per GUI nativa, reqwest per HTTP, WebSocket client
- **Database**: SQLite con WAL mode, foreign keys, indices ottimizzati
- **Security**: Argon2 per password hashing, JWT con HS256, validazione server-side
- **Concurrency**: Async/await con tokio runtime, multi-threaded executor
- **Real-time**: WebSocket bidirectional, gap detection, automatic recovery

---

## Architettura del Sistema

### Panoramica Generale

```
┌─────────────────────────────────────────────────────────────┐
│                     CLIENT (GUI)                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │     UI       │  │    State     │  │   WS Manager │       │
│  │   (egui)     │  │  Management  │  │              │       │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘       │
│         │                 │                 │               │
│         └─────────────────┴─────────────────┘               │
│                           │                                 │
└───────────────────────────┼─────────────────────────────────┘
                            │
                    ┌───────┴───────┐
                    │   HTTP REST   │
                    │   WebSocket   │
                    └───────┬───────┘
                            │
┌───────────────────────────┼─────────────────────────────────┐
│                    SERVER (Axum)                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ Controllers  │  │   Services   │  │ Repositories │       │
│  │  (HTTP/WS)   │  │ (Business)   │  │  (Data)      │       │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘       │
│         │                 │                 │               │
│         └─────────────────┴─────────────────┘               │
│                           │                                 │
│         ┌─────────────────┴─────────────────┐               │
│         │         AppState                  │               │
│         │  - Active Connections (HashMap)   │               │
│         │  - Database Pool (SQLx)           │               │
│         │  - Config                         │               │
│         └───────────────────────────────────┘               │
└───────────────────────────┼─────────────────────────────────┘
                            │
┌───────────────────────────┼─────────────────────────────────┐
│                    DATABASE (SQLite)                         │
│  ┌──────────────┬──────────────┬──────────────┐             │
│  │    users     │conversations │   messages   │             │
│  ├──────────────┼──────────────┼──────────────┤             │
│  │participants  │user_events   │msg_sequences │             │
│  ├──────────────┼──────────────┼──────────────┤             │
│  │user_sequences│   invites    │              │             │
│  └──────────────┴──────────────┴──────────────┘             │
└─────────────────────────────────────────────────────────────┘
```

### Stack Tecnologico

#### Backend

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Web Framework | Axum | 0.7 | HTTP routing, middleware, extractors |
| Async Runtime | Tokio | 1.x | Multi-threaded async executor |
| Database | SQLite | 3.x | Embedded relational database |
| Database Driver | SQLx | 0.7 | Compile-time checked SQL queries |
| WebSocket | tokio-tungstenite | 0.24 | WebSocket protocol implementation |
| Authentication | jsonwebtoken | 9.x | JWT token generation/validation |
| Password Hashing | argon2 | 0.5 | Secure password hashing (Argon2id) |
| Serialization | serde + serde_json | 1.x | JSON serialization/deserialization |
| Logging | tracing + tracing-subscriber | 0.1 | Structured logging |
| Error Handling | anyhow + thiserror | 1.x | Error propagation and custom errors |
| HTTP Middleware | tower + tower-http | 0.5 | Middleware stack (CORS, logging) |
| UUID Generation | uuid | 1.x | Unique identifier generation |
| Date/Time | chrono | 0.4 | Timestamp handling |

#### Frontend

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| GUI Framework | eframe + egui | 0.27 | Immediate mode GUI |
| HTTP Client | reqwest | 0.12 | REST API communication |
| WebSocket | tokio-tungstenite | 0.23 | Real-time communication |
| Async Runtime | tokio | 1.x | Async operations |
| Icons | egui-remixicon | 0.27.2 | Icon library for UI |
| Serialization | serde + serde_json | 1.x | JSON handling |
| Logging | tracing + env_logger | 0.11 | Client-side logging |

### Struttura delle Directory

```
G39/
├── client/                    # Frontend application
│   ├── src/
│   │   ├── api/              # API client modules
│   │   │   ├── auth.rs       # Authentication API calls
│   │   │   ├── chat.rs       # Message API calls
│   │   │   ├── conversation.rs # Conversation API calls
│   │   │   └── ws.rs         # WebSocket client
│   │   ├── app/              # Application core
│   │   │   ├── app.rs        # Main app struct
│   │   │   ├── events/       # Event dispatching system
│   │   │   │   ├── dispatcher.rs
│   │   │   │   ├── auth_handler.rs
│   │   │   │   ├── message_handler.rs
│   │   │   │   ├── conversation_handler.rs
│   │   │   │   ├── sequence_handler.rs
│   │   │   │   └── websocket_handler.rs
│   │   │   └── ws_manager/   # WebSocket lifecycle management
│   │   │       ├── ws_manager.rs
│   │   │       ├── health_monitor.rs
│   │   │       ├── rate_limiter.rs
│   │   │       └── message_processor.rs
│   │   ├── state/            # State management
│   │   │   ├── core.rs       # AppState (conversations, messages)
│   │   │   ├── ui.rs         # UIState (modals, toasts)
│   │   │   └── commands.rs   # Async commands
│   │   ├── ui/               # User interface
│   │   │   ├── pages/        # Page components
│   │   │   ├── layout/       # Layout components (header, sidebar)
│   │   │   └── modals/       # Modal dialogs
│   │   ├── models.rs         # Data models (DTOs)
│   │   └── main.rs           # Entry point
│   └── Cargo.toml
│
├── server/                    # Backend application
│   ├── src/
│   │   ├── auth/             # JWT authentication
│   │   │   └── jwt.rs
│   │   ├── controllers/      # HTTP request handlers
│   │   │   ├── user_controller.rs
│   │   │   ├── conversation_controller.rs
│   │   │   ├── message_controller.rs
│   │   │   └── invite_controller.rs
│   │   ├── services/         # Business logic layer
│   │   │   ├── user_service.rs
│   │   │   ├── conversation_service.rs
│   │   │   ├── message_service.rs
│   │   │   ├── invite_service.rs
│   │   │   └── partecipant.rs
│   │   ├── repositories/     # Data access layer
│   │   │   ├── user_repo.rs
│   │   │   ├── conversation_repo.rs
│   │   │   ├── message_repo.rs
│   │   │   └── invite_repo.rs
│   │   ├── routers/          # Route definitions
│   │   │   ├── user_route.rs
│   │   │   ├── conversation_route.rs
│   │   │   └── message_route.rs
│   │   ├── web_socket/       # WebSocket handling
│   │   │   ├── actor.rs      # ConnectionActor (main WS logic)
│   │   │   ├── reader.rs     # Message reading
│   │   │   ├── broadcast.rs  # Broadcasting logic
│   │   │   ├── initial_state.rs
│   │   │   └── handlers/     # Message type handlers
│   │   │       ├── router.rs
│   │   │       ├── message.rs
│   │   │       ├── conversation.rs
│   │   │       ├── group.rs
│   │   │       └── user.rs
│   │   ├── models.rs         # Database models
│   │   ├── state.rs          # AppState definition
│   │   ├── config.rs         # Configuration
│   │   ├── db.rs             # Database initialization
│   │   ├── error.rs          # Error types
│   │   ├── cpu_logger.rs     # CPU monitoring
│   │   └── main.rs           # Server entry point
│   ├── migrations/           # SQLx database migrations
│   │   ├── 20241210_initial_schema.sql
│   │   └── ...
│   ├── benches/              # Performance benchmarks
│   └── Cargo.toml
│
├── MANUALE_UTENTE.md         # User manual
├── MANUALE_PROGETTISTA.md    # This file
└── README.md                 # Project overview
```

---

## Backend (Server)

### Architettura Layered

Il server segue un'architettura a **3 livelli** (three-tier architecture):

```
┌────────────────────────────────────────────────────────────┐
│                     PRESENTATION LAYER                      │
│  ┌──────────────────┐          ┌──────────────────┐        │
│  │  HTTP Routes     │          │  WebSocket       │        │
│  │  (Controllers)   │          │  Handler         │        │
│  └────────┬─────────┘          └────────┬─────────┘        │
└───────────┼──────────────────────────────┼─────────────────┘
            │                              │
┌───────────┼──────────────────────────────┼─────────────────┐
│           ▼        BUSINESS LAYER        ▼                 │
│  ┌──────────────────────────────────────────────┐          │
│  │             Services                         │          │
│  │  - UserService                               │          │
│  │  - ConversationService                       │          │
│  │  - MessageService                            │          │
│  │  - InviteService                             │          │
│  └────────────────────┬─────────────────────────┘          │
└───────────────────────┼────────────────────────────────────┘
                        │
┌───────────────────────┼────────────────────────────────────┐
│                       ▼       DATA LAYER                   │
│  ┌──────────────────────────────────────────────┐          │
│  │           Repositories                       │          │
│  │  - UserRepo                                  │          │
│  │  - ConversationRepo                          │          │
│  │  - MessageRepo                               │          │
│  │  - InviteRepo                                │          │
│  └────────────────────┬─────────────────────────┘          │
│                       │                                    │
│                       ▼                                    │
│  ┌──────────────────────────────────────────────┐          │
│  │          SQLite Database                     │          │
│  │  - SQLx Pool (compile-time checked)          │          │
│  └──────────────────────────────────────────────┘          │
└────────────────────────────────────────────────────────────┘
```

#### Controllers (Presentation Layer)

Gestiscono le richieste HTTP e validano l'input.

**Esempio: User Controller**

```rust
// server/src/controllers/user_controller.rs
use axum::{extract::State, Json};
use crate::models::{RegisterReq, LoginReq, LoginResp};
use crate::services::user_service::UserService;
use crate::state::AppState;
use crate::error::AppError;

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<IdResp>, AppError> {
    let user_id = UserService::register(&state.db, &req).await?;
    Ok(Json(IdResp { id: user_id }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>, AppError> {
    let resp = UserService::login(&state.db, &req).await?;
    Ok(Json(resp))
}
```

#### Services (Business Logic Layer)

Contengono la logica di business e orchestrano le operazioni.

**Esempio: User Service**

```rust
// server/src/services/user_service.rs
use sqlx::SqlitePool;
use uuid::Uuid;
use argon2::{Argon2, PasswordHash, PasswordVerifier, PasswordHasher};
use argon2::password_hash::SaltString;
use crate::models::{RegisterReq, LoginReq, LoginResp};
use crate::auth::jwt::create_token;
use crate::repositories::user_repo::UserRepo;
use crate::error::AppError;

pub struct UserService;

impl UserService {
    pub async fn register(db: &SqlitePool, req: &RegisterReq) -> Result<Uuid, AppError> {
        // Validation
        if req.username.is_empty() || req.password.len() < 4 {
            return Err(AppError::BadRequest("Invalid input".into()));
        }

        // Check if username exists
        if UserRepo::find_by_username(db, &req.username).await?.is_some() {
            return Err(AppError::Conflict("Username already exists".into()));
        }

        // Hash password
        let salt = SaltString::generate(&mut rand::thread_rng());
        let argon2 = Argon2::default();
        let pass_hash = argon2.hash_password(req.password.as_bytes(), &salt)
            .map_err(|_| AppError::InternalServerError)?
            .to_string();

        // Create user
        let user_id = Uuid::new_v4();
        UserRepo::create(db, user_id, &req.username, &pass_hash).await?;

        Ok(user_id)
    }

    pub async fn login(db: &SqlitePool, req: &LoginReq) -> Result<LoginResp, AppError> {
        // Find user
        let user = UserRepo::find_by_username(db, &req.username).await?
            .ok_or(AppError::Unauthorized)?;

        // Verify password
        let parsed_hash = PasswordHash::new(&user.pass_hash)
            .map_err(|_| AppError::InternalServerError)?;
        Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .map_err(|_| AppError::Unauthorized)?;

        // Generate JWT
        let token = create_token(&user.username, user.id)?;

        // Get user sequence
        let last_sequence = UserRepo::get_user_sequence(db, user.id).await?;

        Ok(LoginResp {
            token,
            user_id: user.id,
            username: user.username,
            last_sequence,
        })
    }
}
```

#### Repositories (Data Access Layer)

Gestiscono le query al database.

**Esempio: User Repository**

```rust
// server/src/repositories/user_repo.rs
use sqlx::{SqlitePool, query_as, query};
use uuid::Uuid;
use crate::models::User;
use crate::error::AppError;

pub struct UserRepo;

impl UserRepo {
    pub async fn find_by_username(
        db: &SqlitePool,
        username: &str
    ) -> Result<Option<User>, AppError> {
        let user = query_as!(
            User,
            "SELECT id as \"id: Uuid\", username, pass_hash, created_at
             FROM users WHERE username = ?",
            username
        )
        .fetch_optional(db)
        .await?;

        Ok(user)
    }

    pub async fn create(
        db: &SqlitePool,
        id: Uuid,
        username: &str,
        pass_hash: &str,
    ) -> Result<(), AppError> {
        let id_str = id.to_string();
        let now = chrono::Utc::now().timestamp();

        query!(
            "INSERT INTO users (id, username, pass_hash, created_at)
             VALUES (?, ?, ?, ?)",
            id_str, username, pass_hash, now
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn delete(db: &SqlitePool, user_id: Uuid) -> Result<(), AppError> {
        let id_str = user_id.to_string();
        query!("DELETE FROM users WHERE id = ?", id_str)
            .execute(db)
            .await?;
        Ok(())
    }
}
```

### Database Schema

#### logic Diagram

```
                        ┌─────────────────┐
                        │     USERS       │
                        │─────────────────│
                        │ id (PK)         │
                        │ username (UQ)   │
                        │ pass_hash       │
                        │ created_at      │
                        └────────▲────────┘
                                 │
          ┌──────────────────────┼─────────┬────────────┐
          │                      │         |            │
          │                      │         |            │
          │                      │         |            │
  ┌───────────────┐      ┌──────────────┐  |    ┌──────────────┐
  │CONVERSATIONS  │◄─────┤PARTICIPANTS  │  |    │  MESSAGES    │
  │───────────────│      │──────────────│  |    │──────────────│
  │ id (PK)       │      │conversation_id│ |    │ id (PK)      │
  │ kind          │      │user_id       │  |    │conversation_id│
  │ title         │      │role          │  |    │author_id     │
  │ owner_id (FK) │      │last_read_seq │  |    │content       │
  │ created_at    │      └──────────────┘  |    │sequence_num  │
  └───▲───────────┘◄───────────────────────|────└──────────────┘
      │                                    |
      │                                    │
      │                                    │
      │                                    │        
      ├─────────────┬──────────┐  ┌──────────────┐
      |             |          |  |              |            
┌─────────────┐ ┌────────┐  ┌────────────┐ ┌──────────┐ 
│MESSAGE_SEQ  │ │INVITES │  │USER_EVENTS │ │USER_SEQ  │ 
│─────────────│ │────────│  │────────────│ │──────────│
│conv_id      | │conv_id │  │user_id     │ │user_id   │
│last_updated │ │token   │  │sequence_num│ │current_  │
│current_seq  │ │expires │  │event_type  │ │  sequence│
└─────────────┘ └────────┘  │conv_id (FK)│ └──────────┘
                            └────────────┘

Relazioni principali:
─────────────────────
• USERS 1:N CONVERSATIONS (owner_id) - Un utente possiede più gruppi
• USERS N:M CONVERSATIONS via PARTICIPANTS - Partecipazione ai gruppi/DM
• USERS 1:N MESSAGES (author_id) - Un utente scrive più messaggi
• CONVERSATIONS 1:N MESSAGES - Una conversazione contiene più messaggi
• CONVERSATIONS 1:1 MESSAGE_SEQUENCES - Sequenza messaggi per conversation
• USERS 1:1 USER_SEQUENCES - Sequenza eventi per utente
• USERS 1:N USER_EVENTS - Eventi utente (event sourcing)

ON DELETE CASCADE:
DELETE users → participants, messages, conversations(owned), user_sequences, user_events
DELETE conversations → participants, messages, message_sequences, invites
```

#### Schema SQL

```sql
-- Users table
CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,  -- UUID
    username TEXT UNIQUE NOT NULL,
    pass_hash TEXT NOT NULL,       -- Argon2 hash
    created_at INTEGER NOT NULL    -- UNIX timestamp
);

-- Conversations table
CREATE TABLE conversations (
    id TEXT PRIMARY KEY NOT NULL,  -- UUID
    kind TEXT NOT NULL CHECK(kind IN ('dm', 'group')),
    title TEXT,                    -- NULL for DMs
    owner_id TEXT,                 -- NULL for DMs
    created_at INTEGER NOT NULL,
    FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Participants table (many-to-many)
CREATE TABLE participants (
    conversation_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('owner', 'member')),
    last_read_msg TEXT,            -- UUID of last read message
    last_read_sequence INTEGER DEFAULT 0,
    PRIMARY KEY (conversation_id, user_id),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Messages table
CREATE TABLE messages (
    id TEXT PRIMARY KEY NOT NULL,  -- UUID
    conversation_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    sequence_num INTEGER,          -- Per-conversation sequence
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (author_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Message sequences (per-conversation)
CREATE TABLE message_sequences (
    conversation_id TEXT PRIMARY KEY NOT NULL,
    current_sequence INTEGER DEFAULT 0,
    last_updated INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- User events (event sourcing)
CREATE TABLE user_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    sequence_num INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    event_data TEXT NOT NULL,      -- JSON
    conversation_id TEXT,
    created_at INTEGER NOT NULL,
    delivered INTEGER DEFAULT 0,   -- Boolean
    UNIQUE(user_id, sequence_num)
);

-- User sequences
CREATE TABLE user_sequences (
    user_id TEXT PRIMARY KEY NOT NULL,
    current_sequence INTEGER DEFAULT 0,
    last_ping_sequence INTEGER DEFAULT 0,
    last_ping_at INTEGER DEFAULT 0,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Invites table
CREATE TABLE invites (
    id TEXT PRIMARY KEY NOT NULL,  -- UUID
    conversation_id TEXT NOT NULL,
    token TEXT UNIQUE NOT NULL,
    expires_at INTEGER NOT NULL,
    used INTEGER DEFAULT 0,        -- Boolean (0 or 1)
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- Indices for performance
CREATE INDEX idx_msgs_conv_ts ON messages(conversation_id, created_at);
CREATE INDEX idx_msgs_conv_seq ON messages(conversation_id, sequence_num);
CREATE UNIQUE INDEX idx_msgs_conv_seq_unique
    ON messages(conversation_id, sequence_num)
    WHERE sequence_num IS NOT NULL;

CREATE INDEX idx_participants_last_read
    ON participants(user_id, conversation_id, last_read_sequence);

CREATE INDEX idx_user_events_seq ON user_events(user_id, sequence_num);
CREATE INDEX idx_user_events_undelivered
    ON user_events(user_id, delivered)
    WHERE delivered = 0;
CREATE INDEX idx_user_events_cleanup
    ON user_events(created_at)
    WHERE delivered = 1;
```

#### Foreign Key Constraints

```
users
  ↓ ON DELETE CASCADE
  ├─→ participants (user_id)
  ├─→ messages (author_id)
  ├─→ conversations (owner_id)
  └─→ user_sequences (user_id)

conversations
  ↓ ON DELETE CASCADE
  ├─→ participants (conversation_id)
  ├─→ messages (conversation_id)
  ├─→ message_sequences (conversation_id)
  └─→ invites (conversation_id)
```

Quando un utente elimina il proprio account:
1. Vengono eliminati tutti i `participants` records
2. Vengono eliminati tutti i `messages` authored dall'utente
3. Vengono eliminate tutte le `conversations` di cui è owner
4. Vengono eliminati i `user_sequences`

### Autenticazione e Sicurezza

#### Password Hashing con Argon2

```rust
use argon2::{
    Argon2,
    PasswordHash,
    PasswordHasher,
    PasswordVerifier,
    password_hash::SaltString,
};

// Hashing during registration
let salt = SaltString::generate(&mut rand::thread_rng());
let argon2 = Argon2::default();  // Argon2id, m=65536, t=3, p=4
let pass_hash = argon2
    .hash_password(password.as_bytes(), &salt)?
    .to_string();

// Verification during login
let parsed_hash = PasswordHash::new(&stored_hash)?;
Argon2::default().verify_password(password.as_bytes(), &parsed_hash)?;
```

**Parametri Argon2**:
- **Algorithm**: Argon2id (hybrid mode)
- **Memory**: 64 MB (m=65536 KiB)
- **Iterations**: 3 (t=3)
- **Parallelism**: 4 threads (p=4)

#### JWT Authentication

**Token Generation:**

```rust
// server/src/auth/jwt.rs
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey, Algorithm};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // username
    pub uid: String,      // user_id (UUID)
    pub exp: i64,         // expiry timestamp
}

pub fn create_token(username: &str, user_id: Uuid) -> Result<String, AppError> {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "default-secret-key-change-in-production".to_string());

    let expiry = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .unwrap()
        .timestamp();

    let claims = Claims {
        sub: username.to_string(),
        uid: user_id.to_string(),
        exp: expiry,
    };

    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

pub fn verify_token(token: &str) -> Result<Claims, AppError> {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "default-secret-key-change-in-production".to_string());

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;

    Ok(token_data.claims)
}
```

**Token Structure:**

```json
{
  "header": {
    "alg": "HS256",
    "typ": "JWT"
  },
  "payload": {
    "sub": "alice",
    "uid": "550e8400-e29b-41d4-a716-446655440000",
    "exp": 1704153600
  },
  "signature": "..."
}
```

**AuthUser Extractor:**

```rust
// server/src/auth/extractor.rs
use axum::{
    extract::FromRequestParts,
    http::request::Parts,
};

pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract Authorization header
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        // Parse "Bearer <token>"
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;

        // Verify JWT
        let claims = verify_token(token)?;

        // Parse UUID
        let user_id = Uuid::parse_str(&claims.uid)
            .map_err(|_| AppError::Unauthorized)?;

        // Verify user exists in database
        let app_state = State::<AppState>::from_request_parts(parts, state).await?;
        let user = UserRepo::find_by_id(&app_state.db, user_id).await?
            .ok_or(AppError::Unauthorized)?;

        Ok(AuthUser {
            id: user.id,
            username: user.username,
        })
    }
}
```

**Usage in Controllers:**

```rust
pub async fn delete_account(
    user: AuthUser,  // Automatically validated
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    UserService::delete_user(&state.db, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

### WebSocket e Real-Time

#### WebSocket Connection Flow

```
CLIENT                           SERVER
  │                                 │
  │  GET /ws?session_id=uuid        │
  │  Authorization: Bearer <token>  │
  ├────────────────────────────────>│
  │                                 │
  │         <Upgrade to WS>         │
  │<────────────────────────────────┤
  │                                 │
  │      ConnectionActor Created    │
  │         initial_state sent      │
  │<────────────────────────────────┤
  │                                 │
  │  {"type":"subscribe"}           │
  ├────────────────────────────────>│
  │                                 │
  │  {"type":"ChatMessage",...}     │
  ├────────────────────────────────>│
  │                                 │
  │  {"type":"ChatMessageReceived"} │
  │<────────────────────────────────┤
  │                                 │
  │  {"type":"Ping","user_seq":42}  │
  ├────────────────────────────────>│
  │                                 │
  │  {"type":"PongReceived",...}    │
  │<────────────────────────────────┤
  │                                 │
```

#### ConnectionActor

Ogni connessione WebSocket ha un `ConnectionActor` dedicato che gestisce:

```rust
// server/src/web_socket/actor.rs
pub struct ConnectionActor {
    pub user_id: Uuid,
    pub username: String,
    pub session_id: Uuid,
    pub sender: Sender<ServerMessage>,     // To send to client
    pub state: Arc<AppState>,
    pub last_ping_sequence: i64,
}

impl ConnectionActor {
    pub async fn run(
        self,
        socket: WebSocket,
        db: SqlitePool,
    ) {
        let (ws_sender, ws_receiver) = socket.split();

        // Register in active connections
        self.state.connections.write().await
            .insert(self.user_id, self.sender.clone());

        // Spawn reader task
        let reader_handle = tokio::spawn(read_messages(
            ws_receiver,
            self.sender.clone(),
        ));

        // Spawn writer task
        let writer_handle = tokio::spawn(write_messages(
            ws_sender,
            self.receiver,
        ));

        // Handle incoming messages
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg).await;
        }

        // Cleanup on disconnect
        self.state.connections.write().await.remove(&self.user_id);
    }

    async fn handle_message(&mut self, msg: ClientMessage) {
        match msg {
            ClientMessage::ChatMessage { cid, content, client_msg_id } => {
                handlers::message::handle_chat_message(
                    &self.state,
                    self.user_id,
                    &self.username,
                    cid,
                    content,
                    client_msg_id,
                ).await;
            }
            ClientMessage::Ping { user_sequence } => {
                self.last_ping_sequence = user_sequence;
                handlers::user::handle_ping(&self.state, self.user_id, user_sequence).await;
            }
            // ... other message types
        }
    }
}
```

#### Broadcasting

```rust
// server/src/web_socket/broadcast.rs
pub async fn broadcast_to_conversation(
    state: &AppState,
    conversation_id: Uuid,
    message: ServerMessage,
    exclude_user: Option<Uuid>,
) -> Result<(), AppError> {
    // Get all participants
    let participants = ConversationRepo::get_participants(&state.db, conversation_id).await?;

    let connections = state.connections.read().await;

    for participant in participants {
        if Some(participant.user_id) == exclude_user {
            continue;
        }

        if let Some(sender) = connections.get(&participant.user_id) {
            let _ = sender.send(message.clone()).await;
        }
    }

    Ok(())
}
```

#### Message Types

**Client → Server:**

```rust
#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Subscribe,
    Ping { user_sequence: i64 },
    ChatMessage { cid: Uuid, content: String, client_msg_id: Uuid },
    DeleteMessage { mid: Uuid },
    DeleteConversation { cid: Uuid },
    LeaveGroup { cid: Uuid },
    // Note: Typing non implementato lato server
    MarkRead { conversation_id: Uuid, sequence_num: i64 },
    RequestUserResume { from_sequence: i64, limit: i64 },
    RequestMessagesResume { conversation_id: Uuid, from_sequence: i64, limit: i64 },
}
```

**Server → Client:**

```rust
#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum ServerMessage {
    InitialState { conversations: Vec<...>, messages: HashMap<...> },
    PongReceived { server_sequence: i64, gaps_detected: bool },
    ChatMessageReceived { id: Uuid, conversation_id: Uuid, author_id: Uuid, ... },
    MessageConfirmation { client_msg_id: Uuid, server_msg_id: Uuid, sequence: i64, status: String },
    MessageDeleted { message_id: Uuid, conversation_id: Uuid },
    ConversationDeleted { conversation_id: Uuid },
    UserJoined { conversation_id: Uuid, user_id: Uuid, username: String },
    UserLeft { conversation_id: Uuid, user_id: Uuid, username: String },
    UserKicked { conversation_id: Uuid, user_id: Uuid, username: String },
    // Note: UserTyping definito ma non implementato
    UserDeletedAccount { user_id: Uuid, username: String, conversation_ids: Vec<Uuid> },
    UserEventsResume { events: Vec<UserEvent> },
    MessagesResume { conversation_id: Uuid, messages: Vec<Message> },
}
```

### Event Sourcing e Sequencing

Il sistema implementa un **dual sequencing** pattern per garantire consistenza e recovery.

#### User-Level Sequencing

Ogni utente ha una sequenza incrementale di eventi:

```rust
// Insert user event
pub async fn insert_user_event(
    db: &SqlitePool,
    user_id: Uuid,
    event_type: &str,
    event_data: serde_json::Value,
    conversation_id: Option<Uuid>,
) -> Result<i64, AppError> {
    // Get next sequence number
    let seq = increment_user_sequence(db, user_id).await?;

    let user_id_str = user_id.to_string();
    let conv_id_str = conversation_id.map(|id| id.to_string());
    let event_data_str = event_data.to_string();
    let now = chrono::Utc::now().timestamp();

    query!(
        "INSERT INTO user_events
         (user_id, sequence_num, event_type, event_data, conversation_id, created_at, delivered)
         VALUES (?, ?, ?, ?, ?, ?, 0)",
        user_id_str, seq, event_type, event_data_str, conv_id_str, now
    )
    .execute(db)
    .await?;

    Ok(seq)
}
```

**Event Types:**

- `conversation_created`: Nuova conversazione creata
- `message_received`: Nuovo messaggio ricevuto
- `user_deleted_account`: Un utente ha eliminato il proprio account
- `conversation_deleted`: Una conversazione è stata eliminata
- `user_joined`: Un utente è entrato in un gruppo
- `user_left`: Un utente è uscito da un gruppo
- `user_kicked`: Un utente è stato espulso
- `owner_deleted`: L'owner di un gruppo ha eliminato l'account
- `message_deleted`: Un messaggio è stato eliminato

#### Conversation-Level Sequencing

Ogni conversazione ha una sequenza incrementale per i messaggi:

```rust
pub async fn post_message(
    db: &SqlitePool,
    conversation_id: Uuid,
    author_id: Uuid,
    content: String,
) -> Result<(Uuid, i64), AppError> {
    let msg_id = Uuid::new_v4();

    // Get next sequence number for this conversation
    let seq_num = increment_message_sequence(db, conversation_id).await?;

    let msg_id_str = msg_id.to_string();
    let conv_id_str = conversation_id.to_string();
    let author_id_str = author_id.to_string();
    let now = chrono::Utc::now().timestamp();

    query!(
        "INSERT INTO messages
         (id, conversation_id, author_id, content, created_at, sequence_num)
         VALUES (?, ?, ?, ?, ?, ?)",
        msg_id_str, conv_id_str, author_id_str, content, now, seq_num
    )
    .execute(db)
    .await?;

    Ok((msg_id, seq_num))
}

async fn increment_message_sequence(
    db: &SqlitePool,
    conversation_id: Uuid,
) -> Result<i64, AppError> {
    let conv_id_str = conversation_id.to_string();
    let now = chrono::Utc::now().timestamp();

    query!(
        "INSERT INTO message_sequences (conversation_id, current_sequence, last_updated)
         VALUES (?, 1, ?)
         ON CONFLICT(conversation_id) DO UPDATE SET
             current_sequence = current_sequence + 1,
             last_updated = ?
         RETURNING current_sequence",
        conv_id_str, now, now
    )
    .fetch_one(db)
    .await
    .map(|r| r.current_sequence)
    .map_err(Into::into)
}
```

#### Gap Detection e Recovery

```rust
// Client sends Ping with last known sequence
ClientMessage::Ping { user_sequence: 42 }

// Server compares with current sequence
let current_seq = get_user_sequence(db, user_id).await?; // Returns 150

if user_sequence < current_seq {
    // Gap detected
    ServerMessage::PongReceived {
        server_sequence: current_seq,
        gaps_detected: true,
    }
} else {
    ServerMessage::PongReceived {
        server_sequence: current_seq,
        gaps_detected: false,
    }
}

// Client requests missing events
ClientMessage::RequestUserResume {
    from_sequence: 42,
    limit: 100,
}

// Server returns events
let events = query_as!(
    UserEvent,
    "SELECT * FROM user_events
     WHERE user_id = ? AND sequence_num > ?
     ORDER BY sequence_num ASC
     LIMIT ?",
    user_id_str, from_sequence, limit
)
.fetch_all(db)
.await?;

ServerMessage::UserEventsResume { events }
```

#### Cleanup Task

Per evitare che la tabella `user_events` cresca all'infinito:

```rust
pub async fn spawn_cleanup_task(db: SqlitePool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(86400)); // 24 hours

        loop {
            interval.tick().await;

            let retention_days = 30;
            let cutoff = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::days(retention_days))
                .unwrap()
                .timestamp();

            // Delete old delivered events
            let result = query!(
                "DELETE FROM user_events
                 WHERE delivered = 1 AND created_at < ?",
                cutoff
            )
            .execute(&db)
            .await;

            match result {
                Ok(r) => tracing::info!("Cleaned up {} old user events", r.rows_affected()),
                Err(e) => tracing::error!("Failed to cleanup user events: {}", e),
            }
        }
    });
}
```

---

## Architettura WebSocket Dettagliata

Il sistema WebSocket di Ruggine Chat implementa un'architettura **Actor Model** per gestire connessioni real-time, broadcast multi-channel e sincronizzazione affidabile dei messaggi.

### Stack Tecnologico

**Core Technologies:**
- **Rust** 1.70+
- **Tokio** - Async runtime
- **Axum** - Web framework con WebSocket support
- **SQLx** - Async database driver (SQLite)
- **serde_json** - JSON serialization

**Key Libraries:**
- `futures` - Stream/Sink abstractions
- `tokio::sync` - Channels (mpsc, watch, broadcast)
- `uuid` - Unique identifiers
- `chrono` - Timestamp management

### Architettura High-Level

```
┌────────────────────────────────────────────────┐
│              Client (Browser/Desktop)          │
│              WebSocket Connection              │
└───────────────────┬────────────────────────────┘
                    │
                    ▼
┌────────────────────────────────────────────────┐
│           Axum WebSocket Handler               │
│         ws_handler() → ws_entrypoint()         │
└───────────────────┬────────────────────────────┘
                    │
                    ▼
┌────────────────────────────────────────────────┐
│          ConnectionActor (Actor Model)         │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐       │
│  │  Writer  │ │  Reader  │ │ Receiver │       │
│  │  Task    │ │  Task    │ │  Task    │       │
│  └──────────┘ └──────────┘ └──────────┘       │
└───────────────────┬────────────────────────────┘
                    │
          ┌─────────┴─────────┐
          │                   │
          ▼                   ▼
┌──────────────────┐  ┌──────────────────┐
│ Broadcast System │  │    Database      │
│ - Conversations  │  │    - SQLite      │
│ - Notifications  │  │    - Messages    │
│                  │  │    - Events      │
└──────────────────┘  └──────────────────┘
```

### Connection Actor Pattern

Il `ConnectionActor` è il cuore del sistema WebSocket. Implementa l'**Actor Model Pattern** spawning 3 task concorrenti che comunicano via channels.

#### Architettura a 3 Task

```
┌──────────────────────────────────────────────────┐
│          ConnectionActor::start()                │
│                                                  │
│  let (ws_tx, ws_rx) = socket.split();           │
│  let (stop_tx, stop_rx) = watch::channel();     │
│  let (out_tx, out_rx) = mpsc::channel(1024);    │
│                                                  │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐│
│  │   Writer   │  │   Reader   │  │  Receiver  ││
│  │            │  │            │  │            ││
│  │ ws_tx (own)│  │  ws_rx     │  │ Broadcast  ││
│  │ out_rx     │  │  out_tx    │  │ Merge      ││
│  │ Heartbeat  │  │  Handlers  │  │ out_tx     ││
│  │ Stop watch │  │  Stop watch│  │ Stop watch ││
│  └────────────┘  └────────────┘  └────────────┘│
│         │              │               │        │
│         └──────────────┼───────────────┘        │
│                        │                        │
│                   select! {}                    │
│            (first to complete triggers)         │
└──────────────────────────────────────────────────┘
```

#### Writer Task

**Responsabilità:**
1. Unico proprietario di `ws_tx` (WebSocket sender)
2. Heartbeat periodico con jitter (anti-thundering herd)
3. Timeout management su send
4. Error tracking (consecutive failures)
5. Graceful shutdown

**Implementazione chiave:**

```rust
let mut writer: JoinHandle<()> = tokio::spawn(async move {
    // Heartbeat con jitter (30s + 0-5s random)
    let heartbeat_base = Duration::from_secs(30);
    let jitter = Duration::from_millis(fastrand::u64(0..5000));
    let mut heartbeat_interval = interval(heartbeat_base + jitter);

    let mut consecutive_failures = 0u32;
    const MAX_CONSECUTIVE_FAILURES: u32 = 3;

    loop {
        select! {
            // Shutdown signal
            _ = stop_rx.changed() => {
                let close_result = timeout(
                    Duration::from_secs(5),
                    ws_tx.send(Message::Close(None))
                ).await;
                break;
            }

            // Messaggio da inviare con timeout
            maybe_msg = out_rx.recv() => {
                let send_result = timeout(
                    Duration::from_secs(10),
                    ws_tx.send(msg)
                ).await;

                match send_result {
                    Ok(Ok(_)) => consecutive_failures = 0,
                    _ => {
                        consecutive_failures += 1;
                        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                            break;
                        }
                    }
                }
            }

            // Heartbeat periodico
            _ = heartbeat_interval.tick() => {
                // Invia server_heartbeat
            }
        }
    }
});
```

**Timeout Strategy:**
- `send()`: 10 secondi
- `close()`: 5 secondi
- `heartbeat`: 5 secondi

#### Reader Task

**Responsabilità:**
1. Leggere messaggi WebSocket da client
2. Parsing JSON
3. Routing a handler appropriati
4. Gestione ping/pong

**Flusso:**
```
WebSocket Message → JSON Parse → Router → Handler
                                         ↓
                                   handle_chat_message
                                   handle_create_conversation
                                   handle_mark_read
                                   handle_delete_message
                                   handle_invite_user
                                   etc.
```

#### Receiver Task

**Responsabilità:**
1. Subscribe a broadcast channels (conversazioni + utente)
2. Merge eventi da N channels
3. Forward a Writer via `out_tx`

**StreamManager** gestisce N broadcast receivers dinamicamente:
- Aggiunge stream per nuove conversazioni
- Rimuove stream per conversazioni eliminate
- Merge di eventi da tutti i canali

### Sistema Broadcast Multi-Channel

Il sistema di broadcast gestisce la distribuzione real-time dei messaggi attraverso due tipologie di channel:

#### 1. Conversation Channels

**Scopo**: Distribuire messaggi real-time a tutti i partecipanti online di una conversazione.

```rust
// Creazione lazy del canale
pub async fn get_or_create_broadcast_tx(
    &self,
    conversation_id: Uuid,
) -> broadcast::Sender<Value> {
    // Fast path: read lock
    {
        let channels = self.broadcast_channels.read().await;
        if let Some(tx) = channels.get(&conversation_id) {
            return tx.clone();
        }
    }

    // Slow path: write lock
    let mut channels = self.broadcast_channels.write().await;
    channels.entry(conversation_id)
        .or_insert_with(|| {
            let (tx, _rx) = broadcast::channel(1024);
            tx
        })
        .clone()
}
```

**Broadcasting:**
```rust
match tx.send(payload) {
    Ok(receiver_count) => {
        info!("Delivered to {} receivers", receiver_count);
    }
    Err(_) => {
        // Non è errore! Il messaggio è già in user_events
        warn!("No active receivers (will deliver via user_events)");
    }
}
```

#### 2. User Notification Channels

**Scopo**: Inviare eventi personali a uno specifico utente:
- Conferme invio messaggi (`message_confirmation`)
- Notifiche nuove conversazioni (`new_conversation`)
- Eventi di modifica conversazioni

**Auto-Subscription** all'avvio della connessione:
```rust
// CRITICO: Auto-subscribe al proprio canale
let _user_tx = state
    .get_or_create_user_notification_channel(user_id)
    .await;
```

#### Doppio Delivery Garantito

Il sistema garantisce la consegna attraverso un **triple path per il sender** e **dual path per gli altri**:

```
Message Sent by Alice
     │
     ├────────────────────────────┬─────────────────────────┐
     │                            │                         │
     ▼                            ▼                         ▼
Message Confirmation      Broadcast Channel          User Events (DB)
(SOLO Alice)              (TUTTI online)             (TUTTI, sempre)
     │                            │                         │
     │ Via user channel           │ Via conversation ch.    │ INSERT INTO user_events
     ▼                            ▼                         ▼
Alice: "✓ Saved"          All: Real-time delivery    Recovery at reconnect
```

| Partecipante | Path 1: Confirmation | Path 2: Broadcast | Path 3: User Events |
|-------------|---------------------|-------------------|---------------------|
| **Alice (sender)** | ✅ Fast feedback<br>"Message saved" | ✅ Real-time delivery | ✅ Recovery backup |
| **Bob (online)** | ❌ | ✅ Real-time delivery | ✅ Recovery backup |
| **Charlie (offline)** | ❌ | ❌ | ✅ Primary delivery |

**Garanzia**: Anche se il broadcast fallisce (0 receivers) o il sender si disconnette, il messaggio è persistito in DB e `user_events` garantisce la delivery al reconnect.

### Graceful Shutdown

**Coordinamento tramite watch::channel:**
```rust
let (stop_tx, stop_rx) = watch::channel(false);

// Trigger shutdown
let _ = stop_tx.send(true);

// Task ricevono segnale
_ = stop_rx.changed() => {
    info!("Received stop signal");
    // Cleanup e exit
}
```

**Orchestrazione con select!:**
```rust
let result = select! {
    r = &mut reader_task => {
        stop_tx.send(true);
        recv_task.abort();
        writer.abort();
        r
    }
    // Altri branch...
};
```

**Cleanup con Grace Period (5 minuti):**
```rust
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(300)).await;
    cleanup_empty_channels(&state, user_id).await;
});
```

Questo permette reconnect rapide riutilizzando risorse esistenti.

---

## Sistema Dual-Sequence

Il sistema di sequenze duali garantisce **ordering** e **recovery** attraverso due livelli indipendenti di numerazione.

### Architettura Doppia Sequenza

```
┌─────────────────────────────────────────────────┐
│            USER SEQUENCE (Global)                │
│  Per ogni utente, incrementale su TUTTI eventi   │
│                                                  │
│  Events:                                         │
│    - new_message (in any conversation)           │
│    - conversation_confirmation                   │
│    - new_conversation                            │
│    - conversation_deleted                        │
│    - member_added                                │
│    - member_removed                              │
│                                                  │
│  DB: user_events table                           │
│    ├─ id (autoincrement)                         │
│    ├─ user_id                                    │
│    ├─ sequence_num ← INCREMENTALE GLOBALE        │
│    ├─ event_type                                 │
│    ├─ event_data (JSON)                          │
│    ├─ conversation_id (nullable)                 │
│    └─ created_at                                 │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│        CONVERSATION SEQUENCE (Per-Conv)          │
│  Per conversazione, incrementale sui messaggi    │
│                                                  │
│  DB: messages table                              │
│    ├─ id                                         │
│    ├─ conversation_id                            │
│    ├─ sequence_num ← INCREMENTALE PER CONV       │
│    ├─ author_id                                  │
│    ├─ content                                    │
│    └─ created_at                                 │
└─────────────────────────────────────────────────┘
```

### User Sequence

**Scopo**: Garantire che ogni utente riceva TUTTI gli eventi che lo riguardano in ordine garantito.

**Generazione atomica:**
```rust
async fn get_next_user_sequence(&self, user_id: Uuid) -> Result<u64> {
    let counters = self.user_sequence_counters.read().await;
    let counter = counters.entry(user_id)
        .or_insert_with(|| AtomicU64::new(0));

    Ok(counter.fetch_add(1, Ordering::SeqCst))
}
```

**Atomicità**: `AtomicU64::fetch_add()` garantisce incremento thread-safe senza lock.

**Eventi tracciati:**
1. `new_message` - Nuovo messaggio in qualsiasi conversazione
2. `conversation_confirmation` - Conferma creazione conversazione (per creatore)
3. `new_conversation` - Notifica nuova conversazione (per altri partecipanti)
4. `conversation_deleted` - Conversazione eliminata
5. `member_added` / `member_removed` - Modifiche membri gruppo

**Esempio evento:**
```json
{
  "sequence": 150,
  "event_type": "new_message",
  "event_data": {
    "type": "new_message",
    "conversation_id": "conv-uuid",
    "conversation_sequence": 42,
    "message": {
      "id": "msg-uuid",
      "author_username": "alice",
      "content": "Hello",
      "created_at": 1234567890
    }
  },
  "conversation_id": "conv-uuid",
  "created_at": 1234567890
}
```

### Conversation Sequence

**Scopo**: Ordinamento garantito dei messaggi all'interno di una conversazione.

**Generazione:**
```rust
async fn get_next_message_sequence(
    &self,
    conversation_id: Uuid,
) -> Result<u64> {
    let counters = self.conversation_sequence_counters.read().await;
    let counter = counters.entry(conversation_id)
        .or_insert_with(|| AtomicU64::new(0));

    Ok(counter.fetch_add(1, Ordering::SeqCst))
}

// Salvataggio in DB
sqlx::query(
    "INSERT INTO messages
     (id, conversation_id, sequence_num, author_id, content, created_at)
     VALUES (?, ?, ?, ?, ?, ?)"
)
.bind(msg_id.to_string())
.bind(conversation_id.to_string())
.bind(sequence as i64)  // ← Sequence
.bind(user_id.to_string())
.bind(content)
.bind(Utc::now().timestamp())
.execute(&state.pool)
.await?;
```

### last_read_sequence

Ogni partecipante traccia fino a dove ha letto:

```sql
CREATE TABLE participants (
    conversation_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    last_read_sequence INTEGER NOT NULL DEFAULT 0,
    ...
);
```

**Update on Mark Read:**
```rust
sqlx::query(
    "UPDATE participants
     SET last_read_sequence = ?
     WHERE conversation_id = ? AND user_id = ?"
)
.bind(sequence_num)
.bind(conversation_id.to_string())
.bind(user_id.to_string())
.execute(pool)
.await?;
```

**Calcolo Unread:**
```typescript
function getUnreadCount(conv: Conversation): number {
  return conv.last_msg_seq - conv.last_read_sequence;
}
```

### Recovery dopo Disconnessione

**Client Request:**
```json
{
  "type": "user_events_resume",
  "from_sequence": 150,
  "limit": 100
}
```

**Server Query:**
```rust
let rows = sqlx::query(
    "SELECT sequence_num, event_type, event_data, conversation_id, created_at
     FROM user_events
     WHERE user_id = ? AND sequence_num >= ?
     ORDER BY sequence_num ASC
     LIMIT ?"
)
.bind(user_id.to_string())
.bind(from_sequence as i64)
.bind(limit)
.fetch_all(&state.pool)
.await?;
```

**Server Response:**
```json
{
  "type": "user_resume_batch",
  "events": [
    {"sequence": 150, "event_type": "new_message", ...},
    {"sequence": 151, "event_type": "conversation_confirmation", ...},
    {"sequence": 152, "event_type": "new_conversation", ...}
  ],
  "from_sequence": 150,
  "to_sequence": 152
}
```

### Gap Detection (Client-Side)

```typescript
class UserSequenceTracker {
  private expectedSequence: number = 0;

  onEvent(event: UserEvent) {
    if (event.sequence !== this.expectedSequence) {
      console.warn(
        `Gap detected: expected ${this.expectedSequence}, got ${event.sequence}`
      );

      // Request missing events
      this.requestResume(this.expectedSequence);
    }

    this.expectedSequence = event.sequence + 1;
    this.processEvent(event);
  }
}
```

### Initial State Loading

All'avvio della connessione, il server invia lo stato completo:

```json
{
  "type": "initial_state",
  "user_sequence": 150,
  "conversations": [
    {
      "id": "conv-uuid",
      "kind": "dm",
      "title": "bob",
      "message_count": 42,
      "last_read_sequence": 40,
      "last_message": {
        "id": "msg-uuid",
        "sequence_num": 42,
        "content": "Last message",
        "author_username": "bob"
      }
    }
  ],
  "members_by_conversation": {...}
}
```

### Consistency Guarantees

**Atomicity:**
- User sequence: Atomic via `AtomicU64`
- DB insert: Atomic via SQLite transaction

**Ordering:**
- User events: `ORDER BY sequence_num ASC` garantito da DB
- Messages: `ORDER BY sequence_num DESC` per storico

**Durability:**
- Scritti in SQLite con WAL mode
- Fsync su commit transazione
- Recovery automatico su crash

### Performance

**Memory Usage (in-memory counters):**
```
Users: 1000 × (16 bytes UUID + 8 bytes AtomicU64) = 24 KB
Convs:  500 × (16 bytes UUID + 8 bytes AtomicU64) = 12 KB
Total:                                              36 KB
```

**Query Performance:**
```sql
-- Index per user_events
CREATE INDEX idx_user_events_user_seq
ON user_events(user_id, sequence_num);

-- Index per messages
CREATE INDEX idx_messages_conv_seq
ON messages(conversation_id, sequence_num);
```

Query time: O(log N) con indici B-tree.

---

## Modello di Concorrenza

Il sistema è costruito su **Tokio async runtime** con pattern di concorrenza basati su **channels** per comunicazione tra task.

### Tokio Async Runtime

**Task Spawning:**
```rust
// Spawning un task indipendente
let handle = tokio::spawn(async move {
    // Task logic
});

// Await del risultato
let result = handle.await?;

// Abort del task
handle.abort();
```

**Caratteristiche:**
- Ogni task è M:N green thread
- Scheduling cooperativo (yield points su `.await`)
- Work-stealing scheduler

### select! Macro

```rust
use tokio::select;

loop {
    select! {
        // Branch 1
        _ = stop_rx.changed() => {
            info!("Shutdown signal");
            break;
        }

        // Branch 2
        maybe_msg = out_rx.recv() => {
            if let Some(msg) = maybe_msg {
                ws_tx.send(msg).await?;
            }
        }

        // Branch 3
        _ = heartbeat_interval.tick() => {
            send_heartbeat().await?;
        }
    }
}
```

**Semantica:**
- Valuta tutti i branch concorrentemente
- Primo branch pronto viene eseguito
- Altri branch vengono cancellati (`.await` interrotto)

### Channel Types

#### 1. mpsc::channel (Multi-Producer, Single-Consumer)

```rust
let (tx, mut rx) = mpsc::channel::<T>(buffer_size);

// Producer (può essere clonato)
let tx1 = tx.clone();
tokio::spawn(async move {
    tx1.send(value).await?;
});

// Consumer (singolo)
while let Some(value) = rx.recv().await {
    process(value);
}
```

**Uso**: `out_tx` / `out_rx` per messaggi outbound.

**Backpressure**: Se buffer pieno, `send()` attende.

#### 2. watch::channel (Single-Producer, Multi-Consumer)

```rust
let (tx, rx) = watch::channel(initial_value);

// Producer (singolo)
tx.send(new_value)?;

// Consumers (multipli, clonabili)
let mut rx1 = rx.clone();
tokio::spawn(async move {
    while rx1.changed().await.is_ok() {
        let value = *rx1.borrow();
        println!("rx1: {}", value);
    }
});
```

**Uso**: `stop_tx` / `stop_rx` per shutdown coordination.

**Comportamento:**
- Mantiene sempre ultimo valore
- `changed()` attende finché valore cambia
- Se producer invia più volte rapidamente, consumer vede solo ultimo

#### 3. broadcast::channel (Multi-Producer, Multi-Consumer)

```rust
let (tx, _rx) = broadcast::channel::<T>(capacity);

// Producers (clonabile)
let tx1 = tx.clone();
tx1.send(value)?;

// Consumers (via subscribe)
let mut rx1 = tx.subscribe();
tokio::spawn(async move {
    while let Ok(msg) = rx1.recv().await {
        println!("{:?}", msg);
    }
});
```

**Uso**: Conversation & User notification channels.

**Lagging:**
```rust
match rx.recv().await {
    Ok(msg) => process(msg),
    Err(RecvError::Lagged(n)) => {
        warn!("Missed {} messages", n);
    }
    Err(RecvError::Closed) => break,
}
```

### Lock Types

#### RwLock (Read-Write Lock)

```rust
use tokio::sync::RwLock;

// Multiple readers (concurrent)
async fn read_data(state: &State, key: &str) -> Option<Value> {
    let data = state.data.read().await;
    data.get(key).cloned()
}

// Single writer (exclusive)
async fn write_data(state: &State, key: String, value: Value) {
    let mut data = state.data.write().await;
    data.insert(key, value);
}
```

**Uso**: `broadcast_channels`, `user_notification_channels`.

**Pattern: Double-Checked Locking:**
```rust
pub async fn get_or_create_tx(&self, id: Uuid) -> Sender {
    // Fast path: read lock (shared)
    {
        let channels = self.channels.read().await;
        if let Some(tx) = channels.get(&id) {
            return tx.clone();
        }
    }

    // Slow path: write lock (exclusive)
    let mut channels = self.channels.write().await;
    channels.entry(id)
        .or_insert_with(|| create_channel())
        .clone()
}
```

**Deadlock Avoidance:**
```rust
// ✅ GOOD: Scope limitato
{
    let data = state.data.read().await;
    let value = data.get(key);
} // Lock rilasciato qui

// ❌ BAD: Lock trattenuto attraverso await
let data = state.data.read().await;
some_async_operation().await; // Deadlock risk!
```

### Atomic Types

#### AtomicU64

```rust
use std::sync::atomic::{AtomicU64, Ordering};

let counter = AtomicU64::new(0);

// Atomic increment
let sequence = counter.fetch_add(1, Ordering::SeqCst);
```

**Ordering Semantics:**
- `Relaxed`: No ordering guarantees (fastest)
- `SeqCst`: Strongest (serialization point) - usato per sequenze

**Uso**: Sequence number generation (lock-free).

#### DashMap (Concurrent HashMap)

```rust
use dashmap::DashMap;

let map: DashMap<String, u32> = DashMap::new();

// Insert (lock-free)
map.insert("key".to_string(), 42);

// Get (lock-free read)
if let Some(value) = map.get("key") {
    println!("Value: {}", *value);
}

// Entry API (lock per entry)
map.entry("key".to_string())
    .and_modify(|v| *v += 1)
    .or_insert(1);
```

**Vantaggio**: Più veloce di `Arc<RwLock<HashMap>>` per accessi concorrenti.

### Graceful Shutdown Pattern

```rust
struct Application {
    stop_tx: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl Application {
    pub async fn shutdown(self) {
        // 1. Signal all tasks
        let _ = self.stop_tx.send(true);

        // 2. Wait for graceful completion (with timeout)
        let shutdown = async {
            for task in self.tasks {
                let _ = task.await;
            }
        };

        if timeout(Duration::from_secs(30), shutdown).await.is_err() {
            warn!("Graceful shutdown timeout, forcing exit");
        }
    }
}
```

### Performance Considerations

**Task Spawn Cost:**
```
tokio::spawn(): ~2 KB stack + overhead
Typical: ~3-5 KB per task
```

**Channel Performance (tipico):**
- mpsc: ~50 ns per send/recv (uncontended), ~20M msg/s
- broadcast: ~100 ns per send, ~10M msg/s
- watch: ~30 ns per update (fastest)

**Lock Contention:**
```rust
// ❌ HIGH CONTENTION
let mut data = state.data.lock().await;
for item in large_list {
    data.process(item); // Lock trattenuto a lungo
}

// ✅ LOW CONTENTION
for item in large_list {
    let mut data = state.data.lock().await;
    data.process(item);
} // Lock rilasciato ogni iterazione
```

---

## Gestione Errori e Recovery

Il sistema implementa **error handling multi-livello** con timeout, retry logic e graceful degradation per garantire resilienza.

### Strategia Multi-Livello

```
Level 1: Connection-Level Errors
    ├─ WebSocket timeout
    ├─ Consecutive failures tracking
    └─ Graceful shutdown

Level 2: Broadcast Errors
    ├─ No active receivers (OK)
    ├─ Lagged receivers
    └─ Channel closed

Level 3: Database Errors
    ├─ Transaction rollback
    ├─ Retry with backoff
    └─ Query timeout

Level 4: Handler Errors
    ├─ Validation errors
    ├─ Authorization errors
    └─ Business logic errors
```

### Connection-Level Errors

#### Timeout Management

```rust
// Send con timeout (10s)
let send_result = timeout(
    Duration::from_secs(10),
    ws_tx.send(message)
).await;

match send_result {
    Ok(Ok(_)) => {
        consecutive_failures = 0; // Reset on success
    }
    Ok(Err(e)) => {
        consecutive_failures += 1;
        if consecutive_failures >= 3 {
            error!("Too many failures, closing connection");
            break;
        }
    }
    Err(_) => {
        consecutive_failures += 1;
        if consecutive_failures >= 3 {
            error!("Too many timeouts, closing connection");
            break;
        }
    }
}
```

**Timeouts applicati:**
- `ws_tx.send()`: 10 secondi
- `ws_tx.close()`: 5 secondi
- Heartbeat send: 5 secondi

#### Consecutive Failures Tracking

```rust
const MAX_CONSECUTIVE_FAILURES: u32 = 3;
let mut consecutive_failures = 0u32;

// Su ogni errore
consecutive_failures += 1;
if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
    break; // Chiudi connessione
}

// Su successo
consecutive_failures = 0; // Reset
```

**Filosofia**: Tollera errori temporanei, ma chiude se persistenti.

### Broadcast Errors

#### No Active Receivers

```rust
match tx.send(payload) {
    Ok(n) => {
        info!("Delivered to {} receivers", n);
    }
    Err(_) => {
        // NON è errore critico!
        warn!("No active receivers, message in user_events");
        // Messaggio già salvato in DB
    }
}
```

**Comportamento**: Channel dropped è normale se tutti disconnessi.

**Recovery**: Messaggi salvati in `user_events` → delivery garantita.

#### Lagged Receivers

```rust
match rx.recv().await {
    Ok(msg) => {
        // Forward normally
    }
    Err(RecvError::Lagged(n)) => {
        warn!("Receiver lagged by {} messages", n);

        // Client deve fare recovery
        send_recovery_request_to_client(n);
    }
    Err(RecvError::Closed) => {
        remove_from_stream_manager();
    }
}
```

**Causa**: Receiver troppo lento, buffer broadcast pieno (1024 slot).

**Recovery Client-Side:**
```typescript
onLaggedEvent(lagCount: number) {
  fetch(`/api/conversations/${convId}/messages?limit=${lagCount}`)
    .then(msgs => msgs.forEach(msg => this.processMessage(msg)));
}
```

### Database Errors

#### Transaction Rollback

```rust
async fn create_conversation_atomic(
    pool: &SqlitePool,
    conversation_id: Uuid,
    participants: Vec<Uuid>,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Step 1: Insert conversation
    sqlx::query("INSERT INTO conversations ...")
        .execute(&mut *tx)
        .await?;

    // Step 2: Insert participants
    for participant_id in participants {
        sqlx::query("INSERT INTO participants ...")
            .execute(&mut *tx)
            .await?;
    }

    // Commit or rollback atomically
    tx.commit().await?;

    Ok(())
}
```

**Su errore**: Rollback automatico, stato DB consistente.

#### Retry with Backoff

```rust
async fn with_retry<F, T>(
    operation: F,
    max_retries: u32,
) -> Result<T>
where
    F: Fn() -> BoxFuture<'static, Result<T>>,
{
    let mut retries = 0;
    let mut delay = Duration::from_millis(100);

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if retries < max_retries => {
                retries += 1;
                warn!("Retry #{}/{}", retries, max_retries);

                tokio::time::sleep(delay).await;
                delay *= 2; // Exponential backoff
            }
            Err(e) => return Err(e),
        }
    }
}
```

### Handler Errors

#### Validation Errors

```rust
let content = value
    .get("content")
    .and_then(|v| v.as_str())
    .filter(|s| !s.trim().is_empty())
    .ok_or_else(|| {
        AppError::BadRequest("Message content cannot be empty".into())
    })?;
```

#### Authorization Errors

```rust
let is_participant = ConversationRepo::is_participant(
    &state.pool,
    conversation_id,
    user_id
).await?;

if !is_participant {
    return Err(AppError::Forbidden);
}
```

**Error Types:**
- `BadRequest`: Input invalido (400)
- `Unauthorized`: Autenticazione richiesta (401)
- `Forbidden`: Non autorizzato (403)
- `NotFound`: Risorsa non esistente (404)
- `Internal`: Errore server (500)

### Error Response Format

```rust
let error_response = json!({
    "type": "error",
    "message": error.user_message(),
    "error_code": error.error_code(),
    "details": error.details(),
});
```

**Esempio:**
```json
{
  "type": "error",
  "message": "Message content cannot be empty",
  "error_code": "BAD_REQUEST",
  "details": null
}
```

### Client Disconnection Handling

**Detection Methods:**
1. WebSocket errors
2. Close frame ricevuto
3. Stream ended
4. Heartbeat mancanti (3 consecutivi)

**Cleanup Actions:**
```rust
// Immediato
1. Abort task (reader, receiver, writer)
2. Drop WebSocket handles
3. Log statistiche connessione

// Dopo 5 minuti (grace period)
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(300)).await;
    cleanup_empty_channels(&state, user_id).await;
});
```

**Perché grace period?**
- Reconnect rapide (< 5 min) riutilizzano risorse
- Evita churn di allocazioni
- Permette connection pooling

### Logging Strategy

```rust
// ERROR: Condizioni critiche
error!("Too many consecutive failures for user {}", user_id);

// WARN: Condizioni anomale ma gestibili
warn!("Broadcast failed: no active receivers");

// INFO: Eventi importanti (lifecycle)
info!("Connection closed for user {}", user_id);

// DEBUG: Dettagli per troubleshooting
debug!("Sent message confirmation");
```

### Recovery Scenarios

**Scenario 1: Database Timeout**
```
Problem: DB query timeout
Solution:
  1. Retry con backoff (3 tentativi)
  2. Se fallisce → Invia stato parziale
  3. Client richiede recovery via user_events_resume
```

**Scenario 2: Broadcast Channel Lagged**
```
Problem: Receiver troppo lento
Solution:
  1. Receiver riceve RecvError::Lagged(n)
  2. Log warning
  3. Client richiede messaggi via HTTP API
  4. Sistema continua normale operazione
```

**Scenario 3: Network Partition**
```
Problem: Client disconnesso, poi riconnette
Solution:
  1. Client rileva gap nelle sequenze
  2. Richiede user_events_resume
  3. Server invia tutti eventi mancanti
  4. Client reintegra stato locale
```

---

## Frontend (Client)

### Architettura MVVM

Il client segue il pattern **Model-View-ViewModel** (MVVM):

```
┌────────────────────────────────────────────────────────┐
│                        VIEW                            │
│  ┌──────────────────────────────────────────────────┐  │
│  │          UI Components (egui)                    │  │
│  │  - Pages (Auth, Chat)                            │  │
│  │  - Layout (Header, Sidebar)                      │  │
│  │  - Modals (Create DM, Create Group, etc.)       │  │
│  └─────────────────┬────────────────────────────────┘  │
└────────────────────┼───────────────────────────────────┘
                     │ read state, dispatch events
                     │
┌────────────────────▼───────────────────────────────────┐
│                   VIEW-MODEL                           │
│  ┌──────────────────────────────────────────────────┐  │
│  │            AppState + UIState                    │  │
│  │  - Current user (token, user_id, username)       │  │
│  │  - Conversations (HashMap)                       │  │
│  │  - Messages (HashMap<cid, Vec<MessageDto>>)      │  │
│  │  - UI state (current page, modals, toasts)       │  │
│  └─────────────────┬────────────────────────────────┘  │
└────────────────────┼───────────────────────────────────┘
                     │ update via events
                     │
┌────────────────────▼───────────────────────────────────┐
│                     MODEL                              │
│  ┌──────────────────────────────────────────────────┐  │
│  │         Event Dispatcher + Handlers              │  │
│  │  - AuthHandler                                   │  │
│  │  - MessageHandler                                │  │
│  │  - ConversationHandler                           │  │
│  │  - SequenceHandler                               │  │
│  │  - WebSocketHandler                              │  │
│  └─────────────────┬────────────────────────────────┘  │
│                    │                                   │
│  ┌─────────────────▼────────────────────────────────┐  │
│  │         API Layer + WebSocket Manager            │  │
│  │  - HTTP Client (reqwest)                         │  │
│  │  - WebSocket Client (tokio-tungstenite)          │  │
│  │  - Health Monitor, Rate Limiter                  │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────┘
```

### State Management

#### AppState (Core State)

```rust
// client/src/state/core.rs
#[derive(Default)]
pub struct AppState {
    // Authentication
    pub token: Option<String>,
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub user_sequence: i64,

    // Data
    pub conversations: HashMap<Uuid, ConversationDto>,
    pub messages: HashMap<Uuid, Vec<MessageDto>>,  // cid → messages
    pub users: HashMap<Uuid, UserDto>,             // user_id → user

    // WebSocket
    pub ws_connected: bool,
    pub ws_sender: Option<WsSender>,

    // Channels
    pub ui_event_sender: Sender<UiEvent>,
    pub ui_event_receiver: Receiver<UiEvent>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, rx) = channel::unbounded();
        Self {
            ui_event_sender: tx,
            ui_event_receiver: rx,
            ..Default::default()
        }
    }

    pub fn get_conversation(&self, cid: Uuid) -> Option<&ConversationDto> {
        self.conversations.get(&cid)
    }

    pub fn get_messages(&self, cid: Uuid) -> Vec<MessageDto> {
        self.messages.get(&cid).cloned().unwrap_or_default()
    }

    pub fn add_message(&mut self, cid: Uuid, msg: MessageDto) {
        self.messages.entry(cid).or_default().push(msg);
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some() && self.user_id.is_some()
    }
}
```

#### UIState

```rust
// client/src/state/ui.rs
#[derive(Default)]
pub struct UIState {
    pub current_page: Page,
    pub selected_conversation_id: Option<Uuid>,

    // Modals
    pub show_account_modal: bool,
    pub show_create_dm_modal: bool,
    pub show_create_group_modal: bool,
    pub show_invite_modal: bool,
    pub show_delete_account_modal: bool,

    // Input fields
    pub login_username: String,
    pub login_password: String,
    pub register_username: String,
    pub register_password: String,
    pub message_input: String,

    // Toasts (notifications)
    pub toasts: Vec<Toast>,

    // Loading states
    pub is_loading: bool,
    // Note: typing_users definito ma non utilizzato (feature non implementata)
}

#[derive(PartialEq)]
pub enum Page {
    Auth,
    Chat,
}

pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created_at: Instant,
}

pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}
```

### WebSocket Manager

Gestisce il lifecycle della connessione WebSocket.

```rust
// client/src/app/ws_manager/ws_manager.rs
pub struct WebSocketManager {
    state: Arc<RwLock<WsState>>,
    health_monitor: HealthMonitor,
    rate_limiter: RateLimiter,
    message_processor: MessageProcessor,
}

enum WsState {
    Disconnected,
    Connecting,
    Connected {
        sender: WsSender,
        receiver: WsReceiver,
    },
    Reconnecting {
        attempt: u32,
    },
}

impl WebSocketManager {
    pub async fn connect(
        &mut self,
        token: &str,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        *self.state.write().await = WsState::Connecting;

        let url = format!("ws://localhost:8080/ws?session_id={}", Uuid::new_v4());

        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;

        let (sender, receiver) = ws_stream.split();

        *self.state.write().await = WsState::Connected {
            sender: Arc::new(Mutex::new(sender)),
            receiver: Arc::new(Mutex::new(receiver)),
        };

        // Start health monitor
        self.health_monitor.start(self.state.clone()).await;

        Ok(())
    }

    pub async fn send(&self, message: ClientMessage) -> Result<(), AppError> {
        // Rate limiting
        self.rate_limiter.check().await?;

        let state = self.state.read().await;
        if let WsState::Connected { sender, .. } = &*state {
            let json = serde_json::to_string(&message)?;
            sender.lock().await.send(Message::Text(json)).await?;
        }

        Ok(())
    }

    pub async fn receive(&self) -> Option<ServerMessage> {
        let state = self.state.read().await;
        if let WsState::Connected { receiver, .. } = &*state {
            match receiver.lock().await.next().await {
                Some(Ok(Message::Text(text))) => {
                    serde_json::from_str(&text).ok()
                }
                _ => None,
            }
        } else {
            None
        }
    }
}
```

#### Health Monitor

```rust
// client/src/app/ws_manager/health_monitor.rs
pub struct HealthMonitor {
    last_ping: Arc<RwLock<Instant>>,
    ping_interval: Duration,
}

impl HealthMonitor {
    pub async fn start(&self, ws_state: Arc<RwLock<WsState>>) {
        let last_ping = self.last_ping.clone();
        let interval = self.ping_interval;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            loop {
                ticker.tick().await;

                // Check if last ping was too long ago
                let elapsed = last_ping.read().await.elapsed();
                if elapsed > Duration::from_secs(60) {
                    tracing::warn!("WebSocket connection unhealthy, reconnecting...");
                    // Trigger reconnection
                }

                // Send ping
                // (implementation depends on ws_state access)
            }
        });
    }
}
```

### Event Dispatching

Il sistema usa un **event bus** per gestire gli aggiornamenti dello stato.

```rust
// client/src/app/events/dispatcher.rs
pub enum UiEvent {
    // Auth events
    LoginSuccess { token: String, user_id: Uuid, username: String, last_sequence: i64 },
    LoginError { message: String },
    LogoutRequested,

    // Message events
    MessageReceived { conversation_id: Uuid, message: MessageDto },
    MessageSent { conversation_id: Uuid, client_msg_id: Uuid },
    MessageConfirmed { client_msg_id: Uuid, server_msg_id: Uuid, sequence: i64 },
    MessageDeleted { message_id: Uuid, conversation_id: Uuid },

    // Conversation events
    ConversationCreated { conversation: ConversationDto },
    ConversationDeleted { conversation_id: Uuid },
    UserJoined { conversation_id: Uuid, user_id: Uuid, username: String },
    UserLeft { conversation_id: Uuid, user_id: Uuid },

    // WebSocket events
    WebSocketConnected,
    WebSocketDisconnected,
    WebSocketError { message: String },

    // Sequence events
    GapDetected { server_sequence: i64 },
    UserEventsResumed { events: Vec<UserEvent> },
}

pub struct EventDispatcher {
    handlers: Vec<Box<dyn EventHandler>>,
}

#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: &UiEvent, state: &mut AppState, ui_state: &mut UIState);
}

impl EventDispatcher {
    pub fn new() -> Self {
        Self {
            handlers: vec![
                Box::new(AuthHandler),
                Box::new(MessageHandler),
                Box::new(ConversationHandler),
                Box::new(SequenceHandler),
                Box::new(WebSocketHandler),
            ],
        }
    }

    pub async fn dispatch(
        &self,
        event: UiEvent,
        state: &mut AppState,
        ui_state: &mut UIState,
    ) {
        for handler in &self.handlers {
            handler.handle(&event, state, ui_state).await;
        }
    }
}
```

**Example Handler:**

```rust
// client/src/app/events/message_handler.rs
pub struct MessageHandler;

#[async_trait]
impl EventHandler for MessageHandler {
    async fn handle(&self, event: &UiEvent, state: &mut AppState, ui_state: &mut UIState) {
        match event {
            UiEvent::MessageReceived { conversation_id, message } => {
                // Add to messages
                state.add_message(*conversation_id, message.clone());

                // Update last_activity in conversation
                if let Some(conv) = state.conversations.get_mut(conversation_id) {
                    conv.last_activity = Some(message.created_at);
                }

                // Show toast if not current conversation
                if ui_state.selected_conversation_id != Some(*conversation_id) {
                    ui_state.toasts.push(Toast {
                        message: format!("New message in {}", conversation_id),
                        level: ToastLevel::Info,
                        created_at: Instant::now(),
                    });
                }
            }

            UiEvent::MessageConfirmed { client_msg_id, server_msg_id, sequence } => {
                // Find and update optimistic message
                for msgs in state.messages.values_mut() {
                    if let Some(msg) = msgs.iter_mut().find(|m| m.id == *client_msg_id) {
                        msg.id = *server_msg_id;
                        msg.sequence_num = Some(*sequence);
                        msg.is_confirmed = true;
                    }
                }
            }

            UiEvent::MessageDeleted { message_id, conversation_id } => {
                // Remove from messages
                if let Some(msgs) = state.messages.get_mut(conversation_id) {
                    msgs.retain(|m| m.id != *message_id);
                }
            }

            _ => {}
        }
    }
}
```

---

## Flussi di Dati Principali

### Flusso di Invio Messaggio

```
┌─────────────────────────────────────────────────────────────┐
│  1. USER types message and presses Enter                    │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  2. UI creates optimistic message                           │
│     - client_msg_id = Uuid::new_v4()                        │
│     - is_confirmed = false                                  │
│     - Add to local state.messages                           │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  3. Send via WebSocket                                      │
│     ClientMessage::ChatMessage {                            │
│       cid, content, client_msg_id                           │
│     }                                                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      │ (WebSocket)
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  4. SERVER receives message                                 │
│     - Verify user is participant                            │
│     - Create Message record in DB                           │
│     - Assign sequence_num (increment_message_sequence)      │
│     - server_msg_id = Uuid::new_v4()                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
          ┌───────────┴───────────┐
          │                       │
┌─────────▼─────────┐   ┌─────────▼─────────┐
│  5a. Broadcast    │   │  5b. Send         │
│  to all           │   │  confirmation     │
│  participants     │   │  to sender        │
│                   │   │                   │
│  ChatMessage-     │   │  Message-         │
│  Received {       │   │  Confirmation {   │
│    id: server_id, │   │    client_msg_id, │
│    content,       │   │    server_msg_id, │
│    author_id,     │   │    sequence,      │
│    sequence_num   │   │    status         │
│  }                │   │  }                │
└─────────┬─────────┘   └─────────┬─────────┘
          │                       │
          │ (WebSocket)           │ (WebSocket)
          │                       │
┌─────────▼─────────┐   ┌─────────▼─────────┐
│  6a. OTHER        │   │  6b. SENDER       │
│  CLIENTS          │   │  CLIENT           │
│  receive new      │   │  receives         │
│  message          │   │  confirmation     │
│                   │   │                   │
│  - Add to         │   │  - Find temp msg  │
│    messages       │   │  - Replace ID     │
│  - Show in UI     │   │  - Set confirmed  │
└───────────────────┘   └───────────────────┘
```

### Flusso di Gap Detection

```
┌─────────────────────────────────────────────────────────────┐
│  1. CLIENT disconnected for 2 hours                         │
│     - Last known user_sequence = 42                         │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  2. CLIENT reconnects WebSocket                             │
│     - Send Ping { user_sequence: 42 }                       │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  3. SERVER receives Ping                                    │
│     - Get user's current_sequence from DB                   │
│     - current_sequence = 150                                │
│     - Gap detected: 42 < 150                                │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  4. SERVER sends PongReceived                               │
│     {                                                        │
│       server_sequence: 150,                                 │
│       gaps_detected: true                                   │
│     }                                                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  5. CLIENT receives Pong                                    │
│     - Dispatch UiEvent::GapDetected                         │
│     - SequenceHandler triggers recovery                     │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  6. CLIENT sends RequestUserResume                          │
│     {                                                        │
│       from_sequence: 42,                                    │
│       limit: 100                                            │
│     }                                                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  7. SERVER queries user_events                              │
│     SELECT * FROM user_events                               │
│     WHERE user_id = ? AND sequence_num > 42                 │
│     ORDER BY sequence_num ASC LIMIT 100                     │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  8. SERVER sends UserEventsResume                           │
│     {                                                        │
│       events: [                                             │
│         {seq:43, type:"message_received", data:{...}},      │
│         {seq:44, type:"user_joined", data:{...}},           │
│         ...                                                 │
│       ]                                                     │
│     }                                                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  9. CLIENT processes events                                 │
│     - For each event:                                       │
│       - Parse event_data                                    │
│       - Apply to state (add message, add conversation, etc.)│
│       - Update UI                                           │
│     - Update local user_sequence = 150                      │
└─────────────────────────────────────────────────────────────┘
```

### Flusso di Eliminazione Account

```
┌─────────────────────────────────────────────────────────────┐
│  1. USER clicks "Delete Account"                            │
│     - Opens confirmation modal                              │
│     - Enters password                                       │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  2. CLIENT sends DELETE /api/users/deleteMe                 │
│     - Header: Authorization: Bearer <token>                 │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  3. SERVER validates AuthUser                               │
│     - Extract user_id from JWT                              │
│     - Verify user exists                                    │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  4. UserService::notify_participants_of_deleted_user()      │
│     - Get all conversations for user                        │
│     - For each DM:                                          │
│       → Notify other participant (DM will be deleted)       │
│     - For each Group (as member):                           │
│       → Broadcast UserLeft event                            │
│     - For each Group (as owner):                            │
│       → Broadcast OwnerDeleted event                        │
│       → Group will be cascade deleted                       │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  5. UserService::delete_user()                              │
│     BEGIN TRANSACTION                                       │
│       DELETE FROM user_events WHERE user_id = ?             │
│       DELETE FROM user_sequences WHERE user_id = ?          │
│       DELETE FROM users WHERE id = ?                        │
│         → CASCADE DELETE:                                   │
│           - participants (user_id)                          │
│           - messages (author_id)                            │
│           - conversations (owner_id)                        │
│     COMMIT                                                  │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  6. SERVER returns 204 NO_CONTENT                           │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│  7. CLIENT receives response                                │
│     - Dispatch UiEvent::LogoutRequested                     │
│     - Clear token, user_id, username                        │
│     - Close WebSocket                                       │
│     - Navigate to Auth page                                 │
└─────────────────────────────────────────────────────────────┘
```

---

## API Reference

### Authentication Endpoints

#### POST /api/users/register

Registra un nuovo utente.

**Request:**
```json
{
  "username": "alice",
  "password": "password123"
}
```

**Response (200):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Errors:**
- 400: Invalid input (username empty, password < 4 chars)
- 409: Username already exists

---

#### POST /api/users/login

Effettua il login e ottiene un JWT token.

**Request:**
```json
{
  "username": "alice",
  "password": "password123"
}
```

**Response (200):**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "username": "alice",
  "last_sequence": 42
}
```

**Errors:**
- 401: Invalid username or password

---

#### POST /api/users/logout

Logout (stateless, client elimina token).

**Request:**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```

**Response (200):**
```json
{
  "message": "Logged out successfully"
}
```

---

#### DELETE /api/users/deleteMe

Elimina l'account corrente (richiede autenticazione).

**Headers:**
```
Authorization: Bearer <token>
```

**Response (204):**
No content

**Errors:**
- 401: Unauthorized

---

### Conversation Endpoints

#### GET /api/conversations

Ottiene tutte le conversazioni dell'utente autenticato.

**Headers:**
```
Authorization: Bearer <token>
```

**Response (200):**
```json
[
  {
    "id": "conv-uuid-1",
    "kind": "dm",
    "title": null,
    "owner_id": null,
    "created_at": 1704067350,
    "last_read_sequence": 42,
    "last_activity": 1704067400,
    "last_msg_seq": 50
  },
  {
    "id": "conv-uuid-2",
    "kind": "group",
    "title": "Team Alpha",
    "owner_id": "user-uuid",
    "created_at": 1704067350,
    "last_read_sequence": 10,
    "last_activity": 1704067500,
    "last_msg_seq": 15
  }
]
```

---

#### GET /api/conversations/:id/with-messages

Ottiene una conversazione con messaggi e membri.

**Headers:**
```
Authorization: Bearer <token>
```

**Response (200):**
```json
{
  "conversation": {
    "id": "conv-uuid",
    "kind": "group",
    "title": "Team Alpha",
    "owner_id": "user-uuid",
    "created_at": 1704067350
  },
  "messages": [
    {
      "id": "msg-uuid-1",
      "conversation_id": "conv-uuid",
      "author_id": "user-uuid",
      "author_username": "alice",
      "content": "Hello!",
      "created_at": 1704067400,
      "sequence_num": 1
    }
  ],
  "members": [
    {
      "user_id": "user-uuid-1",
      "username": "alice",
      "role": "owner"
    },
    {
      "user_id": "user-uuid-2",
      "username": "bob",
      "role": "member"
    }
  ]
}
```

---

#### POST /api/conversations/dm

Crea o recupera una DM con un utente.

**Headers:**
```
Authorization: Bearer <token>
```

**Request:**
```json
{
  "user_username": "bob"
}
```

**Response (200):**
```json
{
  "id": "conv-uuid"
}
```

**Errors:**
- 404: User not found

---

#### POST /api/conversations/groups

Crea un nuovo gruppo.

**Headers:**
```
Authorization: Bearer <token>
```

**Request:**
```json
{
  "name": "Team Alpha"
}
```

**Response (200):**
```json
{
  "id": "conv-uuid"
}
```

---

#### POST /api/conversations/:id/members

Aggiunge un membro al gruppo (solo owner).

**Headers:**
```
Authorization: Bearer <token>
```

**Request:**
```json
{
  "member_id": "user-uuid"
}
```

**Response (200):**
```json
{
  "message": "Member added"
}
```

**Errors:**
- 403: Forbidden (not owner)
- 400: User already a member

---

#### DELETE /api/conversations/:id/members/:user_id

Rimuove un membro dal gruppo (solo owner).

**Headers:**
```
Authorization: Bearer <token>
```

**Response (200):**
```json
{
  "message": "Member removed"
}
```

**Errors:**
- 403: Forbidden (not owner)

---

### Message Endpoints

#### GET /api/conversations/:cid/messages

Ottiene messaggi di una conversazione (con paginazione).

**Headers:**
```
Authorization: Bearer <token>
```

**Query Parameters:**
- `before_sequence` (optional): Ottiene messaggi prima di questa sequenza
- `limit` (optional, default=50): Numero di messaggi da recuperare

**Response (200):**
```json
[
  {
    "id": "msg-uuid-1",
    "conversation_id": "conv-uuid",
    "author_id": "user-uuid",
    "author_username": "alice",
    "content": "Hello!",
    "created_at": 1704067400,
    "sequence_num": 1
  },
  {
    "id": "msg-uuid-2",
    "conversation_id": "conv-uuid",
    "author_id": "user-uuid-2",
    "author_username": "bob",
    "content": "Hi!",
    "created_at": 1704067450,
    "sequence_num": 2
  }
]
```

---

#### DELETE /api/messages/:message_id

Elimina un messaggio (solo autore).

**Headers:**
```
Authorization: Bearer <token>
```

**Response (200):**
```json
{
  "message": "Message deleted"
}
```

**Errors:**
- 403: Forbidden (not author)
- 404: Message not found

---

### Aggiunta Membri ai Gruppi (WebSocket)

> **NOTA IMPORTANTE**: L'aggiunta di membri ai gruppi NON usa token-based invites via HTTP API.
> Il sistema funziona tramite **WebSocket** con il message type `InviteUser`.

#### WebSocket: InviteUser (solo owner)

Aggiunge uno o più membri al gruppo tramite WebSocket.

**Client invia:**
```json
{
  "type": "InviteUser",
  "cid": "conversation-uuid",
  "usernames": ["alice", "bob", "charlie"]
}
```

**Server elabora:**
1. Verifica che il mittente sia owner del gruppo
2. Per ogni username:
   - Cerca l'utente nel database
   - Se esiste e non è già membro, lo aggiunge come partecipante
   - Invia evento `new_conversation` al nuovo membro
   - Invia evento `member_added` agli altri membri del gruppo

**Server risponde (al nuovo membro):**
```json
{
  "type": "new_conversation",
  "conversation": {
    "id": "conv-uuid",
    "kind": "group",
    "title": "Team Alpha",
    "owner_id": "owner-uuid",
    "created_at": 1704067350,
    ...
  }
}
```

**Server broadcast (agli altri membri):**
```json
{
  "type": "member_added",
  "username": "alice",
  "user_id": "user-uuid",
  "added_by": "owner-username",
  "timestamp": 1704067350
}
```

**Errori:**
- Utente non è owner del gruppo
- Username non esiste
- Utente già membro del gruppo

---

### Sistema di Token-Based Invites (Opzionale - NON USATO per aggiunta membri)

> **NOTA**: Questo sistema esiste nel codice ma NON è utilizzato nell'interfaccia utente principale.
> È un'implementazione separata per inviti tramite link condivisibili.

#### POST /api/conversations/:id/invite

Genera un token di invito usa-e-getta (implementato ma non usato nell'UI).

**Headers:**
```
Authorization: Bearer <token>
```

**Response (200):**
```json
{
  "token": "ABC123XYZ456"
}
```

#### POST /api/conversations/join-by-token

Unisciti tramite token (implementato ma non usato nell'UI).

**Request:**
```json
{
  "token": "ABC123XYZ456"
}
```

**Response (200):**
```json
{
  "id": "conv-uuid"
}
```

---

## Deployment e Configurazione

### Variabili d'Ambiente (Server)

Creare un file `.env` in `server/`:

```env
# Database
DATABASE_URL=sqlite://ruggine.sqlite
# Oppure per in-memory (testing):
# DATABASE_URL=:memory:

# Server binding
BIND=127.0.0.1:8080
# Per esporre su rete locale:
# BIND=0.0.0.0:8080

# JWT Secret (MUST BE >= 32 chars in production)
JWT_SECRET=your-secret-key-minimum-32-characters-long-change-in-production

# Logging
RUST_LOG=info,tower_http=debug
# Per debug completo:
# RUST_LOG=debug,sqlx=trace
```

### Build Release

#### Server

```bash
cd server
cargo build --release
```

Il binario sarà in `target/release/server` (o `server.exe` su Windows).

#### Client

```bash
cd client
cargo build --release
```

Il binario sarà in `target/release/client` (o `client.exe` su Windows).

### Esecuzione

```bash
# Terminal 1: Server
cd server
./target/release/server

# Terminal 2: Client
cd client
./target/release/client
```

### Docker Deployment (Esempio)

**Dockerfile (Server):**

```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY server/Cargo.toml server/Cargo.lock ./
COPY server/src ./src
COPY server/migrations ./migrations

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libsqlite3-0 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/server /app/server
COPY --from=builder /app/migrations /app/migrations

ENV DATABASE_URL=sqlite://ruggine.sqlite
ENV BIND=0.0.0.0:8080
ENV JWT_SECRET=change-me-in-production-minimum-32-characters

EXPOSE 8080

CMD ["/app/server"]
```

**docker-compose.yml:**

```yaml
version: '3.8'

services:
  server:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=sqlite://ruggine.sqlite
      - BIND=0.0.0.0:8080
      - JWT_SECRET=${JWT_SECRET}
      - RUST_LOG=info
    volumes:
      - ./data:/app/data
    restart: unless-stopped
```

**Build e Run:**

```bash
docker-compose up -d
```

---

## Performance e Scalabilità

### Database Optimization

#### Indices

Gli indices sono fondamentali per performance:

```sql
-- Message queries (most frequent)
CREATE INDEX idx_msgs_conv_ts ON messages(conversation_id, created_at);
CREATE INDEX idx_msgs_conv_seq ON messages(conversation_id, sequence_num);

-- Unread tracking
CREATE INDEX idx_participants_last_read
  ON participants(user_id, conversation_id, last_read_sequence);

-- Event sourcing
CREATE INDEX idx_user_events_seq ON user_events(user_id, sequence_num);
CREATE INDEX idx_user_events_undelivered
  ON user_events(user_id, delivered) WHERE delivered = 0;
```

#### Query Optimization

- **Pagination**: Usa `LIMIT` e `OFFSET` (o `before_sequence`)
- **Connection Pooling**: SQLx pool con `max_connections=5` (default)
- **WAL Mode**: Write-Ahead Logging per concurrency

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -64000;  -- 64MB cache
```

### WebSocket Scalability

#### Limiti Attuali

- **In-Memory Connections**: `HashMap<Uuid, Sender>` in AppState
- **Single Server**: Non supporta horizontal scaling

#### Soluzione per Scale-Out

Per supportare multiple istanze server:

1. **Redis PubSub** per broadcasting:
   ```rust
   // Invece di broadcast_to_conversation() diretto
   // Pubblica su Redis channel
   redis.publish(
       format!("conversation:{}", cid),
       serde_json::to_string(&message)?
   ).await?;

   // Ogni server ascolta i channel
   // E invia ai propri clients connessi
   ```

2. **Shared State** per active connections:
   ```rust
   // Invece di in-memory HashMap
   // Usa Redis Set per tracciare user_id → server_instance
   redis.sadd(
       format!("user:{}:servers", user_id),
       server_instance_id
   ).await?;
   ```

### Benchmarks

Il progetto include benchmark in `server/benches/`:

```bash
cd server
cargo bench
```

**Esempio output:**

```
message_insertion       time:   [245.23 µs 248.91 µs 253.02 µs]
user_event_insertion    time:   [189.45 µs 192.13 µs 195.28 µs]
conversation_query      time:   [128.67 µs 131.24 µs 134.12 µs]
```

---

## Testing

### Unit Tests

```bash
# Server tests
cd server
cargo test

# Client tests
cd client
cargo test
```

### Integration Tests

```rust
// server/tests/integration_test.rs
#[tokio::test]
async fn test_user_registration() {
    let db = setup_test_db().await;

    let req = RegisterReq {
        username: "testuser".into(),
        password: "testpass".into(),
    };

    let user_id = UserService::register(&db, &req).await.unwrap();
    assert!(!user_id.is_nil());

    // Verify user exists
    let user = UserRepo::find_by_username(&db, "testuser").await.unwrap();
    assert!(user.is_some());
}

#[tokio::test]
async fn test_message_sequencing() {
    let db = setup_test_db().await;

    let conv_id = create_test_conversation(&db).await;
    let user_id = create_test_user(&db).await;

    // Post 3 messages
    let (msg1_id, seq1) = MessageService::post(&db, conv_id, user_id, "Hello".into()).await.unwrap();
    let (msg2_id, seq2) = MessageService::post(&db, conv_id, user_id, "World".into()).await.unwrap();
    let (msg3_id, seq3) = MessageService::post(&db, conv_id, user_id, "!".into()).await.unwrap();

    // Verify sequential
    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);
    assert_eq!(seq3, 3);
}
```

### Load Testing

Usa **k6** o **wrk** per load testing:

```javascript
// load_test.js (k6)
import http from 'k6/http';
import { check } from 'k6';

export let options = {
  stages: [
    { duration: '30s', target: 50 },
    { duration: '1m', target: 100 },
    { duration: '30s', target: 0 },
  ],
};

export default function () {
  let res = http.post('http://localhost:8080/api/users/login', JSON.stringify({
    username: 'testuser',
    password: 'testpass',
  }), {
    headers: { 'Content-Type': 'application/json' },
  });

  check(res, {
    'status is 200': (r) => r.status === 200,
    'has token': (r) => JSON.parse(r.body).token !== undefined,
  });
}
```

```bash
k6 run load_test.js
```

---


## Conclusioni

**Ruggine** è un sistema di messaggistica completo e ben architettato che dimostra l'utilizzo di moderne tecnologie Rust per applicazioni real-time. L'architettura modulare permette facili estensioni future, mentre il sistema di event sourcing garantisce consistenza e recovery.

### Punti di Forza

- **Type Safety**: SQLx compile-time checked queries
- **Performance**: Async/await con tokio, connection pooling
- **Security**: Argon2, JWT, validazione server-side
- **Real-Time**: WebSocket bidirectional con gap detection
- **Consistency**: Dual sequencing (user-level + conversation-level)
- **Cross-Platform**: GUI nativa con egui (Windows, Linux, macOS)

### Limitazioni Attuali

- **Single Server**: No horizontal scaling (in-memory connections)
- **No Encryption**: Messaggi non E2E encrypted
- **Basic Features**: No file sharing, reactions, calls
- **SQLite**: Non ideale per produzione ad alta scala (considera PostgreSQL)

---

**Versione**: 1.0
**Data**: Dicembre 2025
**Autori**: Team PdS2425-C2
**Licenza**: Consultare LICENSE nel repository
