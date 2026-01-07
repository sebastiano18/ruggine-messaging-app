# HTTP ARCHITECTURE

## Overview
Questa documentazione descrive l'architettura HTTP REST del sistema. L'architettura HTTP è **separata e parallela** all'architettura WebSocket, ma condivide i layer Service e Repository.

---

## LAYER 1: ROUTES

Le route definiscono gli endpoint HTTP e mappano alle funzioni dei controller.

### user_route.rs
```rust
POST /api/users/register  → user_controller::register
POST /api/users/login     → user_controller::login
POST /api/users/logout    → user_controller::logout
```

### message_route.rs
```rust
GET /api/conversations/:cid/messages  → message_controller::list
```

### conversation_route.rs
```rust
GET /api/conversations           → conversation_controller::get_conversations
GET /api/conversations/:id       → conversation_controller::get_conversation
```

---

## LAYER 2: CONTROLLERS

I controller gestiscono:
- Parsing parametri HTTP (Path, Query, Json)
- Autenticazione via `AuthUser` extractor
- Validazione input
- Autorizzazione (controllo permessi)
- Orchestrazione chiamate ai service
- Formattazione risposta JSON

**IMPORTANTE**: I controller NON contengono logica business. Delegano tutto ai service.

---

### UserController

#### `register`
```rust
POST /api/users/register
Body: { "username": "...", "password": "..." }

pub async fn register(
    State(st): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> Result<Json<CreatedId>>
```

**Flusso**:
1. Riceve username e password
2. Delega a `UserService::register`
3. Ritorna `{ "id": "uuid" }`

**Response**:
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

#### `login`
```rust
POST /api/users/login
Body: { "username": "...", "password": "..." }

pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>>
```

**Flusso**:
1. Delega autenticazione a `UserService::login`
2. Recupera `last_sequence` dell'utente da `AppState::get_current_user_sequence`
3. Ritorna token JWT + user info + sequence corrente

**Response**:
```json
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "username": "alice",
  "last_sequence": 42
}
```

**Note**: `last_sequence` rappresenta la **sequenza globale corrente dell'utente** nel sistema di eventi. Serve per la sincronizzazione: quando il client si riconnette dopo una disconnessione, può richiedere tutti gli eventi con sequence > last_sequence per recuperare gli aggiornamenti persi. Non indica "quali eventi ha già ricevuto", ma piuttosto "qual è il punto di sincronizzazione corrente da cui partire".

**Valori possibili**:
- `0`: Utente nuovo o nessun evento ricevuto ancora
- `> 0`: Sequenza dell'ultimo evento elaborato
- `0` (fallback): In caso di errore nel recuperare la sequenza dal sistema (comportamento sicuro)

---

#### `logout`
```rust
POST /api/users/logout
Body: { "token": "..." }

pub async fn logout(Json(_req): Json<LogoutReq>) -> Result<()>
```

**Flusso**: Stateless, nessuna operazione server-side. Il client deve eliminare il token.

---

### MessageController

#### `list`
```rust
GET /api/conversations/:cid/messages?limit=50&before_sequence=100

pub async fn list(
    user: AuthUser,                          // Autenticazione automatica
    Path(conversation_id): Path<Uuid>,       // :cid dalla URL
    Query(params): Query<ListQuery>,         // Query params
    State(st): State<AppState>,
) -> Result<Json<Vec<Message>>>
```

**Flusso**:
1. Verifica che l'utente sia partecipante con `ConversationRepo::is_participant`
2. Se `before_sequence` presente → paginazione con `MessageService::list_with_pagination`
3. Altrimenti → lista standard con `MessageService::list`
4. Ritorna messaggi in **ordine diverso** a seconda della modalità:
   - **Senza paginazione**: ordine **decrescente** (dal più recente al più vecchio)
   - **Con paginazione**: ordine **crescente** (dal più vecchio al più recente)

**Query Parameters**:
- `limit` (optional, default 50): Numero massimo di messaggi
  - **Senza paginazione**: Range 1-200 (clamped automaticamente in `MessageService::list`)
  - **Con paginazione**: Range 1-100 (clamped automaticamente in `MessageService::list_with_pagination`)
  - Default: 50 messaggi
