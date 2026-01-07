# WebSocket Manager - Livello Architetturale

## Panoramica
Il `WebSocketManager` è il sistema di gestione del WebSocket dell'applicazione. Coordina diverse componenti modulari per gestire il ciclo di vita della connessione, l'elaborazione dei messaggi in arrivo e in uscita, e il monitoraggio dello stato del sistema.

## Struttura dei File
```
ws_manager/
├── mod.rs                      # Definisce i moduli
├── ws_manager.rs              # Coordinatore principale
├── connection_manager.rs      # Gestione ciclo vita connessione
├── message_processor.rs       # Elaborazione messaggi in uscita
├── message_handlers.rs        # Elaborazione messaggi in arrivo
├── health_monitor.rs          # Monitoraggio salute sistema
└── rate_limiter.rs           # Protezione anti-spam
```

## Componenti Principali

### 1. WebSocketManager (`ws_manager.rs`)
**Ruolo:** Coordinatore centrale che orchestra tutti i sotto-componenti.

**Responsabilità:**
- Entry point principale tramite `ensure_ws_lifecycle()`
- Coordina l'esecuzione sequenziale delle varie fasi
- Mantiene le istanze dei sotto-componenti
- Fornisce l'interfaccia unificata per il resto dell'applicazione

**Ciclo di esecuzione:**
```rust
pub fn ensure_ws_lifecycle(&mut self, state: &mut AppState) {
    // 1. Processa messaggi in uscita (priorità massima)
    self.message_processor.process_outgoing_messages(state, &mut self.rate_limiter);
    
    // 2. Health check periodico (ogni 60s)
    self.health_monitor.perform_health_check(state, &self.connection_manager);
    
    // 3. Gestisce stato connessione (retry, backoff, ecc.)
    self.connection_manager.manage_connection(state);
    
    // 4. Sveglia UI per aggiornamenti
    (state.egui_waker)();
}
```

**Note:** Il ciclo di ping/pong è attualmente **disabilitato** (riga commentata). La sincronizzazione avviene tramite altri meccanismi.

---

### 2. ConnectionManager (`connection_manager.rs`)
**Ruolo:** Gestisce il ciclo di vita della connessione WebSocket.

**Responsabilità:**
- Apertura e chiusura connessione
- Gestione stati (Disconnected/Connecting/Connected)
- Retry automatico con backoff esponenziale
- Rilevamento timeout e connessioni zombie
- Logout forzato dopo troppi fallimenti

**Funzionalità chiave:**
- `manage_connection()`: Loop principale di gestione
- `start_websocket_connection()`: Avvia connessione async
- `disconnect_websocket(notify_ui: bool)`: Disconnessione pulita
  - `notify_ui=true`: Errore reale, mostra all'utente
  - `notify_ui=false`: Reconnect interno, nessun flash UI
- `increment_backoff()`: Implementa backoff esponenziale
- `reset_backoff()`: Reset dopo connessione riuscita

**Stati gestiti:**
```
┌─────────────┐
│ Disconnected│ ◄──┐
└──────┬──────┘    │
       │           │
       ▼           │
  ┌───────────┐   │
  │ Connecting│───┤ (timeout/error)
  └─────┬─────┘   │
        │         │
        ▼         │
   ┌──────────┐  │
   │ Connected│──┘
   └──────────┘
```

**Backoff Strategy:**
```
Tentativo 1: 1 secondo
Tentativo 2: 2 secondi  
Tentativo 3: 5 secondi
Tentativo 4: 10 secondi
Tentativo 5: 15 secondi
Tentativo 6+: Logout forzato (troppi fallimenti)
```

**Protezioni implementate:**
- Flag `is_connecting_in_progress`: Previene doppie connessioni
- Connection timeout: 30 secondi
- Reset backoff completo dopo logout (evita loop)
- Monitoraggio connessioni zombie (ping senza pong)

---

### 3. MessageProcessor (`message_processor.rs`)
**Ruolo:** Elabora e invia i messaggi in uscita verso il WebSocket.

