# API Layer - Rust Ruggine Chat Client

## Panoramica

L'API layer del client Rust Ruggine gestisce tutta la comunicazione con il server attraverso due canali principali:
1. **HTTP REST API** - Per operazioni sincrone (login, fetch dati, logout)
2. **WebSocket** - Per comunicazione real-time bidirezionale (messaggi, eventi, sincronizzazione)

## Architettura API

### Moduli

```
api/
├── mod.rs          # Re-exports
├── auth.rs         # Autenticazione (login, register, logout)
├── conversation.rs # Gestione conversazioni (fetch, pagination)
├── chat.rs         # Messaggi (fetch paginated)
└── ws.rs           # WebSocket (connect, subscribe, bidirectional handler)
```

### Pattern Generale

**HTTP Requests**:
```rust
pub async fn operation(base: &str, token: &str, params...) -> Result<ResponseType> {
    Client::new()
        .method(format!("{base}/api/endpoint"))
        .bearer_auth(token)  // Se richiede auth
        .json(&request_body)  // Se POST/PUT
        .send()
        .await?
        .error_for_status()?
        .json::<ResponseType>()
        .await?
}
```

**Error Handling**: Tutte le funzioni ritornano `Result<T>` usando `anyhow`

## HTTP REST API

### 1. Authentication (auth.rs)

#### register()

**Signature**:
```rust
pub async fn register(base: &str, u: &str, p: &str) -> Result<()>
```

**Request**:
```json
POST /api/users/register
{
    "username": "alice",
    "password": "password123"
}
```

**Response**: 
- Success: `200 OK` (no body)
- Error: `4xx` con messaggio errore

**Implementation**:
```rust
pub async fn register(base: &str, u: &str, p: &str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/users/register"))
        .json(&RegisterReq { username: u, password: p })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
```

**Uso**:
```rust
auth::register(&base, &username, &password).await?;
```

#### login()

**Signature**:
```rust
pub async fn login(base: &str, u: &str, p: &str) -> Result<LoginResp>
```

**Request**:
```json
POST /api/users/login
{
    "username": "alice",
    "password": "password123"
}
```

**Response**:
```rust
pub struct LoginResp {
    pub token: String,           // JWT token
    pub user_id: Uuid,           // UUID (not String!)
    pub username: String,        // Username dell'utente
    pub last_sequence: u64,      // Ultima sequenza utente confermata dal server
}
```

**Implementation**:
```rust
pub async fn login(base: &str, u: &str, p: &str) -> Result<LoginResp> {
    let r = Client::new()
        .post(format!("{base}/api/users/login"))
        .json(&LoginReq { username: u, password: p })
        .send()
        .await?
        .error_for_status()?
        .json::<LoginResp>()
        .await?;
    Ok(r)
}
```

**Uso**:
```rust
let resp = auth::login(&base, &username, &password).await?;
let token = resp.token;
let user_id = resp.user_id;  // Già Uuid, non serve parse
let username = resp.username;
let last_sequence = resp.last_sequence;  // Inizializza user_sequence_confirmed
```

**Note**: 
- `last_sequence` viene usato per inizializzare `user_sequence_confirmed`
- Il server restituisce l'ultima sequenza confermata per permettere recovery dopo riconnessione
- `user_id` è già di tipo `Uuid`, non serve parsing

#### logout()

**Signature**:
```rust
pub async fn logout(base: &str, token: &str) -> Result<()>
```

**Request**:
```json
POST /api/users/logout
{
    "token": "eyJhbGc..."
}
```

**Response**: 
- Success: `200 OK` (no body)
- Error: `4xx` con messaggio errore

**Implementation**:
```rust
pub async fn logout(base: &str, token: &str) -> Result<()> {
    Client::new()
        .post(format!("{base}/api/users/logout"))
        .json(&LogoutReq { token })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
```

**Uso**:
```rust
auth::logout(&base, &token).await?;
```

**Note**: Logout invalida il token server-side

### 2. Conversations (conversation.rs)

#### get_conversations_paginated()

**Signature**:
```rust
pub async fn get_conversations_paginated(
    base: &str,
    token: &str,
    before: Option<i64>,  // Timestamp cursor
    limit: i32,           // Default: 20
) -> Result<PaginatedConversationsResponse>
```

**Request**:
```
GET /api/conversations?limit=20
GET /api/conversations?limit=20&before=1734567890
```

**Response**:
```rust
pub struct PaginatedConversationsResponse {
    pub conversations: Vec<ConversationSummary>,
    pub next_cursor: Option<i64>,  // Timestamp for next page
    pub has_more: bool,
}

pub struct ConversationSummary {
    pub conversation: ConversationDto,
    pub last_message: Option<MessageDto>,
    pub members: Option<Vec<ParticipantInfo>>,
}
```

**Implementation**:
```rust
pub async fn get_conversations_paginated(
    base: &str,
    token: &str,
    before: Option<i64>,
    limit: i32,
) -> Result<PaginatedConversationsResponse> {
    let mut url = format!("{base}/api/conversations?limit={limit}");

    if let Some(cursor) = before {
        url.push_str(&format!("&before={cursor}"));
    }

    let r = Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<PaginatedConversationsResponse>()
        .await?;

    Ok(r)
}
```

**Pagination Pattern**:
```rust
// Prima pagina
let first_page = get_conversations_paginated(base, token, None, 20).await?;

// Pagina successiva
if first_page.has_more {
    let next = get_conversations_paginated(
        base, 
        token, 
        first_page.next_cursor,  // Use cursor from previous response
        20
    ).await?;
}
```

