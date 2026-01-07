# SHARED SERVICES & REPOSITORIES

## Overview
Questo documento descrive i layer **Service** e **Repository** che sono **condivisi** tra l'architettura HTTP e l'architettura WebSocket.

---

## FILOSOFIA DEL DESIGN

### Service Layer
**Responsabilità**:
- Logica business
- Transazioni complesse
- Validazioni business
- Orchestrazione repository
- Broadcasting eventi (via AppState)

**NON contiene**:
- Logica HTTP-specifica (parsing parametri, headers, etc.)
- Logica WebSocket-specifica (connessioni, ping/pong, etc.)

### Repository Layer
**Responsabilità**:
- Accesso diretto al database
- Query SQL
- Mapping row → struct
- Operazioni CRUD atomiche

**NON contiene**:
- Validazioni business
- Logica di autorizzazione
- Broadcasting

---

## DATA STRUCTURES

### Core Models

#### `Message`
```rust
pub struct Message {
    pub id: Uuid,
    pub author_id: Uuid,
    pub conversation_id: Uuid,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub sequence_num: Option<i64>,
}
```

**Utilizzo**: Rappresentazione completa di un messaggio con tutti i metadati.

---

#### `ConversationWithLastMessage`
```rust
pub struct ConversationWithLastMessage {
    pub id: Uuid,
    pub kind: String,              // "dm" | "group"
    pub title: String,             // Nome gruppo o username altro partecipante
    pub owner_id: Uuid,
    pub created_at: i64,
    pub last_read_sequence: i64,
    pub last_activity: i64,
    pub last_msg_seq: i64,
    // Campi last message (tutti Option)
    pub last_msg_id: Option<Uuid>,
    pub last_msg_author_id: Option<Uuid>,
    pub last_msg_author_username: Option<String>,
    pub last_msg_content: Option<String>,
    pub last_msg_timestamp: Option<i64>,
    pub last_msg_sequence: Option<i64>,
    // Membri (solo per gruppi)
    pub members: Option<Vec<ParticipantInfo>>,
}
```

**Utilizzo**: Ritornata da `ConversationRepo::get_conversations` e `get_single_conversation`. Include informazioni sull'ultimo messaggio inline.

---

#### `ConversationOut`
```rust
pub struct ConversationOut {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub owner_id: Uuid,
    pub created_at: i64,
    pub last_read_sequence: i64,
    pub last_activity: i64,
    pub last_msg_seq: i64,
}
```

**Utilizzo**: Versione semplificata senza ultimo messaggio, usata in `ConversationSummary`.

---

#### `ConversationSummary`
```rust
pub struct ConversationSummary {
    pub conversation: ConversationOut,
    pub last_message: Option<Message>,
    pub members: Option<Vec<ParticipantOut>>,
}
```

**Utilizzo**: Struttura finale ritornata ai client HTTP. Separa conversazione, ultimo messaggio e membri.

---

#### `PaginatedConversationsResponse`
```rust
pub struct PaginatedConversationsResponse {
    pub conversations: Vec<ConversationSummary>,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
}
```

**Utilizzo**: Wrapper per paginazione cursor-based delle conversazioni.

---

#### `ParticipantInfo`
```rust
pub struct ParticipantInfo {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,           // "owner" | "member"
    pub joined_at: Option<i64>,
}
```

**Utilizzo**: Informazioni partecipante con timestamp di join, usata in `ConversationWithLastMessage`.

---

#### `ParticipantOut`
```rust
pub struct ParticipantOut {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
}
```

**Utilizzo**: Versione semplificata senza `joined_at`, usata in `ConversationSummary`.

---

## REPOSITORIES

### UserRepo

#### `find_by_name`
```rust
pub async fn find_by_name(
    pool: &SqlitePool,
    username: &str
) -> Result<Option<(Uuid, String)>>
```

**Ritorna**: `(user_id, password_hash)` se trovato

**Usato da**:
- `UserService::login` (HTTP e WS)
- `UserService::register` (HTTP)

---

#### `create`
```rust
pub async fn create(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str
) -> Result<Uuid>
```

**Ritorna**: ID del nuovo utente

**Usato da**:
- `UserService::register` (HTTP)

---

#### `delete_user_cascade`
```rust
pub async fn delete_user_cascade(
    pool: &SqlitePool,
    user_id: Uuid
) -> Result<()>
```