**Responsabilità:**
- Legge dalla coda `ui_to_net_rx`
- Formatta i messaggi nel formato JSON richiesto dal server
- Invia tramite `ws_ctrl.outgoing_tx`
- Gestisce errori di invio con notifiche specifiche
- Integra il rate limiter

**Funzionalità chiave:**
- `process_outgoing_messages()`: Loop principale di processing
- `format_outgoing_message()`: Serializzazione JSON dei vari tipi

**Tipi di messaggi gestiti:**
- `ChatMessage`: Messaggi chat (con gestione DM stubs e gruppi)
- `CheckUser`: Verifica esistenza utente
- `InviteUser`: Invita utenti a gruppo
- `CreateGroup`: Crea nuovo gruppo
- `Typing`: Notifica digitazione
- `Ping`: Keepalive (attualmente non usato)
- `RequestUserResume`: Richiede eventi persi
- `RequestMessagesResume`: Richiede messaggi persi
- `SequenceAck`: Conferma sequenze ricevute
- `DeleteConversation`: Elimina conversazione
- `MarkRead`: Marca messaggi come letti
- `LeaveGroup`: Abbandona gruppo
- `RemoveMember`: Rimuove membro da gruppo
- `DeleteMessage`: Elimina messaggio
- `DeleteUser`: Elimina account

**Protezioni:**
- Rate limiting (tramite RateLimiter)
- Limite elaborazione: 200 messaggi per ciclo
- Validazione contenuto (lunghezza max, campi vuoti)
- Truncate automatico messaggi troppo lunghi (>10,000 caratteri)
- Re-queue se WebSocket non connesso

**Gestione errori specifici:**
- Invia `MessageSendFailed` per messaggi chat falliti
- Notifica errori specifici per tipo operazione (delete, invite, ecc.)
- Logging dettagliato per debug

---

### 4. MessageHandlers (`message_handlers.rs`)
**Ruolo:** Elabora i messaggi in arrivo dal WebSocket.

**Responsabilità:**
- Parsing JSON dei messaggi ricevuti
- Routing ai handler specifici per tipo
- Conversione in `UiEvent` per l'applicazione
- Validazione e sanity checks

**Funzione principale:**
```rust
pub fn handle_websocket_message(tx: &UnboundedSender<UiEvent>, msg: String)
```

**Tipi di eventi gestiti:**
1. **Messaggi e conversazioni:**
   - `chat_message`: Messaggio diretto (vecchio formato)
   - `message`/`new_message`: Nuovo formato con sequenza
   - `message_confirmation`: Conferma ricezione messaggio
   - `message_ack`: ACK server
   - `message_deleted`: Notifica eliminazione

2. **Sincronizzazione:**
   - `initial_state`: Stato iniziale dopo connessione
   - `conversation_messages`: Caricamento messaggi conversazione
   - `user_events_resume`: Eventi utente recuperati
   - `messages_resume`: Messaggi recuperati
   - `user_resume_complete`: Fine recupero eventi
   - `messages_resume_complete`: Fine recupero messaggi

3. **Conversazioni:**
   - `conversation_created`: Nuova conversazione creata
   - `conversation_created_complete`: Conversazione con tutti i dati
   - `conversation_deleted`: Conversazione eliminata

4. **Gruppi:**
   - `member_added`: Membro aggiunto a gruppo
   - `leave_group_ack`: Conferma uscita da gruppo

5. **Sistema:**
   - `pong`: Risposta a ping
   - `server_heartbeat`: Heartbeat server
   - `user_channel_ready`: Canale utente pronto
   - `user_notification`/`user_event`: Notifiche generiche
   - `error`: Errori dal server
   - `warning`: Avvisi dal server

6. **Utenti:**
   - `check_user_response`: Risposta verifica utente
   - `account_deleted_confirm`: Conferma eliminazione account
   - `logged_out`: Logout forzato dal server

**Protezioni:**
- Limite dimensione messaggio: 100KB (ignora messaggi troppo grandi)
- Validazione campi obbligatori
- Logging dettagliato per messaggi non gestiti
- Gestione graceful di JSON malformato