- `before_sequence` (optional): Sequence number per paginazione

**Comportamento ordinamento**:
- **SENZA `before_sequence`**: Ritorna gli ultimi N messaggi in ordine **DECRESCENTE**
  - Query DB: `ORDER BY ... ASC`
  - Post-processing: `.rev()` nel controller
  - Risultato finale: Dal più recente al più vecchio
  - Utile per caricare i messaggi più recenti quando si apre una chat
  - Esempio: `limit=50` → messaggi dal 50 al 1 (recente → vecchio)
  
- **CON `before_sequence`**: Ritorna i messaggi precedenti in ordine **CRESCENTE**
  - Query DB: `ORDER BY ... DESC`
  - Post-processing: `.rev()` nel service
  - Risultato finale: Dal più vecchio al più recente
  - Utile per "scroll infinito" caricando messaggi più vecchi
  - Esempio: `before_sequence=100&limit=50` → messaggi dal 50 al 99 (vecchio → recente)

**Response** (senza paginazione, `limit=50`):
```json
[
  {
    "id": "msg-uuid-50",
    "author_id": "user-uuid",
    "conversation_id": "conv-uuid",
    "author_username": "bob",
    "content": "Ultimo messaggio (più recente)",
    "created_at": 1703001284,
    "sequence_num": 50
  },
  {
    "id": "msg-uuid-49",
    "author_id": "user-uuid",
    "conversation_id": "conv-uuid",
    "author_username": "alice",
    "content": "Penultimo messaggio",
    "created_at": 1703001283,
    "sequence_num": 49
  },
  ...
  {
    "id": "msg-uuid-1",
    "author_id": "user-uuid",
    "conversation_id": "conv-uuid",
    "author_username": "alice",
    "content": "Primo messaggio (più vecchio nel batch)",
    "created_at": 1703001234,
    "sequence_num": 1
  }
]
```
Ritorna fino a `limit` messaggi in **ordine decrescente** (dal più recente al più vecchio).
Se `limit=50`, ritorna gli ultimi 50 messaggi con sequence_num da 50 a 1.
**Nota**: Il limite massimo è 200, quindi se `limit=300` viene automaticamente ridotto a 200.

**Response** (con paginazione `before_sequence=100`, `limit=50`):
```json
[
  {
    "id": "msg-uuid-50",
    "sequence_num": 50,
    "content": "Messaggio più vecchio nel batch",
    "created_at": 1703001280,
    ...
  },
  {
    "id": "msg-uuid-51",
    "sequence_num": 51,
    "content": "Messaggio intermedio",
    "created_at": 1703001281,
    ...
  },
  ...
  {
    "id": "msg-uuid-99",
    "sequence_num": 99,
    "content": "Messaggio più recente nel batch",
    "created_at": 1703001329,
    ...
  }
]
```
Ritorna i **50 messaggi più recenti** con sequence < 100, in **ordine crescente** (dal più vecchio al più recente).
Se ci sono almeno 50 messaggi disponibili con sequence < 100, ritorna sequence_num da 50 a 99.
Se ce ne sono meno, ritorna tutti quelli disponibili sotto il valore di `before_sequence`.
**Nota**: Il limite massimo è 100, quindi se `limit=200` viene automaticamente ridotto a 100.

**Autorizzazione**:
- Solo i partecipanti alla conversazione possono leggere i messaggi
- `403 Forbidden` se l'utente non è partecipante

---

### ConversationController

#### `get_conversations`
```rust
GET /api/conversations?limit=20&before=1703001234

pub async fn get_conversations(
    user: AuthUser,
    State(st): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedConversationsResponse>>
```

**Flusso**:
1. Delega a `ConversationService::get_conversations`
2. Ritorna lista conversazioni con paginazione cursor-based

**Query Parameters**:
- `limit` (optional, default 20): Numero conversazioni
- `before` (optional): Timestamp (cursor) per paginazione