**Operazioni** (in transazione):
1. Elimina `user_events` e `user_sequences` dell'utente
2. Elimina `message_sequences` delle conversazioni che saranno eliminate
3. Elimina tutte le DM a cui partecipa
4. Elimina l'utente (CASCADE elimina: gruppi owner, participants, messages, invites)

**Usato da**:
- `UserService::delete_user` (WS)

---

### MessageRepo

#### `insert`
```rust
pub async fn insert(
    pool: &SqlitePool,
    conversation_id: Uuid,
    author_id: Uuid,
    content: &str,
) -> Result<Uuid>
```

**Operazioni**:
1. Valida che content non sia vuoto
2. Calcola `next_sequence` per la conversazione:
   ```sql
   SELECT COALESCE(MAX(sequence_num), 0) + 1
   FROM messages
   WHERE conversation_id = ?
   ```
3. Inserisce messaggio con sequence incrementale
4. Ritorna `message_id`

**Usato da**:
- `WebSocket::handle_send_message` (WS)

**Note**: La sequence è **per-conversation**, non globale.

---

#### `delete`
```rust
pub async fn delete(
    pool: &SqlitePool,
    message_id: Uuid,
    author_id: Uuid,
) -> Result<u64>
```

**Ritorna**: Numero di righe eliminate (0 o 1)

**Usato da**:
- `MessageService::delete_message` (WS)

---

#### `get_metadata`
```rust
pub async fn get_metadata(
    pool: &SqlitePool,
    message_id: Uuid,
) -> Result<Option<(Uuid, Uuid)>>
```

**Ritorna**: `(author_id, conversation_id)` se il messaggio esiste

**Usato da**:
- `MessageService::delete_message` (WS) - per verificare ownership e conversation_id

---

#### `get_sequence`
```rust
pub async fn get_sequence(
    pool: &SqlitePool,
    message_id: Uuid,
) -> Result<Option<i64>>
```

**Ritorna**: `sequence_num` del messaggio

**Usato da**:
- Broadcasting eventi (WS) - per informare i client della sequence

---

### ConversationRepo

#### `is_participant`
```rust
pub async fn is_participant(
    pool: &SqlitePool,
    conversation_id: Uuid,
    user_id: Uuid,
) -> Result<bool>
```

**Verifica**: Se l'utente è nella tabella `participants` per questa conversazione

**Usato da**:
- `MessageController::list` (HTTP) - autorizzazione
- `WebSocket::handle_send_message` (WS) - autorizzazione

---

#### `is_owner`
```rust
pub async fn is_owner(
    pool: &SqlitePool,
    conversation_id: Uuid,
    user_id: Uuid,
) -> Result<bool>
```

**Verifica**: Se l'utente è l'owner del gruppo

**Usato da**:
- `ConversationService::add_member` (WS)
- `ConversationService::delete_conversation` (WS)

---

#### `get_conversation_kind`
```rust
pub async fn get_conversation_kind(
    pool: &SqlitePool,
    conversation_id: Uuid,
) -> Result<Option<String>>
```

**Ritorna**: `"dm"` o `"group"`

**Usato da**:
- `ConversationService::delete_conversation` (WS) - logica autorizzazione diversa per DM vs gruppi
- `ConversationService::leave_group` (WS) - verifica che sia un gruppo

---

#### `list_participant_ids`
```rust
pub async fn list_participant_ids(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    conversation_id: Uuid,
) -> Result<Vec<Uuid>>
```

**Ritorna**: Lista user_id di tutti i partecipanti

**Usato da**:
- Broadcasting eventi (WS) - per sapere a chi inviare notifiche

---

#### `get_conversations`
```rust
pub async fn get_conversations(
    pool: &SqlitePool,
    user_id: Uuid,
    limit: i32,
    before: Option<i64>
) -> Result<Vec<ConversationWithLastMessage>>
```

**Query**: Esistono DUE versioni della query a seconda del parametro `before`:

**Versione CON paginazione** (`before` presente):
```sql
SELECT
    c.id,
    c.kind,
    CASE
        WHEN c.kind = 'group' THEN COALESCE(c.title, 'Gruppo')
        WHEN c.kind = 'dm' THEN (
            COALESCE(
                -- Cerca username dall'altro partecipante
                (SELECT u.username
                 FROM participants p2
                 JOIN users u ON p2.user_id = u.id
                 WHERE p2.conversation_id = c.id AND p2.user_id != ?
                 LIMIT 1),
                -- Fallback: cerca dall'ultimo messaggio
                (SELECT u.username
                 FROM messages m
                 JOIN users u ON m.author_id = u.id
                 WHERE m.conversation_id = c.id AND m.author_id != ?
                 ORDER BY m.created_at DESC
                 LIMIT 1),
                'Utente sconosciuto'
            )
        )
        ELSE 'Unknown'
    END AS title,
    c.owner_id,
    c.created_at,
    p.last_read_sequence,
    COALESCE(MAX(ms.last_updated, p.joined_at), c.created_at) as last_activity,
    COALESCE(ms.current_sequence, 0) as last_msg_seq,
    last_m.id as last_msg_id,
    last_m.author_id as last_msg_author_id,
    last_u.username as last_msg_author_username,
    last_m.content as last_msg_content,
    last_m.created_at as last_msg_timestamp,
    last_m.sequence_num as last_msg_sequence
FROM conversations c
JOIN participants p ON c.id = p.conversation_id
LEFT JOIN message_sequences ms ON c.id = ms.conversation_id
LEFT JOIN messages last_m ON c.id = last_m.conversation_id
    AND last_m.sequence_num = ms.current_sequence
LEFT JOIN users last_u ON last_m.author_id = last_u.id
WHERE p.user_id = ?
  AND COALESCE(MAX(ms.last_updated, p.joined_at), c.created_at) < ?  -- Filtro paginazione
ORDER BY last_activity DESC
LIMIT ?
```

**Versione SENZA paginazione** (`before` è None):
```sql
-- Stessa query ma SENZA la riga:
--   AND COALESCE(MAX(ms.last_updated, p.joined_at), c.created_at) < ?
```

**Caratteristiche**:
- **Title dinamico per DM**: Recupera automaticamente l'username dell'altro partecipante
- **last_activity**: Calcolato come MAX tra `message_sequences.last_updated` e `participants.joined_at`
- **Last message**: Join con `message_sequences` per ottenere l'ultimo messaggio via `current_sequence`
- **Batch load members**: Dopo la query principale, carica i membri SOLO per i gruppi in una query separata

**Post-processing**:
Dopo la query principale, viene eseguita una seconda query per caricare i membri dei gruppi:
```sql
SELECT
    p.conversation_id,
    p.user_id,
    u.username,
    p.role,
    p.joined_at
FROM participants p
INNER JOIN users u ON p.user_id = u.id
INNER JOIN conversations c ON p.conversation_id = c.id
WHERE p.conversation_id IN (?, ?, ...)
  AND c.kind = 'group'
ORDER BY p.conversation_id,
         CASE WHEN LOWER(p.role) = 'owner' THEN 0 ELSE 1 END,
         u.username
```

**Paginazione**: Cursor-based con `last_activity`

**Usato da**:
- `ConversationController::get_conversations` (HTTP)

---

#### `get_single_conversation`
```rust
pub async fn get_single_conversation(
    pool: &sqlx::SqlitePool,
    conversation_id: Uuid,
    user_id: Uuid,
) -> Result<Option<ConversationWithLastMessage>>
```

**Query principale**:
```sql
SELECT 
    c.id,
    c.kind,
    CASE
        WHEN c.kind = 'group' THEN COALESCE(c.title, 'Gruppo')
        WHEN c.kind = 'dm' THEN (
            COALESCE(
                (SELECT u.username
                 FROM participants p2
                 JOIN users u ON p2.user_id = u.id
                 WHERE p2.conversation_id = c.id AND p2.user_id != ?
                 LIMIT 1),
                'Utente sconosciuto'
            )
        )
        ELSE 'Unknown'
    END AS title,
    c.owner_id,
    c.created_at,
    p.last_read_sequence,
    COALESCE(ms.last_updated, p.joined_at, c.created_at) as last_activity,
    COALESCE(ms.current_sequence, 0) as last_msg_seq,
    last_m.id as last_msg_id,
    last_m.author_id as last_msg_author_id,
    last_u.username as last_msg_author_username,
    last_m.content as last_msg_content,
    last_m.created_at as last_msg_timestamp,
    last_m.sequence_num as last_msg_sequence
FROM conversations c
JOIN participants p ON c.id = p.conversation_id
LEFT JOIN message_sequences ms ON c.id = ms.conversation_id
LEFT JOIN messages last_m ON c.id = last_m.conversation_id 
    AND last_m.sequence_num = ms.current_sequence
LEFT JOIN users last_u ON last_m.author_id = last_u.id
WHERE c.id = ?
  AND p.user_id = ?
```