**Note implementative:**
- Converte UUID da stringhe JSON
- Estrae sequenze per sincronizzazione
- Gestisce campi opzionali con fallback
- Logging strutturato per debug

---

### 5. HealthMonitor (`health_monitor.rs`)
**Ruolo:** Monitora periodicamente lo stato del sistema.

**Responsabilità:**
- Calcolo metriche di salute
- Rilevamento anomalie
- Logging periodico statistiche
- Alert per condizioni critiche

**Controlli eseguiti:**

**1. Sequence Health:**
- Calcola health score tramite `SequenceHandler::get_sequence_health()`
- Alert se health < 0.7
- Verifica ping/pong ratio
- Monitora gap di sequenza

**2. Memory Usage:**
- Conta messaggi cached totali
- Monitora stub DM orfani
- Verifica cache bloat (cached vs active conversations)
- Alert se messaggi > 10,000

**3. Connection Quality:**
- Pong response rate
- Ping mancanti consecutivi
- Frequenza gap

**4. Statistiche connessione:**
- Log connection stats da ConnectionManager
- Uptime tracking

**Frequenza:** Health check ogni 60 secondi

**Soglie di allerta:**
- Sequence health < 0.7
- Cached messages > 10,000 (warning), > 50,000 (critical)
- Pong rate < 50%
- Consecutive gaps > 5
- DM stubs > 20
- Missed pings > max_missed_pings/2

**Logging:**
```
Info:  Health check summary ogni 60s
Warn:  Problemi non critici rilevati
Debug: Statistiche dettagliate connessione
```

---

### 6. RateLimiter (`rate_limiter.rs`)
**Ruolo:** Previene spam e flooding di messaggi.

**Responsabilità:**
- Tracciamento messaggi inviati per finestra temporale
- Reset automatico finestra
- Protezione anti-abuse

**Configurazione:**
- **Max messaggi:** 100 per finestra
- **Durata finestra:** 60 secondi
- **Comportamento:** Blocca invio se limite superato

**Funzionalità:**
```rust
check_rate_limit() -> bool  // Verifica se possiamo inviare
increment_sent()            // Incrementa contatore
```

**Reset automatico:**
- Quando finestra scade, reset contatore a 0
- Logging quando si resetta (se messaggi > 0)

---

## Flusso di Gestione

### Ciclo Principale (`ensure_ws_lifecycle`)
Chiamato continuamente dall'UI loop:

```
┌─────────────────────────────────┐
│ 1. MessageProcessor             │
│    process_outgoing_messages()  │  ◄── Priorità MASSIMA
│    ↓                            │
│    - Legge da ui_to_net_rx     │
│    - Formatta JSON             │
│    - Invia via ws_ctrl         │
│    - Gestisce errori           │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│ 2. HealthMonitor                │
│    perform_health_check()       │  ◄── Ogni 60s
│    ↓                            │
│    - Calcola metriche          │
│    - Log statistiche           │
│    - Alert anomalie            │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│ 3. ConnectionManager            │
│    manage_connection()          │  ◄── Ogni ciclo
│    ↓                            │
│    - Verifica stato            │
│    - Gestisce retry/backoff   │
│    - Monitora timeout          │
│    - Rileva zombie             │
└────────────┬────────────────────┘
             │
             ▼
┌─────────────────────────────────┐
│ 4. egui_waker()                 │  ◄── Sveglia UI
└─────────────────────────────────┘
```

### Flusso Messaggi In Uscita
```
UI Thread
  ↓
  ui_to_net_tx.send(Outgoing::ChatMessage)
  ↓
MessageProcessor (nel ciclo ws_manager)
  ↓
  ui_to_net_rx.try_recv()
  ↓
  format_outgoing_message()
  ↓
  ws_ctrl.outgoing_tx.send(json_string)
  ↓
WebSocket Task (async)
  ↓
Server
```

### Flusso Messaggi In Arrivo
```
Server
  ↓
WebSocket Task (async)
  ↓
spawn_bidirectional_handler callback
  ↓
handle_websocket_message()
  ↓
  - Parse JSON
  - Route per tipo
  - Handler specifico
  ↓
  ui_tx.send(UiEvent::*)
  ↓
UI Thread (consume eventi)
```