**Cursor-based Pagination**:
- `before`: timestamp `last_activity` dell'ultima conversazione caricata
- Server ritorna conversazioni con `last_activity < before`
- Ordered per `last_activity DESC`

#### get_conversation()

**Signature**:
```rust
pub async fn get_conversation(
    base: &str,
    token: &str,
    conversation_id: Uuid
) -> Result<ConversationSummary>
```

**Request**:
```
GET /api/conversations/{conversation_id}
```

**Response**: `ConversationSummary` (singola conversazione)

**Implementation**:
```rust
pub async fn get_conversation(
    base: &str,
    token: &str,
    conversation_id: Uuid
) -> Result<ConversationSummary> {
    let r = Client::new()
        .get(format!("{base}/api/conversations/{conversation_id}"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<ConversationSummary>()
        .await?;
    Ok(r)
}
```

**Uso**:
```rust
let summary = conversation::get_conversation(&base, &token, cid).await?;
```

**Use Case**: Fetch on-demand quando arriva messaggio per conversazione non caricata

### 3. Messages (chat.rs)

#### get_messages_paginated()

**Signature**:
```rust
pub async fn get_messages_paginated(
    base: &str,
    token: &str,
    cid: Uuid,
    limit: Option<i64>,
    before_sequence: Option<i64>,
) -> Result<Vec<MessageDto>>
```

**Request**:
```
GET /api/conversations/{cid}/messages
GET /api/conversations/{cid}/messages?limit=30
GET /api/conversations/{cid}/messages?limit=30&before_sequence=100
```

**Parameters**:
- `limit`: Numero messaggi da caricare (default server-side)
- `before_sequence`: Carica messaggi con `sequence_num < before_sequence`

**Response**: 
```rust
Vec<MessageDto>  // Messaggi ordered by sequence_num ASC
```

**Implementation**:
```rust
pub async fn get_messages_paginated(
    base: &str,
    token: &str,
    cid: Uuid,
    limit: Option<i64>,
    before_sequence: Option<i64>,
) -> Result<Vec<MessageDto>> {
    let mut url = format!("{base}/api/conversations/{cid}/messages");

    let mut params = Vec::new();
    if let Some(limit) = limit {
        params.push(format!("limit={}", limit));
    }
    if let Some(before_seq) = before_sequence {
        params.push(format!("before_sequence={}", before_seq));
    }

    if !params.is_empty() {
        url.push_str("?");
        url.push_str(&params.join("&"));
    }

    let response_messages = Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<MessageResponse>>()
        .await?;

    let messages: Vec<MessageDto> = response_messages
        .into_iter()
        .filter_map(|msg| {
            let id = Uuid::parse_str(&msg.id).ok()?;
            let author_id = Uuid::parse_str(&msg.author_id).ok()?;

            Some(MessageDto {
                id,
                author_id,
                conversation_id: cid,
                author_username: msg.author_username,
                content: msg.content,
                created_at: msg.created_at,
                sequence_num: msg.sequence_num.map(|s| s as u64),
                client_msg_id: None,
                is_confirmed: Some(true),
            })
        })
        .collect();

    Ok(messages)
}
```

**Pagination Pattern**:
```rust
// Load oldest messages first
let oldest_seq = messages.first().map(|m| m.sequence_num).flatten();
let older_messages = chat::get_messages_paginated(
    &base, &token, cid, Some(30), oldest_seq.map(|s| s as i64)
).await?;
```

**Note**: 
- Server ritorna messaggi in ordine `ASC` (oldest first)
- Client deve inserire i nuovi messaggi all'inizio della lista locale
- `before_sequence` è esclusivo (messaggi con seq < before_sequence)

## WebSocket API

### Overview

Il modulo `ws.rs` gestisce la connessione WebSocket bidirezionale real-time con architettura a **due task separati**:

1. **Task Indipendente**: Invia ping applicativi ogni 30s (JSON)
2. **Task Principale**: Gestisce I/O bidirezionale + keepalive nativo ogni 60s

### Types

```rust
pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct WsControl {
    pub shutdown: oneshot::Sender<()>,
    pub outgoing_tx: mpsc::UnboundedSender<String>,
    pub user_sequence: Arc<AtomicU64>,  // ⚠️ Shared reference to user sequence
}
```

**Note**: `user_sequence` è condiviso tra UI e WebSocket handler per:
- Inviare la sequenza corrente nei ping automatici
- Permettere alla UI di leggere/aggiornare la sequenza in modo thread-safe

### 1. connect()

**Signature**:
```rust
pub async fn connect(
    base: &str, 
    token: &str, 
    session_id: Option<Uuid>
) -> Result<WsStream>
```

**URL Construction**:
```rust
// Without session_id
"ws://localhost:8080/ws"

// With session_id
"ws://localhost:8080/ws?session_id=550e8400-e29b-41d4-a716-446655440000"
```