**Post-processing**: Se la conversazione è un gruppo, carica i membri con query separata:
```sql
SELECT
    p.user_id,
    u.username,
    p.role,
    p.joined_at
FROM participants p
INNER JOIN users u ON p.user_id = u.id
WHERE p.conversation_id = ?
ORDER BY CASE WHEN LOWER(p.role) = 'owner' THEN 0 ELSE 1 END,
         u.username
```

**Caratteristiche**:
- Verifica automaticamente che `user_id` sia partecipante (JOIN con participants)
- Popola `members` SOLO per gruppi (non per DM)
- Calcola `last_activity` come `COALESCE(ms.last_updated, p.joined_at, c.created_at)`
- Ritorna `None` se conversazione non esiste o utente non è partecipante

**Usato da**:
- `ConversationService::get_conversation` (HTTP/WS)

---

#### `by_user`
```rust
pub async fn by_user(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<Vec<(Uuid, String, String, Uuid, i64, i64, i64, i64)>>
```

**Ritorna**: Tupla `(conv_id, kind, display_title, owner_id, created_at, last_read_sequence, last_activity, last_msg_seq)`

**Query**:
```sql
SELECT
    c.id,
    c.kind,
    CASE
        WHEN c.kind = 'group' THEN c.title
        WHEN c.kind = 'dm' THEN (
            COALESCE(
                (SELECT u.username
                 FROM participants p2
                 JOIN users u ON p2.user_id = u.id
                 WHERE p2.conversation_id = c.id AND p2.user_id != ?
                 LIMIT 1),
                (SELECT u.username
                 FROM messages m
                 JOIN users u ON m.author_id = u.id
                 WHERE m.conversation_id = c.id AND m.author_id != ?
                 ORDER BY m.created_at DESC
                 LIMIT 1)
            )
        )
        ELSE 'Unknown'
    END AS display_title,
    c.owner_id,
    c.created_at,
    COALESCE(p.last_read_sequence, 0) AS last_read_sequence,
    COALESCE(
        (SELECT MAX(m.created_at) 
         FROM messages m 
         WHERE m.conversation_id = c.id),
        c.created_at
    ) AS last_activity,
    COALESCE(
        (SELECT current_sequence 
         FROM message_sequences 
         WHERE conversation_id = c.id),
        0
    ) AS last_msg_seq
FROM conversations c
LEFT JOIN participants p ON c.id = p.conversation_id AND p.user_id = ?
WHERE p.user_id = ?
   OR (c.kind = 'dm' AND EXISTS(
        SELECT 1 FROM messages m 
        WHERE m.conversation_id = c.id AND m.author_id = ?
   ))
ORDER BY last_activity DESC
```

**Caratteristiche importanti**:
- Include DM anche se l'utente non è in `participants` ma ha inviato messaggi
- Calcola `last_activity` come `MAX(m.created_at)` dai messaggi (diverso da `get_conversations`)
- NON popola i membri (struttura più leggera)

**Usato da**:
- `UserService::notify_participants_of_deleted_user` (WS) - per trovare tutte le conversazioni dell'utente prima della cancellazione

---

#### `create_group`
```rust
pub async fn create_group(
    pool: &SqlitePool,
    name: &str,
    owner_id: Uuid
) -> Result<Uuid>
```

**Operazioni**:
1. Genera nuovo UUID per la conversazione
2. Salva timestamp corrente per `created_at` e `joined_at`
3. Inserisce conversazione:
   ```sql
   INSERT INTO conversations (id, kind, title, owner_id, created_at)
   VALUES (?, 'group', ?, ?, ?)
   ```
4. Inserisce owner come partecipante:
   ```sql
   INSERT INTO participants (conversation_id, user_id, role, joined_at)
   VALUES (?, ?, 'owner', ?)
   ```

**Note importanti**:
- **NON usa transazioni esplicite** (BEGIN/COMMIT) - si affida alle garanzie di atomicità di SQLite per singole operazioni
- Usa lo **stesso timestamp** per `created_at` e `joined_at` per consistenza
- L'owner viene automaticamente aggiunto come partecipante con `role = 'owner'`

**Usato da**:
- `WebSocket::handle_create_group` (WS)

---

#### `add_member`
```rust
pub async fn add_member(
    pool: &SqlitePool,
    conversation_id: Uuid,
    user_id: Uuid
) -> Result<()>
```