### Gestione Eventi Connessione
```
┌─────────────┐
│ Disconnected│
└──────┬──────┘
       │ (has token & authenticated)
       ▼
  start_websocket_connection()
       │
       ├─► spawn async task
       │   └─► connect()
       │       └─► subscribe()
       │           └─► spawn_bidirectional_handler()
       │               └─► send(WsControlReady)
       ▼
  ┌───────────┐
  │ Connecting│
  └─────┬─────┘
        │
        ├─► SUCCESS: send(WsConnected)
        │   ├─► reset_backoff()
        │   └─► WsStatus::Connected
        │
        ├─► TIMEOUT (30s): disconnect, backoff
        │   └─► WsStatus::Disconnected
        │
        └─► ERROR: send(WsError + WsDisconnected)
            ├─► increment_backoff()
            └─► WsStatus::Disconnected
                └─► Retry dopo backoff delay
```

---

## Statistiche Tracciate

### ConnectionStats
```rust
pub struct ConnectionStats {
    pub connection_attempts: u32,      // Tentativi totali
    pub successful_connections: u32,   // Connessioni riuscite
    pub disconnections: u32,           // Numero disconnessioni
    pub last_error: Option<String>,    // Ultimo errore
    pub uptime_start: Option<Instant>, // Inizio uptime corrente
}
```

### SequenceStats (da AppState, monitorato da HealthMonitor)
```rust
pub struct SequenceStats {
    pub ping_count: u32,           // Ping inviati
    pub pong_count: u32,           // Pong ricevuti
    pub gaps_detected: u32,        // Gap rilevati
    pub events_recovered: u32,     // Eventi recuperati
    pub average_gap_size: f64,     // Dimensione media gap
    pub last_gap_time: Option<Instant>, // Ultimo gap
}
```

---

## Protezioni Implementate

### 1. Anti-Loop di Reconnect
**Problema:** Connessioni multiple sovrapposte.
**Soluzione:**
```rust
is_connecting_in_progress: bool  // Lock manuale
```
- Settato a `true` quando inizia connessione
- Reset a `false` solo in stati finali (Connected/Disconnected)
- Check prima di ogni tentativo

### 2. Timeout Protection
**Problema:** Hang infiniti durante connessione.
**Soluzione:**
```rust
connection_attempt_start: Option<Instant>  // Timer dedicato
```
- Impostato all'inizio del tentativo
- Verificato in `WsStatus::Connecting`
- Timeout dopo 30 secondi → forza disconnessione

### 3. Zombie Connection Detection
**Problema:** Connessione aperta ma non funzionante.
**Soluzione:**
```rust
if state.sequence_stats.pong_count == 0 && 
   state.sequence_stats.ping_count > 5 {
    state.request_ws_reconnect = true;
}
```

### 4. Memory Protection
**Problema:** Uso memoria eccessivo.
**Soluzioni:**
- Limite messaggi processati: 200 per ciclo
- Alert se cached > 10,000
- Warning se stub DM > 20
- Critical warning se cached > 50,000

### 5. Rate Limiting
**Problema:** Spam/flooding.
**Soluzione:**
- Max 100 messaggi/minuto
- Check prima di ogni invio
- Blocco temporaneo se superato

### 6. Backoff Intelligente
**Problema:** Retry storms dopo fallimenti.
**Soluzione:**
- Delay crescente: 1s → 2s → 5s → 10s → 15s
- Reset completo dopo:
  - Connessione riuscita
  - Logout (token diventa None)
  - Logout forzato (>= 6 fallimenti)

### 7. Silent Reconnect
**Problema:** Flash "disconnected" durante login.
**Soluzione:**
```rust
disconnect_websocket(state, notify_ui: bool)
```
- `notify_ui=false` per reconnect interni
- `notify_ui=true` per errori reali

---

## Integrazione con AppState