**Response**:
```json
{
  "conversations": [
    {
      "conversation": {
        "id": "conv-uuid",
        "kind": "dm",
        "title": "Chat con Bob",
        "owner_id": "550e8400-e29b-41d4-a716-446655440000",
        "created_at": 1703001234,
        "last_read_sequence": 10,
        "last_activity": 1703005000,
        "last_msg_seq": 15
      },
      "last_message": {
        "id": "msg-uuid",
        "author_id": "user-uuid",
        "conversation_id": "conv-uuid",
        "author_username": "bob",
        "content": "Ci vediamo domani",
        "created_at": 1703005000,
        "sequence_num": 15
      },
      "members": [
        {
          "user_id": "user-uuid-1",
          "username": "alice",
          "role": "member"
        },
        {
          "user_id": "user-uuid-2",
          "username": "bob",
          "role": "member"
        }
      ]
    }
  ],
  "next_cursor": 1703004000,
  "has_more": true
}
```

**Paginazione**:
- `next_cursor`: Usare come parametro `before` nella prossima richiesta
- `has_more`: true se ci sono altre conversazioni

---

#### `get_conversation`
```rust
GET /api/conversations/:id

pub async fn get_conversation(
    user: AuthUser,
    State(st): State<AppState>,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<ConversationSummary>>
```

**Flusso**:
1. Delega a `ConversationService::get_conversation`
2. Ritorna dettagli conversazione singola

**Response**:
```json
{
  "conversation": {
    "id": "conv-uuid",
    "kind": "group",
    "title": "Team Project",
    "owner_id": "owner-uuid",
    "created_at": 1703001234,
    "last_read_sequence": 5,
    "last_activity": 1703005000,
    "last_msg_seq": 8
  },
  "last_message": {
    "id": "msg-uuid",
    ...
  },
  "members": [
    { "user_id": "...", "username": "alice", "role": "owner" },
    { "user_id": "...", "username": "bob", "role": "member" }
  ]
}
```

**Note sui campi**:
- `owner_id`: Sempre presente (UUID valido). Per DM contiene un UUID, per gruppi l'UUID del proprietario
- `kind`: "dm" o "group"
- `last_read_sequence`: Ultima sequence letta dall'utente richiedente
- `last_msg_seq`: Sequence dell'ultimo messaggio nella conversazione (0 se nessun messaggio)

**Autorizzazione**:
- Solo i partecipanti alla conversazione possono accedere ai suoi dettagli
- `404 Not Found` se la conversazione non esiste o l'utente non è partecipante
- L'autorizzazione è implementata **implicitamente** nel repository tramite `JOIN participants` con filtro `WHERE p.user_id = ?`, quindi se l'utente non è partecipante la query non ritorna risultati

---

## AUTENTICAZIONE

Tutti gli endpoint (tranne register/login) richiedono autenticazione via JWT token.

### AuthUser Extractor
```rust
pub struct AuthUser {
    pub id: Uuid,
    pub username: String,
}
```

**Header richiesto**:
```
Authorization: Bearer <jwt_token>
```

**Errori**:
- `401 Unauthorized`: Token mancante, invalido, scaduto, o utente non più esistente nel database

---

## ERROR HANDLING

### Codici HTTP
- `200 OK`: Operazione riuscita
- `400 Bad Request`: Input invalido (es. password troppo corta)
- `401 Unauthorized`: Autenticazione fallita
- `403 Forbidden`: Utente non autorizzato per questa risorsa
- `404 Not Found`: Risorsa non trovata
- `409 Conflict`: Username già esistente (register)
- `500 Internal Server Error`: Errore server

### Response di errore
```json
{
  "error": "Descrizione dell'errore"
}
```

---

## OPERAZIONI NON ESPOSTE VIA HTTP

I seguenti metodi esistono nei service ma **NON** hanno endpoint HTTP corrispondenti. Queste operazioni sono disponibili **solo via WebSocket** o attraverso effetti collaterali (cascade delete):

### Creazione e invio messaggi
- `MessageRepo::insert` - Creazione di nuovi messaggi (solo WebSocket `send_message`)
- **Motivo**: I messaggi richiedono notifiche real-time e conferme con `temp_id`

### Eliminazione messaggi
- `MessageService::delete_message` - Eliminazione messaggi (solo WebSocket `delete_message`)
- **Motivo**: Richiede broadcasting in tempo reale agli altri partecipanti