**Implementation**:
```rust
pub async fn connect(base: &str, token: &str, session_id: Option<uuid::Uuid>) -> Result<WsStream> {
    let ws_url = if let Some(sid) = session_id {
        format!("{}/ws?session_id={}", base.trim_end_matches('/'), sid)
            .replacen("http", "ws", 1)
    } else {
        format!("{}/ws", base.trim_end_matches('/')).replacen("http", "ws", 1)
    };

    debug!("Connecting to WebSocket: {}", ws_url);

    if token.trim().is_empty() {
        return Err(anyhow::anyhow!("Token di autenticazione vuoto"));
    }

    let mut req: Request<()> = ws_url.as_str().into_client_request()?;
    req.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token.trim()))?,
    );

    let (ws, resp) = connect_async(req).await?;
    debug!("WebSocket connection established with status: {}", resp.status());
    Ok(ws)
}
```

**Usage**:
```rust
let session_id = Some(Uuid::new_v4());
let ws = ws::connect(&base, &token, session_id).await?;
```

**Error Cases**:
- Empty token → `anyhow::Error`
- Connection refused → Network error
- Invalid URL → Parsing error
- Server rejection → HTTP status error

### 2. subscribe()

**Signature**:
```rust
pub async fn subscribe(ws: &mut WsStream) -> Result<()>
```

**Message**:
```json
{"type":"subscribe"}
```

**Implementation**:
```rust
pub async fn subscribe(ws: &mut WsStream) -> Result<()> {
    debug!("Sending subscribe message");

    let subscribe_msg = Message::Text(r#"{"type":"subscribe"}"#.into());

    tokio::time::timeout(std::time::Duration::from_secs(10), ws.send(subscribe_msg)).await??;

    Ok(())
}
```

**Usage**:
```rust
ws::subscribe(&mut ws).await?;
```

**Note**: Timeout di 10 secondi per evitare hang indefinito

### 3. spawn_bidirectional_handler()

**Signature**:
```rust
pub fn spawn_bidirectional_handler(
    ws: WsStream,
    on_text: impl FnMut(String) + Send + 'static,
    disconnect_notifier: Option<mpsc::UnboundedSender<UiEvent>>,
    user_sequence: Arc<AtomicU64>,  // ⚠️ PARAMETRO CRITICO
) -> WsControl
```

**Parameters**:
- `ws`: WebSocket stream connesso
- `on_text`: Callback invocata per ogni messaggio di testo ricevuto
- `disconnect_notifier`: Canale per notificare disconnessioni non volontarie
- `user_sequence`: Shared atomic counter per la sequenza utente corrente

**Return Value**: 
```rust
WsControl {
    shutdown: oneshot::Sender<()>,           // Trigger graceful shutdown
    outgoing_tx: mpsc::UnboundedSender<String>,  // Send messages to server
    user_sequence: Arc<AtomicU64>,           // Shared sequence counter
}
```

**Architecture**: Spawna **DUE task tokio separati**

#### Task 1: Independent Application Ping (30s)

```rust
// Clone per il ping task
let ping_tx = outgoing_tx.clone();
let ping_user_seq = user_sequence.clone();

tokio::spawn(async move {
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Skip first immediate tick
    ping_interval.tick().await;

    loop {
        ping_interval.tick().await;

        let user_seq = ping_user_seq.load(Ordering::SeqCst);

        let ping_json = serde_json::json!({
            "type": "ping",
            "timestamp": chrono::Utc::now().timestamp(),
            "user_sequence": user_seq,
        });

        match serde_json::to_string(&ping_json) {
            Ok(msg) => {
                // Se il send fallisce, il WebSocket è chiuso → termina task
                if ping_tx.send(msg).is_err() {
                    debug!("Ping task: WebSocket closed, stopping ping task");
                    break;
                }
                debug!("📡 Automatic ping sent from independent tokio task (user_seq: {})", user_seq);
            }
            Err(e) => {
                error!("Failed to serialize ping JSON: {}", e);
            }
        }
    }

    debug!("Independent ping task terminated");
});
```

**Caratteristiche**:
- ✅ **Completamente indipendente** dal loop principale
- ✅ **Non dipende da egui** o dalla UI
- ✅ Invia ping **JSON applicativi** ogni 30 secondi
- ✅ Include `user_sequence` corrente nel payload
- ✅ Auto-termina quando `outgoing_tx` è chiuso
- ✅ Fire-and-forget (no timeout, no retry)

#### Task 2: Main Bidirectional Loop

