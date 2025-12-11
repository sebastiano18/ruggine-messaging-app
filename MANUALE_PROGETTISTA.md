# Manuale Progettista - Ruggine Chat

## Indice

1. [Introduzione](#introduzione)
2. [Architettura del Sistema](#architettura-del-sistema)
   - [Panoramica Generale](#panoramica-generale)
   - [Stack Tecnologico](#stack-tecnologico)
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

#### Repositories (Data Access Layer)

Gestiscono le query al database.

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

**Parametri Argon2**:
- **Algorithm**: Argon2id (hybrid mode)
- **Memory**: 64 MB (m=65536 KiB)
- **Iterations**: 3 (t=3)
- **Parallelism**: 4 threads (p=4)

#### JWT Authentication


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

### WebSocket e Real-Time

#### WebSocket Connection Flow

```
CLIENT                           SERVER
  │                                 │
  │  POST /api/users/login          │
  │  Content-Type: application/json │
  │  {"username":"alice",           │
  │   "password":"password123"}     │
  ├────────────────────────────────>│
  │                                 │
  │            200 OK               │
  │  {                              │
  │    "token":"<jwt>",             │
  │    "user_id":"<uuid>",          │
  │    "username":"alice",          │
  │    "last_sequence":42           │
  │  }                              │
  │<────────────────────────────────┤
  │                                 │
  │  GET /ws?session_id=<uuid>      │
  │  Authorization: Bearer <jwt>    │
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



#### Broadcasting



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



#### Gap Detection e Recovery



#### Cleanup Task

Per evitare che la tabella `user_events` cresca all'infinito:



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



**Broadcasting:**


#### 2. User Notification Channels

**Scopo**: Inviare eventi personali a uno specifico utente:
- Conferme invio messaggi (`message_confirmation`)
- Notifiche nuove conversazioni (`new_conversation`)
- Eventi di modifica conversazioni

**Auto-Subscription** all'avvio della connessione:


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


## Sistema Dual-Sequence

Il sistema di sequenze duali garantisce **ordering** e **recovery** attraverso due livelli indipendenti di numerazione.

### Architettura Doppia Sequenza

```
┌──────────────────────────────────────────────────┐
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


### last_read_sequence

Ogni partecipante traccia fino a dove ha letto:



**Update on Mark Read:**


**Calcolo Unread:**


### Recovery dopo Disconnessione

**Client Request:**


**Server Query:**


**Server Response:**


### Gap Detection (Client-Side)



### Initial State Loading

All'avvio della connessione, il server invia lo stato completo:


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



**Caratteristiche:**
- Ogni task è M:N green thread
- Scheduling cooperativo (yield points su `.await`)
- Work-stealing scheduler

### select! Macro



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



**Timeouts applicati:**
- `ws_tx.send()`: 10 secondi
- `ws_tx.close()`: 5 secondi
- Heartbeat send: 5 secondi

#### Consecutive Failures Tracking



**Filosofia**: Tollera errori temporanei, ma chiude se persistenti.

### Broadcast Errors



**Comportamento**: Channel dropped è normale se tutti disconnessi.

**Recovery**: Messaggi salvati in `user_events` → delivery garantita.


**Recovery Client-Side:**


### Database Errors

#### Transaction Rollback


**Su errore**: Rollback automatico, stato DB consistente.

#### Retry with Backoff


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
│  │  - Modals (Create DM, Create Group, etc.)        │  │
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



#### UIState


### WebSocket Manager

Gestisce il lifecycle della connessione WebSocket.


#### Health Monitor


### Event Dispatching

Il sistema usa un **event bus** per gestire gli aggiornamenti dello stato.





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

#### GET /ws?session_id=\<uuid\>

Esegue l’handshake di upgrade e stabilisce una connessione WebSocket bidirezionale autenticata.

**Request:**

no body

**Response (101):**

no body

---

### Aggiunta Membri ai Gruppi (WebSocket)


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