**Operazioni**:
1. Genera timestamp corrente per `joined_at`
2. Inserisce nuovo partecipante:
   ```sql
   INSERT INTO participants (conversation_id, user_id, role, joined_at, last_read_sequence)
   VALUES (?, ?, 'member', ?, 0)
   ```

**Campi inizializzati**:
- `role`: Sempre `'member'` (non `'owner'`)
- `joined_at`: Timestamp corrente dell'operazione
- `last_read_sequence`: Inizializzato a `0` (nessun messaggio letto ancora)

**Usato da**:
- `ConversationService::add_member` (WS)

---

#### `remove_member`
```rust
pub async fn remove_member(
    pool: &SqlitePool,
    conversation_id: Uuid,
    user_id: Uuid
) -> Result<()>
```

**Elimina**: Riga da `participants`

**Usato da**:
- `ConversationService::leave_group` (WS)

---

#### `delete_conversation`
```rust
pub async fn delete_conversation(
    pool: &SqlitePool,
    conversation_id: Uuid
) -> Result<()>
```

**Elimina**: Conversazione (CASCADE elimina participants, messages, etc.)

**Usato da**:
- `ConversationService::delete_conversation` (WS)

---

## SERVICES

### UserService

#### `register`
```rust
pub async fn register(
    pool: &sqlx::SqlitePool,
    username: &str,
    password: &str
) -> Result<Uuid>
```

**Flusso**:
1. Valida input:
   - Username non vuoto
   - Password >= 4 caratteri
2. Verifica username non esistente con `UserRepo::find_by_name`
3. Hash password con Argon2
4. Crea utente con `UserRepo::create`

**Usato da**:
- `UserController::register` (HTTP)

**Errori**:
- `400 Bad Request`: Input invalido
- `409 Conflict`: Username già esistente

---

#### `login`
```rust
pub async fn login(
    pool: &sqlx::SqlitePool,
    jwt_secret: &str,
    username: &str,
    password: &str,
) -> Result<(String, Uuid)>
```

**Flusso**:
1. Valida input non vuoto
2. Cerca utente con `UserRepo::find_by_name`
3. Verifica password con Argon2
4. Genera JWT token (exp: 24h)

**Ritorna**: `(token, user_id)`

**Response HTTP**: `UserController::login` ritorna `LoginResp`:
```rust
pub struct LoginResp {
    pub token: String,
    pub user_id: Uuid,
    pub username: String,
    pub last_sequence: u64,  // Sequenza corrente dell'utente per sincronizzazione eventi
}
```

**Note**: Il campo `last_sequence` viene recuperato tramite `AppState::get_current_user_sequence(user_id)` e rappresenta l'ultima sequenza di eventi elaborati dall'utente. Se il recupero fallisce, viene impostato a 0.

**Usato da**:
- `UserController::login` (HTTP)

**Errori**:
- `400 Bad Request`: Input vuoto
- `401 Unauthorized`: Username o password errati

---

#### `get_user_id_by_username`
```rust
pub async fn get_user_id_by_username(
    pool: &sqlx::SqlitePool, 
    username: &str
) -> Result<Uuid>
```

**Flusso**:
1. Cerca utente con `UserRepo::find_by_name`
2. Ritorna `user_id` o `404 Not Found`

**Usato da**:
- Operazioni interne che necessitano di risolvere username → user_id

**Errori**:
- `404 Not Found`: Username non esiste

---

#### `delete_user`
```rust
pub async fn delete_user(
    pool: &sqlx::SqlitePool,
    user_id: Uuid
) -> Result<()>
```

**Flusso**:
1. Delega a `UserRepo::delete_user_cascade`
2. Elimina tutto in transazione

**Usato da**:
- `WebSocket::handle_delete_account` (WS)

---

#### `notify_participants_of_deleted_user`
```rust
pub async fn notify_participants_of_deleted_user(
    state: &AppState,
    deleted_user_id: Uuid,
) -> Result<()>
```

**Flusso**:
1. Recupera tutte le conversazioni dell'utente
2. Per ogni conversazione:
   - **DM**: Notifica `conversation_deleted` (reason: "user_deleted")
   - **Gruppo owner**: Notifica `conversation_deleted` (reason: "owner_deleted")
   - **Gruppo member**: Notifica `user_deleted_account`

**Usato da**:
- `WebSocket::handle_delete_account` (WS) - dopo aver eliminato l'utente

---

### MessageService