```rust
tokio::spawn(async move {
    let mut ping_interval = tokio::time::interval(Duration::from_secs(60));
    let mut last_pong = std::time::Instant::now();
    let mut consecutive_failures = 0u32;
    const MAX_FAILURES: u32 = 5;
    const PING_TIMEOUT: Duration = Duration::from_secs(120);
    let mut graceful_shutdown = false;

    loop {
        tokio::select! {
            // ===== Branch 1: Shutdown Signal =====
            _ = &mut shutdown_rx => {
                debug!("WebSocket shutdown requested");
                graceful_shutdown = true;
                
                let close_frame = CloseFrame {
                    code: CloseCode::Normal,
                    reason: Cow::from("app_exit"),
                };

                if let Err(e) = ws.send(Message::Close(Some(close_frame))).await {
                    debug!("Failed to send close frame: {}", e);
                }

                // Wait for server acknowledgment (max 1s)
                let _ = tokio::time::timeout(
                    Duration::from_millis(1000),
                    ws.next()
                ).await;

                debug!("WebSocket connection closed gracefully");
                break;
            }

            // ===== Branch 2: Outgoing Messages =====
            Some(msg) = outgoing_rx.recv() => {
                debug!("Sending message to server: {}",
                       msg.chars().take(100).collect::<String>());

                if msg.len() > 100_000 {
                    error!("Message too large ({} bytes), dropping", msg.len());
                    continue;
                }

                match ws.send(Message::Text(msg)).await {
                    Ok(_) => {
                        consecutive_failures = 0;
                    }
                    Err(e) => {
                        error!("Failed to send message to server: {}", e);
                        consecutive_failures += 1;
                        if consecutive_failures >= MAX_FAILURES {
                            error!("Too many consecutive send failures ({}), closing connection",
                                   consecutive_failures);
                            break;
                        }
                    }
                }
            }

            // ===== Branch 3: Incoming Messages =====
            Some(msg_result) = ws.next() => {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        debug!("Received text message: {}",
                               text.chars().take(100).collect::<String>());

                        if text.len() > 1_000_000 {
                            error!("Received message too large ({} bytes), ignoring", text.len());
                            continue;
                        }

                        consecutive_failures = 0;
                        on_text(text);
                    }
                    
                    Ok(Message::Pong(payload)) => {
                        debug!("Received pong from server (payload: {} bytes)", payload.len());
                        last_pong = std::time::Instant::now();  // ⚠️ UPDATE TIMESTAMP
                        consecutive_failures = 0;
                    }
                    
                    Ok(Message::Ping(payload)) => {
                        debug!("Received ping from server, sending pong");
                        if let Err(e) = ws.send(Message::Pong(payload)).await {
                            warn!("Failed to send pong response: {}", e);
                            consecutive_failures += 1;
                        } else {
                            consecutive_failures = 0;
                        }
                    }
                    
                    Ok(Message::Close(frame)) => {
                        debug!("Received close frame: {:?}", frame);
                        let _ = ws.send(Message::Close(None)).await;
                        break;
                    }
                    
                    Ok(Message::Binary(data)) => {
                        debug!("Received binary message ({} bytes), ignoring", data.len());
                    }
                    
                    Ok(Message::Frame(_)) => {
                        debug!("Received raw frame, ignoring");
                    }
                    
                    Err(e) => {
                        error!("WebSocket receive error: {}", e);
                        consecutive_failures += 1;

                        let error_str = e.to_string().to_lowercase();
                        debug!("Error string (lowercase) for matching: '{}'", error_str);

                        // ⚠️ MULTI-PLATFORM ERROR DETECTION
                        if error_str.contains("connection closed") ||
                           error_str.contains("broken pipe") ||
                           error_str.contains("interrotta") ||  // Italian: "Connessione in corso interrotta"
                           error_str.contains("10054") {        // Windows error code
                            error!("Connection terminated by peer - error matched: {}", error_str);
                            break;
                        }

                        if consecutive_failures >= MAX_FAILURES {
                            error!("Too many consecutive receive failures ({}), closing connection",
                                   consecutive_failures);
                            break;
                        }

                        warn!("WebSocket receive error (failure {}/{}), retrying after delay",
                              consecutive_failures, MAX_FAILURES);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }

            // ===== Branch 4: Keepalive Ping (60s) =====
            _ = ping_interval.tick() => {
                let elapsed_since_pong = last_pong.elapsed();

                if elapsed_since_pong > PING_TIMEOUT {
                    error!("No pong received for {:?}, connection appears dead", elapsed_since_pong);
                    break;
                }

                debug!("Sending WebSocket keepalive ping (last pong: {:?} ago)", elapsed_since_pong);

                // ⚠️ SOLO ping WebSocket nativo per keepalive
                match ws.send(Message::Ping(vec![])).await {
                    Ok(_) => debug!("WebSocket keepalive ping sent"),
                    Err(e) => {
                        error!("Failed to send WebSocket ping: {}", e);
                        consecutive_failures += 1;
                        if consecutive_failures >= MAX_FAILURES {
                            error!("Too many ping failures ({}), closing connection", consecutive_failures);
                            break;
                        }
                    }
                }
            }
        }
    }

    // Notifica disconnessione se non è shutdown graceful
    if !graceful_shutdown {
        if let Some(notifier) = disconnect_notifier {
            let _ = notifier.send(UiEvent::WsDisconnected);
        }
    }

    debug!("WebSocket handler loop ended (failures: {})", consecutive_failures);
});
```

**Caratteristiche Chiave**:

1. **Graceful Shutdown**:
   - Close frame con `CloseCode::Normal` e reason `"app_exit"`
   - Wait per acknowledgment dal server (max 1s)
   - No notification se shutdown volontario

2. **Consecutive Failures Tracking**:
   - `MAX_FAILURES = 5` - Massimo 5 errori consecutivi
   - Reset a 0 su ogni operazione riuscita
   - Chiusura automatica se superato

3. **Connection Termination Detection**:
   - Pattern matching su error string (case-insensitive)
   - Riconosce: "connection closed", "broken pipe", "interrotta", "10054"
   - Multi-platform e multi-lingua

4. **Keepalive Ping (60s)**:
   - WebSocket ping nativo (Message::Ping)
   - Separato dall'application ping
   - Timeout detection: 120s senza pong

5. **Message Size Limits**:
   - Outgoing: max 100KB (100,000 bytes)
   - Incoming: max 1MB (1,000,000 bytes)
   - Drop se supera limiti

6. **Pong Timeout Detection**:
   - Track `last_pong` timestamp
   - Check ogni keepalive tick
   - Break se > 120s senza pong