### Campi letti (read-only):
- `ws_status: WsStatus` - Stato corrente connessione
- `token: Option<String>` - Token autenticazione
- `ws_ctrl: Option<WsControl>` - Controllo WebSocket
- `sequence_stats: SequenceStats` - Statistiche sequenze
- `current_session_id: i64` - ID sessione corrente
- `conversations: Option<Vec<...>>` - Lista conversazioni
- `conversation_messages: HashMap<...>` - Cache messaggi
- `dm_stubs: HashMap<Uuid, (String, Instant)>` - Stub DM temporanei
- `pending_confirmations: HashMap<...>` - Messaggi in attesa conferma

### Campi modificati:
- `ws_status` - Aggiornato durante transizioni stato
- `ws_ctrl` - Impostato quando connessione pronta
- `request_ws_reconnect` - Flag per forzare reconnect
- `connection_attempt_start` - Timer timeout connessione

### Channels usati:
- `ui_tx: UnboundedSender<UiEvent>` - Eventi verso UI
- `ui_to_net_rx: UnboundedReceiver<Outgoing>` - Messaggi da UI
- `ws_ctrl.outgoing_tx` - Invio verso WebSocket
- `ws_ctrl.shutdown` - Shutdown WebSocket task

---

## Logging e Debug

### Livelli di Log

**Info:**
- Connessioni/disconnessioni
- Tentativi di retry
- Health check summary
- Operazioni principali

**Debug:**
- Dettagli messaggi (primi 200 caratteri)
- Transizioni di stato
- Statistiche connessione
- Formattazione messaggi

**Warn:**
- Problemi non critici
- Retry con backoff
- Health issues (health < 0.7)
- High memory usage
- Pong rate basso
- Gap frequenti

**Error:**
- Fallimenti critici
- Timeout connessione
- Errori JSON parsing
- Messaggi troppo grandi
- Errori invio WebSocket

### Esempi Log Importanti

```
// Connessione
"Starting WebSocket connection attempt #3 to ws://... 
 (retry after 2 failures, next retry in 5s)"

// Health
"Health check - WS: true, Seq Health: 0.85, 
 Cached Msgs: 523, DM Stubs: 2, Missed Pings: 0/3"

// Problemi
"Poor sequence health detected: 0.65"
"Connection appears to be zombie (no pongs received)"
"Too many connection failures (6), forcing logout"

// Messaggi
"Sending WebSocket message: {"type":"chat_message"..."
"Received WebSocket message: {"type":"new_message"..."
```

---

## Note di Implementazione

### Thread Safety
- **Tutti i componenti sono single-threaded** (chiamati solo dal main UI thread)
- Operazioni async tramite `tokio::spawn`
- Comunicazione thread-safe via channels (mpsc)
- Nessun lock esplicito necessario

### Error Handling
- **Non-blocking:** Errori non bloccano il ciclo principale
- **Fallback sicuri:** Stati consistenti anche dopo errori
- **Notifiche UI:** Tutti gli errori rilevanti comunicati tramite `UiEvent::Error`
- **Logging completo:** Ogni errore loggato con contesto

### Performance
- **Health check non-blocking:** Solo ogni 60s
- **Batch processing:** Max 200 messaggi per ciclo
- **Early returns:** Skip work quando non necessario
- **Minimal allocations:** Riuso buffer dove possibile
- **Cleanup lazy:** Nessun cleanup proattivo, solo alert

### Ownership e Lifetime
- `ConnectionManager` possiede `ConnectionStats`
- `WebSocketManager` possiede tutte le componenti
- `AppState` prestato mutabilmente ad ogni ciclo
- Nessun lifetime complicato, tutto `'static` nelle async task

---

## Pattern Architetturali Usati

### 1. Facade Pattern
`WebSocketManager` fornisce interfaccia semplificata per sistema complesso.

### 2. Strategy Pattern
Diversi handler per diversi tipi di messaggi in `MessageHandlers`.

### 3. State Machine
`ConnectionManager` implementa state machine esplicita (Disconnected/Connecting/Connected).

### 4. Observer Pattern
Comunicazione event-driven tramite channels (UI ↔ Network).

### 5. Separation of Concerns
Ogni componente ha responsabilità ben definita e isolata.