#### `list`
```rust
pub async fn list(
    pool: &SqlitePool,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<(Uuid, Uuid, String, String, i64, Option<i64>)>>
```

**Query**:
```sql
SELECT m.id, m.author_id, u.username, m.content, m.created_at, m.sequence_num
FROM messages m
JOIN users u ON m.author_id = u.id
WHERE m.conversation_id = ?
ORDER BY COALESCE(m.sequence_num, m.created_at) ASC
LIMIT ?
```

**Ritorna**: Tuple `(id, author_id, username, content, created_at, sequence_num)`

**Limite**: 1-200 messaggi

**Usato da**:
- `MessageController::list` (HTTP) - quando NON c'è `before_sequence`

---

#### `list_with_pagination`
```rust
pub async fn list_with_pagination(
    pool: &SqlitePool,
    conversation_id: Uuid,
    limit: i64,
    before_sequence: Option<i64>,
) -> Result<Vec<Message>>
```

**Query** (se `before_sequence` presente):
```sql
SELECT m.id, m.author_id, u.username, m.content, m.created_at, m.sequence_num
FROM messages m
JOIN users u ON m.author_id = u.id
WHERE m.conversation_id = ? AND m.sequence_num < ?
ORDER BY m.sequence_num DESC
LIMIT ?
```

**Query** (se `before_sequence` assente - ultimi messaggi):
```sql
SELECT m.id, m.author_id, u.username, m.content, m.created_at, m.sequence_num
FROM messages m
JOIN users u ON m.author_id = u.id
WHERE m.conversation_id = ?
ORDER BY m.sequence_num DESC
LIMIT ?
```

**Paginazione**: Carica messaggi con sequence < `before_sequence`

**Ordine ritorno**: Invertito (`.rev()`) per essere cronologico

**Limite**: 1-100 messaggi (diverso da `list` che ha limite 200)

**Differenze da `list`**:
- Ritorna `Vec<Message>` invece di tuple
- Ordina per `sequence_num DESC` poi inverte
- Supporta paginazione con `before_sequence`
- Limite minore (100 vs 200)

**Usato da**:
- `MessageController::list` (HTTP) - quando c'è `before_sequence`

---

#### `delete_message`
```rust
pub async fn delete_message(
    pool: &SqlitePool,
    message_id: Uuid,
    requester_id: Uuid,
) -> Result<Uuid>
```

**Flusso**:
1. Recupera metadata con `MessageRepo::get_metadata`
2. Verifica ownership: `author_id == requester_id`
3. Elimina con `MessageRepo::delete`
4. Ritorna `conversation_id` per broadcasting

**Usato da**:
- `WebSocket::handle_delete_message` (WS)

**Errori**:
- `404 Not Found`: Messaggio non esiste
- `403 Forbidden`: Utente non è l'autore

---

### ConversationService

#### `get_conversation`
```rust
pub async fn get_conversation(
    pool: &sqlx::SqlitePool,
    conversation_id: Uuid,
    user_id: Uuid
) -> Result<Option<ConversationSummary>>
```

**Flusso**:
1. Chiama `ConversationRepo::get_single_conversation` per recuperare dati
2. Mappa `ConversationWithLastMessage` → `ConversationSummary`:
   - Costruisce `ConversationOut` dai campi base
   - Costruisce `Message` se `last_msg_id` presente
   - Mappa `members` da `Vec<ParticipantInfo>` → `Vec<ParticipantOut>`

**Usato da**:
- `ConversationController::get_conversation` (HTTP)

**Errori**:
- `404 Not Found`: Conversazione non esiste o utente non è partecipante

---

#### `get_conversations`
```rust
pub async fn get_conversations(
    pool: &sqlx::SqlitePool,
    user_id: Uuid,
    limit: i32,
    before: Option<i64>
) -> Result<PaginatedConversationsResponse>
```

**Flusso**:
1. Recupera conversazioni con `ConversationRepo::get_conversations`
2. Mappa a `Vec<ConversationSummary>`
3. Calcola `next_cursor` e `has_more`

**Ritorna**:
```rust
PaginatedConversationsResponse {
    conversations: Vec<ConversationSummary>,
    next_cursor: Option<i64>,  // last_activity dell'ultima conversazione
    has_more: bool,            // true se len == limit
}
```

**Usato da**:
- `ConversationController::get_conversations` (HTTP)

---

#### `create_group`
```rust
pub async fn create_group(
    pool: &sqlx::SqlitePool,
    name: &str,
    owner_id: Uuid
) -> Result<Uuid>
```

