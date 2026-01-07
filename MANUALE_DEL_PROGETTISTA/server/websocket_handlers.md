# WebSocket Protocol Handlers Documentation

Documentazione completa del **Protocol Layer** WebSocket per Rust Ruggine.

---

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Message Routing](#message-routing)
- [Handler Categories](#handler-categories)
  - [Message Handlers](#message-handlers)
  - [Conversation Handlers](#conversation-handlers)
  - [Group Handlers](#group-handlers)
  - [User Handlers](#user-handlers)
- [Request/Response Schemas](#requestresponse-schemas)
- [Related Documentation](#related-documentation)

---

## Overview

### Purpose

Il **WebSocket Protocol Layer** si occupa di:
- **Routing**: Instradare messaggi client verso handler appropriati
- **Validation**: Validare input e formati richieste
- **Orchestration**: Coordinare business logic, database, e broadcast
- **Response**: Inviare conferme e notifiche ai client

### Location

```
src/web_socket/handlers/
├── mod.rs              # Module organization e re-exports
├── router.rs           # Message routing e dispatching
├── message.rs          # Handler per messaggi chat
├── conversation.rs     # Handler per conversazioni DM
├── group.rs            # Handler per gruppi
└── user.rs             # Handler per operazioni utente
```

### Separation of Concerns

The WebSocket system is organized in three layers:

- **Infrastructure Layer**: Connection management, threading model, broadcast channels (WEBSOCKET_ARCHITECTURE.md)
- **Protocol Layer**: Message routing, validation, orchestration, request/response schemas (this document)
- **Business Logic Layer**: Services, controllers, repositories (APPLICATION_LAYER.md)

Handlers act as the bridge between infrastructure and business logic.

---

## Architecture

### Handler Execution Flow

When a WebSocket message arrives:

1. Client sends JSON message via WebSocket
2. Reader (reader.rs) parses and validates JSON
3. Reader extracts message type and dispatches to router
4. Router (router.rs) routes to appropriate handler
5. Handler validates input and checks authorization
6. Handler calls business logic (Services) and database (Repositories)
7. Handler sends confirmation to user via user_notification_channel
8. Handler broadcasts updates to conversation via conversation_broadcast_channel
9. Writer sends response back to client

### Handler Structure

Ogni handler segue questo pattern:

```rust
pub async fn handle_xxx(
    state: &AppState,           // Database pool + caches
    value: &Value,              // Parsed JSON message
    user_id: Uuid,              // Authenticated user
    out_tx: &mpsc::Sender<OutboundMsg>,  // Optional: response channel
) -> Result<()> {
    // 1. Extract & validate parameters
    let param = value.get("field")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing field".into()))?;
    
    // 2. Authorization check
    let is_authorized = check_permission(state, user_id).await?;
    if !is_authorized {
        return Err(AppError::Forbidden);
    }
    
    // 3. Business logic
    let result = SomeService::do_something(&state.pool, ...).await?;
    
    // 4. Database operations
    sqlx::query("INSERT INTO ...")
        .bind(...)
        .execute(&state.pool)
        .await?;
    
    // 5. Send confirmation
    state.send_sequenced_event_to_user(
        user_id,
        "event_type",
        json!({...}),
        Some(conversation_id)
    ).await?;
    
    // 6. Broadcast to others
    broadcast_to_conversation(state, conversation_id, payload).await?;
    
    Ok(())
}
```

---

## Message Routing

### Router Implementation

**File**: `router.rs`

```rust
pub async fn handle_incoming_message(
    state: &AppState,
    value: &mut Value,
    user_id: Uuid,
    username: &str,
) -> Result<()> {
    let message_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("chat_message");

    match message_type {
        "create_conversation" => handle_create_conversation(state, value, user_id, username).await,
        "chat_message" | "message" => handle_chat_message(state, value, user_id, username).await,
        "invite_user" => handle_invite_user(state, value, user_id).await,
        _ => Err(AppError::BadRequest(format!("Unknown message type: {}", message_type))),
    }
}
```

### Reader Integration

**File**: `reader.rs` (chiamata al router)

```rust
// reader.rs (simplified)
match message_type {
    "subscribe" => handle_subscribe(...).await,
    "ping" => handle_ping(...).await,
    "check_user" => handle_check_user(...).await,
    "create_group" => handle_create_group_with_participants(...).await,
    "mark_read" => handle_mark_read(...).await,
    "delete_message" => handle_delete_message(...).await,
    "delete_user" => handle_delete_user(...).await,
    "leave_group" => handle_leave_group(...).await,
    "remove_member" => handle_remove_member(...).await,
    "delete_conversation" => handle_delete_conversation(...).await,
    "user_events_resume" => handle_user_events_resume_request(...).await,
    
    // Default fallback: router gestisce il resto
    _ => {
        match handlers::handle_incoming_message(
            &state,
            &mut value,
            user_id,
            &username,
        ).await {
            Ok(_) => {},
            Err(e) => {
                // Error handling...
            }
        }
    }
}
```

---

## Handler Categories

### Message Handlers

**File**: `message.rs`

#### `handle_chat_message`

**Purpose**: Crea e invia un nuovo messaggio in una conversazione esistente o crea conversazione DM se necessaria.

**Request Schema**:
```json
{
  "type": "chat_message",
  "content": "Hello!",
  "client_msg_id": "client-uuid-123",  // Optional, per dedup
  "client_temp_id": "temp-uuid-456",   // Optional, per optimistic UI con nuova conversazione
  "target_username": "alice"           // Required solo per nuova DM (con client_temp_id)
}
```

**Note sulla risoluzione conversation_id**:
- Per messaggi in conversazioni esistenti: il sistema risolve automaticamente il `conversation_id` tramite la funzione helper `extract_conversation_id()`
- Per primo messaggio in nuova DM: usa `client_temp_id` + `target_username` per creare la conversazione
- La cache mantiene il mapping `client_temp_id` → `real_conversation_id` per richieste successive

**Flow**:
1. **Check conversazione esistente**: Se `client_temp_id` + `target_username` presenti:
   - Controlla cache: conversazione già creata?
   - Se NO → delega a `handle_message_with_new_conversation`
2. **Extract conversation_id**: Risolve `conversation_id` (anche da temp_id cache)
3. **Validate content**: Content non vuoto
4. **Authorization**: Verifica che user sia participant
5. **Database**:
   - Ottiene `message_sequence` (atomico)
   - Inserisce in `messages` table
   - Auto-marca come letto per l'autore
6. **Cache**: Salva `client_msg_id` → `server_msg_id` mapping
7. **Confirmation**: Invia `message_confirmation` all'autore
8. **User Events**: Batch insert lightweight notifications per altri participant
9. **Broadcast**: Invia messaggio completo via `conversation_broadcast_channel`

**Response (Confirmation)**:
```json
{
  "type": "message_confirmation",
  "client_msg_id": "client-uuid-123",
  "server_msg_id": "uuid-from-db",
  "conversation_id": "uuid",
  "sequence": 42,
  "created_at": 1234567890,
  "status": "saved"
}
```

**Broadcast (to others)**:
```json
{
  "type": "chat_message",
  "id": "uuid",
  "conversation_id": "uuid",
  "author_id": "uuid",
  "author_username": "alice",
  "content": "Hello!",
  "created_at": 1234567890,
  "sequence": 42,
  "client_msg_id": "client-uuid-123"  // Optional
}
```

**Error Cases**:
- `400 Bad Request`: Content vuoto, conversation_id non risolvibile
- `403 Forbidden`: User non è participant della conversazione
- `404 Not Found`: Conversation non esiste

---

#### `handle_mark_read`

**Purpose**: Marca messaggi come letti fino a una certa sequenza.

**Request Schema**:
```json
{
  "type": "mark_read",
  "conversation_id": "uuid",
  "sequence_num": 42
}
```

**Flow**:
1. **Extract parameters**: `conversation_id`, `sequence_num`
2. **Authorization**: Verifica che user sia participant
3. **Database**: Update `participants.last_read_sequence`
4. **No broadcast**: Read status è personale

**Response**: Nessuna (silent operation).

**Error Cases**:
- `400 Bad Request`: Parametri mancanti o invalidi
- `403 Forbidden`: User non è participant

---

#### `handle_delete_message`

**Purpose**: Elimina un messaggio esistente.

**Request Schema**:
```json
{
  "type": "delete_message",
  "mid": "message-uuid"
}
```

**Flow**:
1. **Extract message_id**: Valida UUID
2. **Authorization**: `MessageService.delete_message()` verifica ownership
3. **Database**: Soft delete (imposta `deleted_at`)
4. **Broadcast**: Notifica a tutti i participant via `broadcast_message_deleted`
5. **Acknowledgment**: Invia ACK al richiedente

**Response (ACK)**:
```json
{
  "type": "delete_message_ack",
  "message_id": "uuid",
  "status": "ok"
}
```

**Broadcast (to conversation)**:
```json
{
  "type": "message_deleted",
  "message_id": "uuid",
  "conversation_id": "uuid",
  "deleted_by": "user-uuid",
  "deleted_at": 1234567890
}
```

**Error Cases**:
- `400 Bad Request`: `mid` mancante o invalido
- `403 Forbidden`: User non è autore del messaggio
- `404 Not Found`: Messaggio non esiste

---

### Conversation Handlers

**File**: `conversation.rs`

#### `handle_create_conversation`

**Purpose**: Crea una nuova conversazione DM (Direct Message) con messaggio iniziale opzionale.

**Request Schema**:
```json
{
  "type": "create_conversation",
  "conversation_type": "dm",  // Default, può essere omesso
  "target_username": "bob",
  "initial_message": "Hi Bob!",  // Optional
  "client_msg_id": "client-uuid-123",  // Optional
  "client_temp_id": "temp-uuid-456"    // Optional, per optimistic UI
}
```

**Flow**:
1. **Extract parameters**: `target_username`, `initial_message`, `client_temp_id`
2. **Check cache**: Se `client_temp_id` esiste:
   - Conversazione già creata? → Aggiungi solo messaggio
3. **Validate target**: User esiste? Non sei tu stesso?
4. **Check existing DM**: DM con target già esiste?
5. **Transaction**:
   - Crea `conversations` record
   - Aggiungi 2 `participants` (creator + target)
   - Se `initial_message`: crea messaggio con sequence=1
6. **Cache**: Salva `client_temp_id` → `real_conversation_id`
7. **Commit transaction**
8. **Events**:
   - Invia `conversation_confirmation` al creator (con `client_temp_id`)
   - Invia `new_conversation` al target participant

**Response (Confirmation to creator)**:
```json
{
  "type": "conversation_confirmation",
  "sequence": 15,  // User event sequence
  "event_type": "conversation_confirmation",
  "conversation": {
    "id": "real-uuid",
    "kind": "dm",
    "title": null,
    "display_title": "bob",  // Target username
    "owner_id": "creator-uuid",
    "created_at": 1234567890,
    "message_count": 1,
    "participants": [...],
    "last_message": {...},  // If initial_message provided
    "client_temp_id": "temp-uuid-456"  // ← Key per mapping UI
  }
}
```

**Event (to target participant)**:
```json
{
  "type": "new_conversation",
  "sequence": 42,
  "event_type": "new_conversation",
  "conversation": {
    "id": "real-uuid",
    "kind": "dm",
    "display_title": "alice",  // Creator username
    "owner_id": "creator-uuid",
    "created_at": 1234567890,
    "message_count": 1,
    "participants": [...],
    "last_message": {...}
  }
}
```

**Error Cases**:
- `400 Bad Request`: `target_username` mancante o vuoto, tentativo di DM con sé stesso
- `404 Not Found`: Target user non esiste
- `409 Conflict`: DM con target già esiste

---

#### `handle_message_with_new_conversation`

**Purpose**: Helper interno che gestisce il caso speciale di primo messaggio che deve creare una nuova conversazione.

**When called**: Automaticamente da `handle_chat_message` quando rileva:
- `client_temp_id` presente
- `target_username` presente
- Conversazione non esiste ancora in cache

**Flow**:
1. Estrae parametri da messaggio chat
2. Costruisce richiesta `create_conversation`
3. Delega a `handle_create_conversation`

**Note**: Questo pattern permette optimistic UI sul client:
- Client genera `client_temp_id` localmente
- Invia messaggio con `client_temp_id` + `target_username`
- Server crea conversazione se necessario
- Client riceve confirmation con mapping `client_temp_id` → `real_id`

---

#### `handle_delete_conversation`

**Purpose**: Rimuove user da una conversazione (soft delete per DM, leave per gruppi).

**Request Schema**:
```json
{
  "type": "delete_conversation",
  "conversation_id": "conversation-uuid"
}
```

**Flow**:
1. **Extract conversation_id**: Valida UUID
2. **Authorization**: Verifica che user sia participant
3. **Get conversation kind**: DM vs Group
4. **Delete logic**:
   - **DM**: Rimuove participant record (soft delete conversation se entrambi eliminano)
   - **Group**: Marca participant come left, invia broadcast `user_left`
5. **Notify user**: Invia `conversation_deleted` event
6. **Broadcast** (solo gruppi): Notifica altri membri

**Response (to user)**:
```json
{
  "type": "delete_conversation_ack",
  "conversation_id": "uuid",
  "status": "ok"
}
```

**Error Cases**:
- `400 Bad Request`: `conversation_id` mancante o invalido
- `403 Forbidden`: User non è participant
- `404 Not Found`: Conversazione non esiste

---

### Group Handlers

**File**: `group.rs`

#### `handle_create_group_with_participants`

**Purpose**: Crea un nuovo gruppo con nome e lista iniziale di membri.

**Request Schema**:
```json
{
  "type": "create_group",
  "group_name": "Team Alpha",
  "participant_usernames": ["bob", "charlie", "diana"],
  "client_temp_id": "temp-uuid-789"  // Optional
}
```

**Flow**:
1. **Extract parameters**: `group_name`, `participant_usernames[]`, `client_temp_id`
2. **Create group**: Via `ConversationService.create_group()`
   - Creator diventa owner automaticamente
3. **Cache temp_id**: Salva mapping per confirmation
4. **Resolve usernames**: Converte usernames → user_ids
   - Skip utenti non trovati (con warning)
5. **Add members**: Aggiungi tutti i participant al gruppo
6. **Fetch metadata**: Timestamp creazione
7. **Events**: Invia eventi personalizzati a tutti i participant:
   - **Creator**: `conversation_created_complete` (con `client_temp_id`)
   - **Altri membri**: `new_conversation`

**Response (Confirmation to creator)**:
```json
{
  "type": "conversation_created_complete",
  "sequence": 20,
  "event_type": "conversation_created_complete",
  "conversation": {
    "id": "group-uuid",
    "kind": "group",
    "title": "Team Alpha",
    "owner_id": "creator-uuid",
    "created_at": 1234567890,
    "last_read_sequence": 0,
    "last_msg_seq": 0,
    "message_count": 0,
    "members": [
      {"user_id": "uuid1", "username": "alice", "role": "owner"},
      {"user_id": "uuid2", "username": "bob", "role": "member"},
      ...
    ],
    "client_temp_id": "temp-uuid-789"  // Per mapping UI
  }
}
```

**Event (to other members)**:
```json
{
  "type": "new_conversation",
  "sequence": 35,
  "event_type": "new_conversation",
  "conversation": {
    "id": "group-uuid",
    "kind": "group",
    "title": "Team Alpha",
    "owner_id": "creator-uuid",
    "created_at": 1234567890,
    "last_read_sequence": 0,
    "members": [...]
  }
}
```

**Error Cases**:
- `400 Bad Request`: `group_name` o `participant_usernames` mancanti
- `404 Not Found`: Alcuni usernames non esistono (skip silently)

---

#### `handle_invite_user`

**Purpose**: Invita uno o più utenti a un gruppo esistente.

**Request Schema (Singolo utente)**:
```json
{
  "type": "invite_user",
  "cid": "group-uuid",
  "username": "eve"
}
```

**Request Schema (Multipli utenti - Batch mode)**:
```json
{
  "type": "invite_user",
  "cid": "group-uuid",
  "usernames": ["eve", "frank", "grace"]
}
```

**Note**: L'handler supporta entrambi i formati:
- `username` (string): Invita un singolo utente
- `usernames` (array): Invita multipli utenti in batch
- Gli utenti non trovati vengono saltati con warning, senza bloccare l'operazione

**Flow**:
1. **Extract parameters**: `cid`, `username` o `usernames[]`
2. **Authorization**: Verifica che inviter sia owner del gruppo
3. **Validate group**: Conversazione deve essere di tipo "group"
4. **Resolve usernames**: Converte usernames → user_ids
5. **For each valid user**:
   - Verifica che non sia già membro
   - Aggiungi a `participants` table
   - Invia `new_conversation` event all'utente invitato (con last_message se presente)
   - Invia `member_added` broadcast agli altri membri del gruppo

**Event (to invited user)**:
```json
{
  "type": "new_conversation",
  "sequence": 40,
  "event_type": "new_conversation",
  "conversation": {
    "id": "group-uuid",
    "kind": "group",
    "title": "Team Alpha",
    "owner_id": "owner-uuid",
    "created_at": 1234567890,
    "members": [...],
    "last_message": {  // Include last message preview se esiste
      "id": "msg-uuid",
      "author_id": "uuid",
      "author_username": "alice",
      "content": "Welcome!",
      "created_at": 1234567890,
      "sequence_num": 15
    }
  }
}
```

**Broadcast (to existing group members)**:
```json
{
  "type": "member_added",
  "sequence": 41,
  "event_type": "member_added",
  "conversation_id": "group-uuid",
  "user_id": "new-member-uuid",
  "username": "eve",
  "added_by": "inviter-uuid",
  "timestamp": 1234567890
}
```

**Error Cases**:
- `400 Bad Request`: `cid` mancante, né `username` né `usernames` forniti
- `403 Forbidden`: Inviter non è owner del gruppo
- `404 Not Found`: Gruppo non esiste
- `400 Bad Request`: Tutti gli utenti specificati già membri o non trovati

---

#### `handle_leave_group`

**Purpose**: Utente abbandona un gruppo.

**Request Schema**:
```json
{
  "type": "leave_group",
  "conversation_id": "group-uuid"
}
```

**Flow**:
1. **Extract conversation_id**: Valida UUID
2. **Fetch username**: Recupera username dell'utente dal database
3. **Leave group**: Via `ConversationService.leave_group()`
   - Verifica che user sia member
   - Verifica che conversazione sia di tipo "group"
   - Verifica che owner non stia lasciando (deve prima trasferire ownership)
   - Rimuove participant record
4. **Notify group**: Via `notify_user_left_group()` - invia eventi a tutti i membri rimanenti
5. **Acknowledgment**: Invia ACK al leaving user

**Broadcast (to group)**:
```json
{
  "type": "user_left",
  "conversation_id": "group-uuid",
  "user_id": "leaving-user-uuid",
  "username": "bob",
  "left_at": 1234567890
}
```

**Response (ACK to leaving user)**:
```json
{
  "type": "leave_group_ack",
  "conversation_id": "group-uuid",
  "status": "ok"
}
```

**Error Cases**:
- `400 Bad Request`: `conversation_id` mancante o invalido
- `403 Forbidden`: User non è member, oppure è owner
- `404 Not Found`: Gruppo non esiste

---

#### `handle_remove_member`

**Purpose**: Owner rimuove un membro dal gruppo (kick).

**Request Schema**:
```json
{
  "type": "remove_member",
  "conversation_id": "group-uuid",
  "user_id": "uuid-to-remove"
}
```

**Flow**:
1. **Extract parameters**: `conversation_id`, `user_id`
2. **Validate group**: Conversazione deve essere di tipo "group"
3. **Authorization**: Verifica che requester sia owner del gruppo
4. **Validate target**: 
   - Target è effettivamente member
   - Requester non può rimuovere sé stesso (usare `leave_group`)
5. **Remove member**: Elimina record da `participants` table
6. **Notify removed user**: Invia `member_removed` event all'utente rimosso
7. **Broadcast**: Invia `member_removed` event agli altri membri del gruppo
8. **Acknowledgment**: Invia ACK al requester

**Event (to removed user)**:
```json
{
  "type": "member_removed",
  "sequence": 48,
  "event_type": "member_removed",
  "conversation_id": "group-uuid",
  "removed_user_id": "removed-user-uuid",
  "removed_username": "bob",
  "removed_by": "owner-uuid"
}
```

**Broadcast (to remaining group members)**:
```json
{
  "type": "member_removed",
  "sequence": 49,
  "event_type": "member_removed",
  "conversation_id": "group-uuid",
  "removed_user_id": "removed-user-uuid",
  "removed_username": "bob",
  "removed_by": "owner-uuid"
}
```

**Response (ACK to requester)**:
```json
{
  "type": "remove_member_ack",
  "conversation_id": "group-uuid",
  "removed_user_id": "removed-user-uuid",
  "status": "ok"
}
```

**Error Cases**:
- `400 Bad Request`: Parametri mancanti o invalidi, tentativo di rimuovere sé stesso
- `403 Forbidden`: Requester non è owner
- `404 Not Found`: Gruppo o target user non esiste, target non è member

---

### User Handlers

**File**: `user.rs`

#### `handle_check_user`

**Purpose**: Verifica se un username esiste nel sistema.

**Request Schema**:
```json
{
  "type": "check_user",
  "username": "alice",
  "request_id": "client-request-123"  // Optional, per matching response
}
```

**Flow**:
1. **Extract username**: Case-insensitive
2. **Database query**: Cerca user per username
3. **Response**: Ritorna esistenza + user_id se trovato

**Response**:
```json
{
  "type": "check_user_response",
  "username": "alice",
  "exists": true,
  "user_id": "uuid",  // Presente solo se exists=true
  "request_id": "client-request-123"
}
```

**Error Cases**: Nessuno (sempre risponde con `exists: false` se non trovato).

---

#### `handle_user_events_resume_request`

**Purpose**: Recovery di eventi utente mancanti dopo reconnect o offline period.

**Request Schema**:
```json
{
  "type": "user_events_resume",
  "from_sequence": 42,  // Ultima sequenza nota dal client
  "limit": 100          // Optional, default 100, max 1000
}
```

**Flow**:
1. **Extract parameters**: `from_sequence`, `limit`
2. **Fetch events**: Via `state.get_user_events_since_enriched()`
   - Recupera eventi leggeri da DB
   - **Arricchisce** automaticamente con dati completi (messaggi, conversazioni)
3. **Response**: Invia array di eventi arricchiti

**Response (with events)**:
```json
{
  "type": "user_events_resume",
  "events": [
    {
      "sequence": 43,
      "event_type": "new_message_notification",
      "conversation_id": "uuid",
      "message": {  // ← Arricchito con dati completi
        "id": "msg-uuid",
        "author_id": "uuid",
        "author_username": "bob",
        "content": "Hello!",
        "created_at": 1234567890,
        "sequence_num": 15
      }
    },
    {
      "sequence": 44,
      "event_type": "new_conversation",
      "conversation": {  // ← Arricchito
        "id": "conv-uuid",
        "kind": "dm",
        "display_title": "charlie",
        "participants": [...]
      }
    }
  ],
  "count": 2,
  "timestamp": 1234567890
}
```

**Response (no events)**:
```json
{
  "type": "user_resume_complete",
  "from_sequence": 42,
  "current_sequence": 42,
  "events_count": 0,
  "message": "No events to resume"
}
```

**Error Cases**:
- `500 Internal Server Error`: Failure retrieving events (rare)

**Notes**: Questo handler è **critico** per offline support e reconnect scenarios.

---

#### `handle_delete_user`

**Purpose**: Elimina account utente (GDPR compliance).

**Request Schema**:
```json
{
  "type": "delete_user"
}
```

**Flow**:
1. **Notify participants**: Invia `user_deleted` a tutte le conversazioni dell'utente
2. **Delete user**: Via `UserService.delete_user()`
   - Rimuove participant records
   - Soft delete user record
   - Mantiene messaggi per integrità conversazioni
3. **Confirmation**: Invia ACK al client (prima del disconnect)

**Broadcast (to all user's conversations)**:
```json
{
  "type": "user_deleted",
  "user_id": "deleted-user-uuid",
  "username": "alice",
  "deleted_at": 1234567890,
  "conversation_id": "conv-uuid"
}
```

**Response (to user)**:
```json
{
  "type": "account_deleted_confirm",
  "message": "Account eliminato con successo"
}
```

**Error Cases**:
- `500 Internal Server Error`: Failure during deletion process

**Note**: Dopo questa operazione, la connessione WebSocket viene chiusa.

---

## Request/Response Schemas

### Common Fields

Tutti i messaggi client → server includono:

```json
{
  "type": "message_type",  // Required
  "timestamp": 1234567890,  // Added by reader.rs
  "author_id": "uuid",      // Added by reader.rs
  "author_username": "alice" // Added by reader.rs
}
```

### Common Response Patterns

#### Success (no explicit response)
Handler completa silenziosamente, client riceve solo eventi broadcast.

#### Success (with confirmation)
```json
{
  "type": "xxx_confirmation",
  "status": "ok",
  "...": "operation-specific fields"
}
```

#### Error Response
```json
{
  "type": "error",
  "message": "Human-readable error",
  "error_code": "ERROR_CODE_CONSTANT",
  "client_msg_id": "...",  // If provided in request
  "client_temp_id": "..."  // If provided in request
}
```

---

## Parameter Naming Conventions

### Conversation ID Parameters

La nomenclatura dei parametri per conversation_id varia tra i diversi handler per ragioni storiche:

| Handler | Parameter Name | Reason |
|---------|---------------|---------|
| `handle_invite_user` | `cid` | Compatto, ideale per operazioni gruppi |
| `handle_leave_group` | `conversation_id` | Esplicito, auto-documentante |
| `handle_remove_member` | `conversation_id` | Consistenza con leave_group |
| `handle_delete_conversation` | `conversation_id` | Esplicito per operazione critica |
| `handle_chat_message` | (risolto automaticamente) | Helper `extract_conversation_id()` |

**Note per client developers**: 
- Verificare la sezione specifica di ogni handler per il nome parametro corretto
- Entrambe le forme (`cid` e `conversation_id`) sono semanticamente equivalenti
- Future versioni potrebbero standardizzare su un singolo formato

### User ID Parameters

| Handler | Parameter Name | Context |
|---------|---------------|---------|
| `handle_remove_member` | `user_id` | ID dell'utente da rimuovere |
| Altri handlers | (autenticato) | `user_id` viene dal context di autenticazione |

---