**Return Value Usage**:
```rust
let ctrl = spawn_bidirectional_handler(ws, on_text, Some(notifier), user_seq);

// Send message
ctrl.outgoing_tx.send(json_message)?;

// Update sequence (thread-safe)
ctrl.user_sequence.store(new_seq, Ordering::SeqCst);

// Read sequence
let current_seq = ctrl.user_sequence.load(Ordering::SeqCst);

// Shutdown
ctrl.shutdown.send(())?;
```

### Message Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                      WebSocket Architecture                      │
└─────────────────────────────────────────────────────────────────┘

╔════════════════════════════════════════════════════════════════╗
║  Task 1: Independent Application Ping (30s interval)           ║
╠════════════════════════════════════════════════════════════════╣
║  • Tokio async task (completely independent)                   ║
║  • Sends: {"type":"ping", "timestamp":..., "user_sequence":...}║
║  • Auto-terminates when WebSocket closes                       ║
║  • Purpose: Application-level sync & liveness check            ║
║  • Does NOT depend on UI/egui loops                            ║
╚════════════════════════════════════════════════════════════════╝
                          │
                          │ (via outgoing_tx channel)
                          ▼
╔════════════════════════════════════════════════════════════════╗
║  Task 2: Main Bidirectional Loop (tokio::select!)             ║
╠════════════════════════════════════════════════════════════════╣
║                                                                ║
║  ┌──────────────────────────────────────────────────────────┐ ║
║  │  Branch 1: Shutdown Signal                               │ ║
║  │  • Graceful close with CloseFrame                        │ ║
║  │  • Wait for server acknowledgment (max 1s)               │ ║
║  └──────────────────────────────────────────────────────────┘ ║
║                                                                ║
║  ┌──────────────────────────────────────────────────────────┐ ║
║  │  Branch 2: Outgoing Messages (from UI/app/ping-task)    │ ║
║  │  • Size validation (max 100KB)                          │ ║
║  │  • Send as WebSocket Text frame                         │ ║
║  │  • Track consecutive failures                           │ ║
║  └──────────────────────────────────────────────────────────┘ ║
║                                                                ║
║  ┌──────────────────────────────────────────────────────────┐ ║
║  │  Branch 3: Incoming Messages (from server)              │ ║
║  │  • Text: size check (max 1MB) → on_text callback        │ ║
║  │  • Ping: respond with Pong                              │ ║
║  │  • Pong: update last_pong timestamp (critical!)         │ ║
║  │  • Close: send Close response and terminate             │ ║
║  │  • Error: detect connection termination patterns        │ ║
║  └──────────────────────────────────────────────────────────┘ ║
║                                                                ║
║  ┌──────────────────────────────────────────────────────────┐ ║
║  │  Branch 4: Keepalive Ping (60s interval)                │ ║
║  │  • WebSocket native ping (Message::Ping([]))            │ ║
║  │  • Check: no pong > 120s? → terminate                   │ ║
║  │  • Track consecutive ping failures                      │ ║
║  └──────────────────────────────────────────────────────────┘ ║
║                                                                ║
╚════════════════════════════════════════════════════════════════╝
```

### Ping System Comparison

| Feature | Application Ping (30s) | Keepalive Ping (60s) |
|---------|----------------------|---------------------|
| **Tipo** | JSON message | WebSocket native ping |
| **Payload** | `{"type":"ping", "timestamp":..., "user_sequence":...}` | Empty bytes `[]` |
| **Purpose** | Application-level sync + liveness | Protocol-level keepalive |
| **Risposta** | `{"type":"pong", "user_sequence":..., "conversation_sequences":...}` | WebSocket Pong frame |
| **Task** | Independent tokio task | Main loop select branch |
| **Intervallo** | 30 secondi | 60 secondi |
| **Timeout** | N/A (fire and forget) | 120s without pong → disconnect |
| **Dependency** | None (fully async) | Main loop must be running |

### Error Handling & Resilience

**Error Categories**:

1. **Send Failures** (`MAX_FAILURES = 5`):
   ```rust
   consecutive_failures += 1;
   if consecutive_failures >= MAX_FAILURES {
       error!("Too many send failures, closing");
       break;
   }
   ```

2. **Receive Errors**:
   - Connection termination patterns (immediate break)
   - Transient errors (retry after 100ms delay)
   - Max 5 consecutive failures

3. **Connection Termination Detection** (Multi-Platform):
   ```rust
   let error_str = e.to_string().to_lowercase();
   
   if error_str.contains("connection closed") ||
      error_str.contains("broken pipe") ||
      error_str.contains("interrotta") ||      // Italian: "Connessione in corso interrotta"
      error_str.contains("10054") {            // Windows error code WSAECONNRESET
       error!("Connection terminated by peer - error matched: {}", error_str);
       break;  // Immediate termination
   }
   ```

4. **Pong Timeout**:
   ```rust
   const PING_TIMEOUT: Duration = Duration::from_secs(120);
   
   if last_pong.elapsed() > PING_TIMEOUT {
       error!("No pong received for {:?}, connection dead", last_pong.elapsed());
       break;
   }
   ```

**Retry Strategy** (gestito da `ConnectionManager` in `app/ws_manager`):
- Exponential backoff
- Max 6 tentativi
- Forced logout se tutti falliscono

### Message Validation

**Outgoing Validation**:
```rust
// Size check
if msg.len() > 100_000 {
    error!("Message too large ({} bytes), dropping", msg.len());
    continue;
}
```

**Incoming Validation**:
```rust
// Size check
if text.len() > 1_000_000 {
    error!("Received message too large ({} bytes), ignoring", text.len());
    continue;
}