### Gestione gruppi
- `ConversationService::create_group` - Creazione gruppi (solo WebSocket `create_group`)
- `ConversationService::add_member` - Aggiunta membri (solo WebSocket `add_member`)
- `ConversationService::delete_conversation` - Eliminazione conversazioni (solo WebSocket `delete_conversation` o cascade su delete utente)
- `ConversationService::leave_group` - Uscita da gruppo (solo WebSocket `leave_group`)
- **Motivo**: Richiedono notifiche in tempo reale a tutti i membri del gruppo

### Gestione letture
- `ParticipantService::mark_read` - Segnare messaggi come letti (solo WebSocket `mark_read`)
- **Motivo**: Operazione frequente che beneficia della connessione persistente

### Gestione utenti
- `UserService::delete_user` - Eliminazione account (solo WebSocket `delete_account`)
- **Motivo**: Richiede notifiche complesse a tutti i partecipanti delle conversazioni dell'utente

**Rationale generale**: HTTP è progettato per operazioni di **lettura** e **autenticazione** iniziale. Tutte le operazioni di **scrittura che richiedono notifiche real-time** sono delegate a WebSocket per garantire la sincronizzazione immediata tra tutti i client.

---

## DIFFERENZE CON WEBSOCKET

| Aspetto | HTTP | WebSocket |
|---------|------|-----------|
| **Paradigma** | Request/Response | Event-driven |
| **Stato** | Stateless | Stateful (connessione persistente) |
| **Autenticazione** | JWT per ogni richiesta | JWT solo al connect |
| **Validazione** | Nei controller | Nel WebSocket handler |
| **Creazione messaggi** | ❌ Non supportato | ✅ `send_message` |
| **Eliminazione messaggi** | ❌ Non supportato | ✅ `delete_message` |
| **Creazione gruppi** | ❌ Non supportato | ✅ `create_group` |
| **Gestione membri** | ❌ Non supportato | ✅ `add_member`, `leave_group` |
| **Real-time updates** | ❌ Polling necessario | ✅ Push automatico |
| **Mark as read** | ❌ Non supportato | ✅ `mark_read` |

**HTTP è usato per**:
- Login/Register
- Caricamento iniziale messaggi (lettura)
- Listing conversazioni (lettura)

**WebSocket è usato per**:
- Invio/ricezione messaggi in tempo reale
- Creazione/eliminazione conversazioni
- Gestione membri gruppi
- Notifiche eventi (user typing, message deleted, etc.)
- Conferme con `temp_id`
- Mark as read

---

## FLUSSO TIPICO CLIENT

### 1. Registrazione e Login
```
POST /api/users/register → 201 Created
POST /api/users/login    → 200 OK + { token, last_sequence }
```

### 2. Caricamento iniziale
```
GET /api/conversations   → Lista conversazioni
GET /api/conversations/:id/messages → Ultimi 50 messaggi
```

### 3. Connessione WebSocket
```
ws://server/ws + Authorization header
→ Ricevi eventi in real-time
```

### 4. Invio messaggio
```
WebSocket: send_message
→ Server risponde con message_confirmed
→ Broadcast agli altri partecipanti
```

### 5. Paginazione messaggi vecchi
```
GET /api/conversations/:id/messages?before_sequence=50
→ Carica fino a 100 messaggi precedenti
```

---

## NOTE IMPLEMENTATIVE

### Limiti diversi per paginazione
Il sistema usa limiti massimi diversi per ottimizzare le performance:
- **Lista normale** (caricamento iniziale): max 200 messaggi - consente di caricare più storia iniziale
- **Paginazione** (scroll infinito): max 100 messaggi - richieste più frequenti, quindi limite più conservativo

### Ordinamento con `.rev()`
L'ordinamento finale viene ottenuto attraverso una combinazione di query SQL e post-processing in Rust:
- **Lista normale**: Query `ASC` + `.rev()` nel controller → output `DESC`
- **Paginazione**: Query `DESC` + `.rev()` nel service → output `ASC`

Questa strategia permette di ottimizzare le query SQL mantenendo l'ordinamento desiderato per l'utente.