**Flusso**: Delega a `ConversationRepo::create_group`

**Usato da**:
- `WebSocket::handle_create_group` (WS)

---

#### `add_member`
```rust
pub async fn add_member(
    pool: &sqlx::SqlitePool,
    conversation_id: Uuid,
    member_id: Uuid,
    requester_id: Uuid
) -> Result<()>
```

**Flusso**:
1. Verifica che `requester_id` sia owner con `ConversationRepo::is_owner`
2. Aggiunge membro con `ConversationRepo::add_member`

**Usato da**:
- `WebSocket::handle_add_member` (WS)

**Errori**:
- `401 Unauthorized`: Requester non è owner

---

#### `delete_conversation`
```rust
pub async fn delete_conversation(
    pool: &sqlx::SqlitePool,
    conversation_id: Uuid,
    requester_id: Uuid,
) -> Result<()>
```

**Flusso**:
1. Recupera `kind` con `ConversationRepo::get_conversation_kind`
2. Autorizzazione:
   - **Gruppo**: Solo owner può eliminare
   - **DM**: Entrambi i partecipanti possono eliminare
3. Elimina con `ConversationRepo::delete_conversation`

**Usato da**:
- `WebSocket::handle_delete_conversation` (WS)

**Errori**:
- `404 Not Found`: Conversazione non esiste
- `401 Unauthorized`: Utente non autorizzato

---

#### `leave_group`
```rust
pub async fn leave_group(
    pool: &sqlx::SqlitePool,
    conversation_id: Uuid,
    requester_id: Uuid,
) -> Result<()>
```

**Flusso**:
1. Verifica che sia un gruppo
2. Verifica che requester NON sia owner (owner deve eliminare il gruppo)
3. Verifica che sia partecipante
4. Rimuove con `ConversationRepo::remove_member`

**Usato da**:
- `WebSocket::handle_leave_group` (WS)

**Errori**:
- `400 Bad Request`: Non è un gruppo, o owner tenta di uscire
- `401 Unauthorized`: Non è partecipante

---

#### `list_participant_ids`
```rust
pub async fn list_participant_ids(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    conversation_id: Uuid,
) -> Result<Vec<Uuid>>
```

**Flusso**: Delega a `ConversationRepo::list_participant_ids`

**Usato da**:
- Tutte le funzioni di broadcasting (WS)

---

#### Broadcasting Functions

Queste funzioni inviano eventi WebSocket ai partecipanti tramite `send_event_to_multiple_users`.

##### `broadcast_message_deleted`
```rust
pub async fn broadcast_message_deleted(
    st: &AppState,
    conversation_id: Uuid,
    message_id: Uuid,
    participant_ids: Vec<Uuid>,
)
```

**Evento**:
```json
{
  "type": "message_deleted",
  "message_id": "...",
  "conversation_id": "..."
}
```

**Usato da**: `WebSocket::handle_delete_message` (WS)

---

##### `broadcast_conversation_deleted`
```rust
pub async fn broadcast_conversation_deleted(
    st: &AppState,
    conversation_id: Uuid,
    by: Uuid,
    participant_ids: Vec<Uuid>,
    include_author: bool,
)
```

**Evento**:
```json
{
  "type": "conversation_deleted",
  "conversation_id": "...",
  "by": "...",
  "timestamp": 1703001234
}
```

**Usato da**: `WebSocket::handle_delete_conversation` (WS)

---

##### `notify_user_left_group`
```rust
pub async fn notify_user_left_group(
    state: &AppState,
    conversation_id: Uuid,
    user_id: Uuid,
    username: &str,
) -> Result<()>
```

**Evento**:
```json
{
  "type": "user_left_group",
  "conversation_id": "...",
  "user_id": "...",
  "username": "alice",
  "timestamp": 1703001234
}
```

**Usato da**: `WebSocket::handle_leave_group` (WS)

---

##### `notify_user_deleted_account`
```rust
pub async fn notify_user_deleted_account(
    state: &AppState,
    conversation_id: Uuid,
    deleted_user_id: Uuid,
    deleted_username: &str,
) -> Result<()>
```

**Evento**:
```json
{
  "type": "user_deleted_account",
  "conversation_id": "...",
  "deleted_user_id": "...",
  "deleted_username": "alice"
}
```

**Usato da**: `UserService::notify_participants_of_deleted_user` (WS)

---