// Content validation happens in handle_websocket_message() function
// in app/ws_manager/handlers.rs (not in ws.rs)
```

**Note**: La validazione del contenuto (JSON parsing, type checking) avviene nel layer superiore (`handle_websocket_message`), non nel WebSocket handler.

### Keepalive Strategy

**Purpose**: Detect dead connections and maintain NAT/firewall holes

**Multi-Layer Approach**:
1. **Application Ping** (30s) - Sync + liveness + sequence exchange
2. **WebSocket Ping** (60s) - Protocol-level keepalive
3. **Pong Timeout** (120s) - Connection death detection

**Pong Tracking**:
```rust
let mut last_pong = Instant::now();

// On Message::Pong received
last_pong = Instant::now();  // ⚠️ CRITICAL: Must update timestamp

// Periodic check (every 60s)
if last_pong.elapsed() > Duration::from_secs(120) {
    // Connection dead
    break;
}
```

**Why Two Ping Systems?**:

1. **Application Ping (30s)** - Task Indipendente:
   - Sincronizza sequenze tra client e server
   - Permette recovery da gap detection
   - Scambio bidirezionale di stato
   - Completamente indipendente dalla UI

2. **WebSocket Ping (60s)** - Main Loop:
   - Mantiene alive la connessione TCP
   - Attraversa NAT e firewall
   - Detect morte connessione via pong timeout
   - Integrato nel main loop

## API Usage Patterns

### 1. Login Flow

```rust
// 1. Login
let resp = auth::login(&base, &username, &password).await?;

// 2. Store credentials
state.token = Some(resp.token.clone());
state.user_id = Some(resp.user_id);  // Già Uuid
state.username = Some(resp.username);
state.user_sequence_confirmed = resp.last_sequence;

// 3. Generate session ID
state.current_session_id = Some(Uuid::new_v4());

// 4. Initialize shared sequence counter
let user_seq_shared = Arc::new(AtomicU64::new(resp.last_sequence));

// 5. Connect WebSocket
let mut ws = ws::connect(&base, &resp.token, state.current_session_id).await?;

// 6. Subscribe
ws::subscribe(&mut ws).await?;

// 7. Start bidirectional handler with user_sequence
let ctrl = ws::spawn_bidirectional_handler(
    ws,
    move |msg| handle_websocket_message(&tx, msg),
    Some(disconnect_tx),
    user_seq_shared.clone(),  // ⚠️ Pass shared sequence
);

state.ws_ctrl = Some(ctrl);
```

### 2. Load Conversations

```rust
// Initial load
let resp = conversation::get_conversations_paginated(
    &base, &token, None, 20
).await?;

state.conversations = Some(resp.conversations);
state.has_more_conversations = resp.has_more;
state.next_cursor = resp.next_cursor;

// Load more (pagination)
if state.has_more_conversations {
    let more = conversation::get_conversations_paginated(
        &base, &token, state.next_cursor, 20
    ).await?;
    
    state.conversations.as_mut().unwrap().extend(more.conversations);
    state.has_more_conversations = more.has_more;
    state.next_cursor = more.next_cursor;
}
```

### 3. Load Messages

```rust
// Open conversation - load initial messages
let messages = chat::get_messages_paginated(
    &base, &token, conversation_id, Some(30), None
).await?;

// Store in conversation
if let Some(conv) = state.conversations.iter_mut().find(|c| c.id == conversation_id) {
    conv.messages = messages;
}

// Load older messages (scroll up)
if let Some(first_msg) = conversation.messages.first() {
    if let Some(seq) = first_msg.sequence_num {
        let older = chat::get_messages_paginated(
            &base, &token, conversation_id, Some(30), Some(seq as i64)
        ).await?;
        
        // Prepend older messages
        conversation.messages.splice(0..0, older);
    }
}
```

### 4. Send Message

```rust
// 1. Generate client message ID
let client_msg_id = Uuid::new_v4().to_string();

// 2. Increment user sequence
let user_seq = state.user_sequence_next.fetch_add(1, Ordering::SeqCst);

// 3. Create optimistic message
let optimistic_msg = MessageDto {
    id: Uuid::new_v4(),  // Temp ID
    author_id: state.user_id.unwrap(),
    conversation_id,
    author_username: state.username.clone().unwrap(),
    content: content.clone(),
    created_at: chrono::Utc::now().timestamp(),
    sequence_num: None,  // Not confirmed yet
    client_msg_id: Some(client_msg_id.clone()),
    is_confirmed: Some(false),
};

// 4. Add to UI immediately
conversation.messages.push(optimistic_msg);

// 5. Send via WebSocket
let msg = serde_json::json!({
    "type": "chat_message",
    "conversation_id": conversation_id.to_string(),
    "content": content,
    "client_msg_id": client_msg_id,
    "user_sequence": user_seq,
});

state.ws_ctrl.as_ref()
    .unwrap()
    .outgoing_tx
    .send(msg.to_string())?;
```

### 5. Handle Confirmation

```rust
// When server confirms message
fn handle_message_confirmed(
    conversation_id: Uuid,
    server_msg_id: Uuid,
    client_msg_id: String,
    sequence_num: u64,
) {
    if let Some(conv) = state.conversations.iter_mut()
        .find(|c| c.id == conversation_id) 
    {
        if let Some(msg) = conv.messages.iter_mut()
            .find(|m| m.client_msg_id.as_ref() == Some(&client_msg_id)) 
        {
            // Update optimistic message with server data
            msg.id = server_msg_id;
            msg.sequence_num = Some(sequence_num);
            msg.is_confirmed = Some(true);
            msg.client_msg_id = None;  // Clear temp ID
        }
    }
}
```

### 6. Graceful Shutdown

```rust
// Before app exit
if let Some(ctrl) = state.ws_ctrl.take() {
    // Trigger graceful shutdown
    let _ = ctrl.shutdown.send(());
    
    // Wait briefly for close to complete
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// Logout to invalidate token
if let Some(token) = &state.token {
    let _ = auth::logout(&base, token).await;
}
```

## Data Models

### Core Types

```rust
// Login response
pub struct LoginResp {
    pub token: String,
    pub user_id: Uuid,           // ⚠️ Uuid, not String
    pub username: String,
    pub last_sequence: u64,
}

// Message DTO
pub struct MessageDto {
    pub id: Uuid,
    pub author_id: Uuid,
    pub conversation_id: Uuid,
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub sequence_num: Option<u64>,  // None for unconfirmed
    pub client_msg_id: Option<String>,  // Client temp ID
    pub is_confirmed: Option<bool>,     // Confirmed by server
}

// Response HTTP dal server (viene convertito in MessageDto)
pub struct MessageResponse {
    pub id: String,              // UUID as string
    pub author_id: String,       // UUID as string
    pub author_username: String,
    pub content: String,
    pub created_at: i64,
    pub sequence_num: Option<i64>,  // Note: i64 in response, u64 in DTO
}

// Conversazione
pub struct ConversationDto {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub owner_id: Uuid,
    pub created_at: i64,
    pub last_read_sequence: i64,
    pub last_activity: i64,
    pub last_msg_seq: i64,
}

// Summary conversazione (con ultimo messaggio e membri)
pub struct ConversationSummary {
    pub conversation: ConversationDto,
    pub last_message: Option<MessageDto>,
    pub members: Option<Vec<ParticipantInfo>>,
}

// Info partecipante
pub struct ParticipantInfo {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub joined_at: Option<i64>,
}
```

### Sequence Number System

Il sistema usa **dual sequence tracking**:

1. **User Sequence** (`u64`): Globale per l'utente
   - Traccia eventi cross-conversation
   - Usato per recovery after disconnect
   - Incrementato ad ogni evento utente

2. **Message Sequence** (`Option<u64>`): Per conversazione
   - Locale a ogni conversazione
   - Garantisce ordering messaggi
   - `None` per messaggi optimistic non ancora confermati

**Type Conversions**:
```rust
// HTTP Response (i64) → DTO (u64)
let sequence_num = response.sequence_num.map(|s| s as u64);

// Il server usa i64 per compatibilità SQL
// Il client usa u64 per sequence counter atomico
```

## Security Considerations

### 1. Token Management

**Storage**: In-memory only (`state.token`)

**Transmission**: 
- HTTP: Bearer token in `Authorization` header
- WebSocket: Bearer token in upgrade request

**Expiration**: 
- Server-side management
- Client detects via 401 responses
- Automatic logout on token expiration

### 2. TLS/SSL

**HTTPS**: Automatic with `https://` URLs
```rust
let base = "https://api.example.com";  // Uses TLS
```

**WSS**: Automatic with `wss://` URLs (WebSocket over TLS)
```rust
let ws_url = base.replacen("https", "wss", 1);  // Secure WebSocket
```

### 3. Input Validation

**Client-side**:
- Message length limits (100KB outgoing, 1MB incoming)
- Username validation (non-empty, no special chars)
- UUID validation (parse before use)
- JSON structure validation

**Server-side**: Complete validation assumed

### 4. Session Management

**Session ID**: 
- Generated client-side per login (`Uuid::new_v4()`)
- Prevents session hijacking/reuse
- Server can identify multiple sessions
- Invalidated on logout

**Token Lifecycle**:
```
Login → Token generated
    ↓
Token used in all requests
    ↓
Logout → Token invalidated server-side
    ↓
New login → New token
```

## Testing API Layer

### Unit Tests

```rust
#[tokio::test]
async fn test_login_success() {
    let resp = auth::login("http://localhost:8080", "alice", "password").await;
    assert!(resp.is_ok());
    
    let login_resp = resp.unwrap();
    assert!(!login_resp.token.is_empty());
    assert!(login_resp.user_id != Uuid::nil());
}

#[tokio::test]
async fn test_login_invalid_credentials() {
    let resp = auth::login("http://localhost:8080", "alice", "wrong").await;
    assert!(resp.is_err());
}

#[tokio::test]
async fn test_message_pagination() {
    let cid = Uuid::new_v4();
    let messages = chat::get_messages_paginated(
        "http://localhost:8080",
        "valid_token",
        cid,
        Some(10),
        None
    ).await;
    
    assert!(messages.is_ok());
    assert!(messages.unwrap().len() <= 10);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_full_connection_flow() {
    // 1. Login
    let resp = auth::login(BASE, "alice", "pass").await.unwrap();
    
    // 2. Create shared sequence
    let user_seq = Arc::new(AtomicU64::new(resp.last_sequence));
    
    // 3. Connect WS
    let session_id = Some(Uuid::new_v4());
    let mut ws = ws::connect(BASE, &resp.token, session_id).await.unwrap();
    
    // 4. Subscribe
    ws::subscribe(&mut ws).await.unwrap();
    
    // 5. Start handler
    let ctrl = ws::spawn_bidirectional_handler(
        ws,
        |_msg| {},
        None,
        user_seq.clone()
    );
    
    // 6. Load conversations
    let convs = conversation::get_conversations_paginated(
        BASE, &resp.token, None, 20
    ).await.unwrap();
    
    assert!(!convs.conversations.is_empty());
    
    // 7. Cleanup
    let _ = ctrl.shutdown.send(());
}

#[tokio::test]
async fn test_message_send_receive() {
    // Setup connection with shared sequence
    let user_seq = Arc::new(AtomicU64::new(0));
    let (ctrl, mut receiver) = setup_test_websocket(user_seq.clone()).await;
    
    // Increment sequence
    let seq = user_seq.fetch_add(1, Ordering::SeqCst);
    
    // Send message
    let msg = json!({
        "type": "chat_message",
        "content": "test",
        "user_sequence": seq
    });
    ctrl.outgoing_tx.send(msg.to_string()).unwrap();
    
    // Wait for echo/confirmation
    let received = tokio::time::timeout(
        Duration::from_secs(5),
        receiver.recv()
    ).await;
    
    assert!(received.is_ok());
}
```

### Mock Server Testing

```rust
use mockito::{mock, server_url};

#[tokio::test]
async fn test_login_with_mock() {
    let _m = mock("POST", "/api/users/login")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{
            "token":"fake_token",
            "user_id":"550e8400-e29b-41d4-a716-446655440000",
            "username":"alice",
            "last_sequence":42
        }"#)
        .create();
    
    let resp = auth::login(&server_url(), "alice", "password").await;
    assert!(resp.is_ok());
    
    let login_resp = resp.unwrap();
    assert_eq!(login_resp.username, "alice");
    assert_eq!(login_resp.last_sequence, 42);
}
```

## Troubleshooting

### Common Issues

**1. WebSocket Connection Refused**
```
Error: Connection refused (os error 111)
```
**Solution**: 
- Verify server is running
- Check firewall rules
- Confirm correct port

**2. Token Expiration**
```
Error: HTTP 401 Unauthorized
```
**Solution**:
- Token expired on server
- Re-login required
- Check server token lifetime settings

**3. Message Too Large**
```
Error: Message too large (150000 bytes), dropping
```
**Solution**:
- Reduce message size
- Split into multiple messages
- Increase client limits (if appropriate)

**4. Pong Timeout**
```
Error: No pong received for 120.5s, connection dead
```
**Solution**:
- Network issue (high latency/packet loss)
- Server not responding to pings
- Check network connectivity

**Note**: `PING_TIMEOUT = 120s` è definito in `ws.rs`

**5. Connection Termination**
```
Error: Connection terminated - error: broken pipe
Error: Connection terminated - error: interrotta  (Italian)
Error: Connection terminated - error: 10054        (Windows)
```
**Solution**:
- Normal during network interruptions
- ConnectionManager will auto-retry
- Check network stability

**6. Missing user_sequence Parameter**
```
Error: mismatched types, expected 4 parameters, found 3
```
**Solution**:
- `spawn_bidirectional_handler()` requires `user_sequence: Arc<AtomicU64>` as 4th parameter
- Create shared atomic counter: `Arc::new(AtomicU64::new(initial_seq))`
- Pass to function: `spawn_bidirectional_handler(ws, on_text, notifier, user_seq)`

### Debug Logging

Enable detailed logging:
```rust
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

Key log patterns:
```
DEBUG: Sending message to server: {...}
DEBUG: Received text message: {...}
DEBUG: 📡 Automatic ping sent from independent tokio task (user_seq: 42)
DEBUG: Received pong from server (payload: 0 bytes)
DEBUG: Sending WebSocket keepalive ping (last pong: 15.2s ago)
DEBUG: WebSocket keepalive ping sent
ERROR: Failed to send message: connection closed
ERROR: No pong received for 125.7s, connection dead
ERROR: Connection terminated by peer - error matched: broken pipe
```

## Conclusioni

L'API Layer del client Rust Ruggine implementa:

✅ **Dual-channel communication** (HTTP + WebSocket)
✅ **Dual-task ping architecture** (30s independent + 60s integrated)
✅ **Shared sequence counter** via `Arc<AtomicU64>` thread-safe
✅ **Graceful shutdown** con close frame e acknowledgment
✅ **Automatic reconnection** con exponential backoff (via ConnectionManager)
✅ **Error handling** robusto con pattern matching multi-platform
✅ **Message validation** (size limits, UUID parsing)
✅ **Session management** sicuro con session ID
✅ **Keepalive multi-layer** (30s app + 60s protocol + 120s timeout)
✅ **Type-safe API** con Result<T> e strong typing
✅ **Consecutive failure tracking** (MAX_FAILURES = 5)
✅ **Connection termination detection** (multi-platform error patterns)
✅ **Pong timestamp tracking** con update corretto alla ricezione

Il design permette:
- Comunicazione real-time affidabile
- Gestione errori automatica e robusta
- Sincronizzazione precisa delle sequenze
- Testing semplificato con mock servers
- Estensibilità sicura con type system
- Debug efficace con logging dettagliato
- Performance ottimizzata con connection reuse
- Ping completamente indipendente dalla UI/egui

---

**Document Version**: 5.0
**Last Updated**: 2024-12-23  