##### `notify_conversation_deleted`
```rust
pub async fn notify_conversation_deleted(
    state: &AppState,
    conversation_id: Uuid,
    deleted_user_id: Uuid,
    reason: &str,
) -> Result<()>
```

**Evento**:
```json
{
  "type": "conversation_deleted",
  "conversation_id": "...",
  "reason": "user_deleted" | "owner_deleted",
  "deleted_user_id": "..."
}
```

**Usato da**: `UserService::notify_participants_of_deleted_user` (WS)

---

### ParticipantService

#### `mark_read`
```rust
pub async fn mark_read(
    pool: &SqlitePool,
    conversation_id: Uuid,
    user_id: Uuid,
    sequence_num: i64,
) -> Result<()>
```

**Query**:
```sql
UPDATE participants
SET last_read_sequence = MAX(last_read_sequence, ?)
WHERE conversation_id = ? AND user_id = ?
```

**Note**: Usa `MAX` per evitare che last_read_sequence diminuisca

**Usato da**:
- `WebSocket::handle_mark_read` (WS)

---

## PATTERN DI USO

### Da HTTP Controller
```rust
// Autorizzazione
let is_participant = ConversationRepo::is_participant(&pool, conv_id, user_id).await?;
if !is_participant {
    return Err(AppError::Forbidden);
}

// Business logic
let messages = MessageService::list(&pool, conv_id, 50).await?;

// Response
Ok(Json(messages))
```

### Da WebSocket Handler
```rust
// Validazione
if content.trim().is_empty() {
    return Err("Empty message");
}

// Business logic
let msg_id = MessageRepo::insert(&pool, conv_id, author_id, &content).await?;

// Broadcasting
let participants = ConversationService::list_participant_ids(&pool, conv_id).await?;
broadcast_to_conversation(&state, conv_id, event).await;
```

---

## TRANSAZIONI

### Quando usare transazioni

**UserRepo::delete_user_cascade**:
```rust
let mut tx = pool.begin().await?;

// 1. Elimina user_events
sqlx::query("DELETE FROM user_events WHERE user_id = ?")
    .execute(&mut *tx).await?;

// 2. Elimina DM
sqlx::query("DELETE FROM conversations WHERE kind = 'dm' AND ...")
    .execute(&mut *tx).await?;

// 3. Elimina utente
sqlx::query("DELETE FROM users WHERE id = ?")
    .execute(&mut *tx).await?;

tx.commit().await?;
```

**Note su ConversationRepo::create_group**: Questa funzione NON usa transazioni esplicite - esegue due INSERT separati affidandosi alle garanzie di atomicità di SQLite per singole operazioni. In caso di fallimento della seconda INSERT, la conversazione rimarrebbe senza owner (problema noto, ma accettabile per la semplicità del design attuale).

---

## BEST PRACTICES

### 1. Repository puro
```rust
// ✅ CORRETTO - solo accesso dati
pub async fn insert(pool: &SqlitePool, ...) -> Result<Uuid> {
    sqlx::query("INSERT ...").execute(pool).await?;
    Ok(id)
}

// ❌ SBAGLIATO - contiene business logic
pub async fn insert(pool: &SqlitePool, ...) -> Result<Uuid> {
    if content.len() > 1000 {  // Validazione business
        return Err(...);
    }
    // ...
}
```

### 2. Service orchestrazione
```rust
// ✅ CORRETTO - orchestrazione + validazione
pub async fn delete_message(...) -> Result<Uuid> {
    let (author_id, conv_id) = MessageRepo::get_metadata(...).await?;
    
    if author_id != requester_id {  // Business rule
        return Err(AppError::Forbidden);
    }
    
    MessageRepo::delete(...).await?;
    Ok(conv_id)
}
```

### 3. Autorizzazione nel chiamante
```rust
// HTTP Controller
let is_participant = ConversationRepo::is_participant(...).await?;
if !is_participant {
    return Err(AppError::Forbidden);
}
let messages = MessageService::list(...).await?;

// WebSocket Handler
let is_participant = ConversationRepo::is_participant(...).await?;
if !is_participant {
    send_error("Not authorized").await;
    return;
}
let msg_id = MessageRepo::insert(...).await?;
```

### 4. Broadcasting separato
```rust
// Service ritorna dati necessari
let conversation_id = MessageService::delete_message(...).await?;

// Chiamante gestisce broadcasting
let participants = ConversationService::list_participant_ids(...).await?;
ConversationService::broadcast_message_deleted(...).await;
```

