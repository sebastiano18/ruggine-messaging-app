# Business Logic - Rust Ruggine Chat Client

## Panoramica

La business logic del client Rust Ruggine implementa le regole applicative e la logica di dominio attraverso un sistema di **Event Handlers specializzati** coordinati da un **Event Dispatcher centrale**. Ogni handler è responsabile di un dominio specifico e muta lo stato dell'applicazione in risposta agli eventi.

## Architettura Event-Driven

### Event Dispatcher

**Tipo**: `EventDispatcher` (struct statica con metodo `handle_event`)

**Responsabilità**: Router centrale che smista gli eventi agli handler appropriati

**Pattern**: Match exhaustive su `UiEvent` enum

**Flusso**:
```
UiEvent dal canale ui_rx
    ↓
EventDispatcher::handle_event()
    ↓
Match sull'evento → Handler specifico
    ↓
Handler muta AppState
    ↓
waker() trigger repaint UI
```

**Caratteristiche**:
- Singolo punto di ingresso per tutti gli eventi
- Routing esplicito tramite match statement
- Chiamata sincrona agli handler (no async)
- Trigger automatico waker alla fine

**Gestione Speciale - Fetch On-Demand**:

Quando arriva `WsIncoming` per conversazione non caricata, dispatcher gestisce 3 casi:

```rust
CASO 1: Conversazione NON esiste e NON in fetch
    → Mark fetching_conversations.insert(conv_id)
    → FORCE-BUFFER messaggio (bypass gap check)
    → Spawn fetch task → UiEvent::ConversationSummaryFetched

CASO 2: Conversazione NON esiste MA in fetch
    → FORCE-BUFFER messaggio
    → Aspetta fetch completion

CASO 3: Conversazione ESISTE
    → Processa normalmente tramite WebSocketHandler
    → Move conversation to top
```

**Gestione Fetch Completion - ConversationSummaryFetched**:

Quando il fetch della conversazione completa con successo:

```
1. Inizializza conversation_sequences:
   - conversation_sequences[conv_id] = conversation.last_msg_seq
   - conversation_sequences_confirmed[conv_id] = conversation.last_msg_seq
   
2. Tenta delivery messaggi bufferizzati:
   - Loop su messaggi consecutivi dal buffer (expected = last_msg_seq + 1)
   - Per ogni messaggio bufferizzato:
     * Verifica non duplicato in cache (skip se esiste)
     * Inserimento binario ordinato nella cache (per seq o timestamp)
     * Update conversation_sequences tramite SequenceHandler
     * Traccia max_delivered_seq (massima sequenza deliverizzata)
     * Se conversazione corrente (cid): aggiungi anche alla UI
   - Log final sequence dopo delivery
   
3. Calcola unread count (se NON conversazione corrente):
   - unread_count = max_delivered_seq - last_read_sequence
   - Se unread_count > 0: insert in conversation_unread_counts
   - Altrimenti: remove da conversation_unread_counts
   - Se conversazione corrente: remove (no unread)
   
4. Rimuovi da fetching_conversations set
   
5. Verifica duplicati:
   - Check se conversazione già esiste (evita inserimento)
   - Skip insert se già presente
   
6. Inserisci conversazione nuova:
   - Clone conversation DTO
   - CRITICO: Aggiorna last_msg_seq con max_delivered_seq
   - Insert in posizione [0]
   - Sort conversations per last_activity (descending)
   - Log insertion e total count
```

**Note**: 
- Buffer viene consumato durante delivery (messaggi rimossi)
- Max_delivered_seq traccia la sequenza più alta processata
- Sort garantisce ordine cronologico corretto

**Gestione Fetch Failed - ConversationFetchFailed**:

Quando il fetch della conversazione fallisce:

```
1. Log warning con conversation_id
2. Rimuovi da fetching_conversations (permette retry)
3. Messaggi bufferizzati rimangono nel buffer
4. Retry possibile su prossimo messaggio in arrivo
```

**Note**:
- Non pulisce buffer (messaggi conservati per retry)
- Conversazione può essere ri-fetched su prossimo WsIncoming

**Force-Buffer**: Messaggi per conversazioni non caricate vengono bufferizzati automaticamente senza gap check, saranno processati dopo il fetch completion.

### Struttura Handler

**Pattern comune**:
```rust
pub struct XxxHandler;

impl XxxHandler {
    pub fn handle_xxx(state: &mut AppState, ...) {
        // 1. Validazione input
        // 2. Mutazione AppState
        // 3. Side effects (invio eventi, spawn tasks)
        // 4. Logging
    }
}
```

**Caratteristiche**:
- Metodi statici (no instance state)
- Parametro `&mut AppState` per mutazioni
- No return values (side effects via AppState)
- Possibilità di inviare nuovi eventi tramite `state.ui_tx`

## Handler Specializzati

### 1. AuthHandler

**File**: `auth_handler.rs`

**Dominio**: Autenticazione e gestione account

**Eventi Gestiti**:
- `LoginStarted` / `RegisterStarted` → cambio `LoginState`
- `Logged(token, user_id, last_sequence)` → setup sessione
- `LoggedOut` → cleanup completo stato
- `DeleteAccountStart/Cancel/Confirm` → workflow eliminazione

**Logica Principale**:

#### handle_logged()
```
1. Salva token e user_id in AppState
2. Genera nuovo session_id (UUID v4) per WebSocket
3. Inizializza sequenze:
   - user_sequence_confirmed = last_sequence
   - user_sequence_received = last_sequence
   - Clear conversation_sequences
4. Reset sequence_stats
5. Cambio page → Conversations
6. Trigger reconnect WebSocket (request_ws_reconnect = true)
7. Clear messaggi errore login
```

**Session ID**: Generato ad ogni login per prevenire riutilizzo connessioni e race conditions multiple WebSocket

#### handle_logged_out()
```
1. Shutdown WebSocket control (se presente)
2. Clear token, user_id, session_id
3. Chiusura TUTTI i modal/popup:
   - show_account_modal, show_create_group_modal
   - show_invite_popup, show_group_info_popup
   - confirm_delete_account
   - pending_deletion, pending_message_deletion
   - pending_member_kick, pending_user_check
   - user_check_request_id, user_check_timestamp
4. Reset stato interno popup:
   - create_group_popup.reset()
   - invite_popup.reset()
5. Clear dati applicazione:
   - messages, conversations
   - conversation_messages cache
   - conversation_unread_counts
   - members_list (lista membri gruppi)
   - dm_stubs, group_stubs
   - pending_confirmations
6. Reset pagination state (is_loading_more_conversations, has_more_conversations, next_cursor, fetching_conversations)
7. Reset sequenze (confirmed=0, received=0)
8. Clear buffer riordinamento (message_reorder_buffer, user_event_reorder_buffer)
9. Reset stato caricamento (is_loading, is_loading_more, is_initial_load_complete, has_more_messages, is_recovering_user_events, is_recovering_messages, pending_resume_requests)
10. Reset ping/pong (missed_pings, last_ping_time)
11. Clear UI fields (group_name, dm_username, conv_title, input, invite_conversation_id, last_invite_token, last_created_invite)
12. Page → Auth, LoginState → Idle, WsStatus → Disconnected
```

**Note**: Username/password NON vengono puliti per permettere retry rapido

#### handle_delete_account_confirm()
```
1. Verifica WebSocket connesso
2. Invia Outgoing::DeleteUser via ui_to_net_tx
3. Error handling se invio fallisce
```

### 2. MessageHandler

**File**: `message_handler.rs`

**Dominio**: Ciclo di vita messaggi (invio, conferma, eliminazione)

**Eventi Gestiti**:
- `MessageSendFailed(msg_id)` → marca messaggio come fallito
- `MessageConfirmation{...}` → aggiorna messaggio ottimistico
- `MessageDeleted{...}` → rimuove messaggio

**Logica Principale**:

#### handle_message_send_failed()
```
1. Trova messaggio in state.messages tramite msg_id
2. Estrai client_msg_id e conversation_id
3. Marca is_confirmed = Some(false) in UI
4. Marca is_confirmed = Some(false) in cache
5. Rimuovi da pending_confirmations (se client_msg_id presente)
6. Log errore
```

#### handle_message_confirmation()
```
Scenario 1: Messaggio già esiste (ricevuto via resume)
    → Rimuovi messaggio ottimistico da UI e cache (retain by client_msg_id)
    → Pulisci pending_confirmations
    → Exit (no further processing)

Scenario 2: Messaggio in pending_confirmations
    1. Rimuovi da pending_confirmations
    2. Aggiorna in state.messages:
       - id → server_msg_id
       - sequence_num → sequence
       - is_confirmed = Some(true)
    3. Aggiorna in conversation_messages cache (stesso update)
    4. Sort cache per sequence_num (se presente)
    5. Update conversation sequence via SequenceHandler
    6. Aggiorna ConversationDto metadata:
       - last_msg_seq = sequence
       - last_activity = confirmed_msg.created_at
    7. Log conferma

Scenario 3: Messaggio sconosciuto
    → Warning log
    → No action
```

**Optimistic UI Pattern**:
- Client genera UUID client-side (`client_msg_id`)
- Messaggio inserito immediatamente con `is_confirmed = None`
- Server conferma con `server_msg_id` reale + sequence
- Update in-place tramite match su `client_msg_id`

#### handle_message_deleted()
```
1. Rimuovi da state.messages (se conversazione attiva)
2. Rimuovi da conversation_messages cache
3. Log eliminazione
```

### 3. ConversationHandler

**File**: `conversation_handler.rs`

**Dominio**: Gestione conversazioni (creazione, eliminazione, messaggi, membri)

**Eventi Gestiti**:
- `InitialStateReceived{...}` → setup iniziale conversazioni
- `ConversationMessagesReceived{...}` → caricamento messaggi
- `ConversationConfirmed{...}` → conferma conversazione creata
- `OlderMessagesLoaded{...}` → caricamento paginato
- `Opened(cid)` → apertura conversazione
- `Closed(cid)` → chiusura conversazione
- `ConversationDeleted(cid)` → eliminazione conversazione
- `ConversationCreated(cid)` → nuova conversazione
- `DmStubCreated{...}` → stub conversazione DM
- `ConversationsLoaded{...}` → batch conversazioni
- `AllMessagesLoaded{...}` → caricamento completo
- `RefreshedMsgs{...}` → refresh messaggi
- `SingleConversationLoaded{...}` → singola conversazione
- `InitialLoadComplete` → fine caricamento iniziale
- `LoadingProgress(msg)` → progresso caricamento
- `MembersLoaded{...}` → caricamento membri
- `ConversationCompleteFetched{...}` → fetch completo conversazione
- `LastMessageUpdate{...}` → aggiornamento ultimo messaggio

**Logica Principale**:

#### handle_initial_state_received()
```
1. Set conversations = received conversations
2. Set has_more_conversations = (count == 20)
3. Inizializza user_sequence via SequenceHandler::set_initial_user_sequence():
   - user_sequence_confirmed = user_sequence
   - user_sequence_received = user_sequence
   - user_sequence_shared (Arc) = user_sequence
4. Carica membri da members_by_conversation map
5. Inizializza conversation_sequences per conversazioni con messaggi:
   - Usa last_msg_seq (NON last_read_sequence)
   - **Inizializza SOLO se last_msg_seq > 0**
   - Evita gap permanenti con messaggi non letti
   - Evita inizializzazione per conversazioni vuote
   - conversation_sequences[cid] = last_msg_seq
   - conversation_sequences_confirmed[cid] = last_msg_seq
6. Calcola unread counts:
   - last_cached_seq - last_read_sequence
   - Max(0, unread_count)
7. Pulisci dm_stubs che ora sono conversazioni reali
8. Log statistiche (total conversations, unread, has_more)
```

**Nota importante**: Usa `last_msg_seq` per inizializzare sequenze, non `last_read_sequence`, per evitare gap artificiali quando ci sono messaggi non letti.

#### handle_conversation_messages_received()
```
1. Log messaggi ricevuti (count, has_more, sequences)
2. Sort messaggi per sequence_num (o created_at se no seq)
3. Insert in conversation_messages cache
4. Inizializza conversation_sequences se necessario:
   - Trova max sequence_num tra messaggi
   - Set conversation_sequences[cid] = max_seq
   - Set conversation_sequences_confirmed[cid] = max_seq

Caso 1: Conversazione corrente (cid match)
    - state.messages = sorted_messages
    - Update conv_title da conversations list
    - NOTA: has_more_messages NON viene aggiornato qui

Caso 2: Conversazione background
    - Solo cache update
    - Log cache update
```

#### handle_conversation_confirmed()
```
1. Identifica stub da rimuovere (se client_temp_id fornito)
2. Determina se stub era conversazione attiva (was_active_stub)
3. Rimuovi stub:
   - Da dm_stubs map
   - Da conversation_sequences
   - Da conversations list
4. Aggiungi conversazione reale:
   - Rimuovi duplicati (per ID reale)
   - Insert(0) - conversazione in cima
   - Log aggiunta
5. Se messaggi presenti:
   - Insert in conversation_messages cache
   - Inizializza sequences con last message seq
6. Se was_active_stub:
   - Aggiorna cid con ID reale
   - Aggiorna conv_title
   - Aggiorna state.messages
7. Se conversazione già attiva (cid match):
   - Aggiorna state.messages
```

#### handle_older_messages_loaded()
```
1. Set is_loading_more = false
2. Se no new_messages:
   - Set has_more_messages[cid] = false
   - Exit
3. Se new_messages.len() < 30:
   - Set has_more_messages[cid] = false
4. Merge con messaggi esistenti:
   - Extend new + existing
   - Deduplica per ID
   - Sort per sequence o created_at
5. Update state.messages e cache
```

#### handle_opened()
```
1. Set cid, page = Chat
2. Clear state.messages, reset is_loading_more
3. Set has_more_messages[cid] = true
4. Inizializza conversation_sequences se assenti (=0)
5. Reset unread count a 0
6. Carica dalla cache se disponibile:
   - state.messages = cached
   - Update sequences con max_seq
   - Invia mark_read SE:
     * Conversazione esiste ancora
     * max_seq > last_read_sequence
   - Update local last_read_sequence
7. Pre-caricamento checks:
   - Skip se è DM stub
   - Skip se è Group stub
   - Skip se chat vuota e no messaggi sul server
   - Altrimenti: load_older_messages()
```

**Mark Read Logic**: Mark read viene inviato solo se ci sono effettivamente messaggi non letti (max_seq > last_read_sequence)

#### handle_conversation_deleted()
```
1. Rimuovi da conversations list
2. Clear conversation_messages cache
3. Clear sequences (conversation_sequences, confirmed, is_recovering)
4. Se conversazione corrente:
   - Reset cid, conv_title, messages
   - Page → Conversations
5. Rimuovi dm_stub se presente
```

#### handle_conversation_created()
```
1. Set cid = new conversation ID
2. Page = Chat
3. Clear messages
```

#### handle_dm_stub_created()
```
1. Aggiungi a dm_stubs map
2. Set cid = stub_id
3. Page = Chat
4. Clear messages
5. Init empty cache entry
6. Set conv_title = target_username
```

#### handle_conversations_loaded()
```
1. Identifica stubs da rimuovere:
   - Match DM per title
2. Pulisci stubs obsoleti
3. Set conversations = loaded
```

#### handle_all_messages_loaded()
```
1. Set conversation_messages = all_messages
2. Per ogni conversazione:
   - Trova max_seq tra messaggi
   - Init conversation_sequences
   - Init conversation_sequences_confirmed
3. Se conversazione attiva:
   - Update state.messages dalla cache
```

#### handle_refreshed_msgs()
```
1. Update state.messages = new messages
2. Update cache per cid corrente
3. Update conversation_sequence con max_seq
```

#### handle_single_conversation_loaded()
```
1. Se conversazione esiste: update in-place
2. Se conversazione nuova: append
3. Se conversations vuoto: init con singola conv
```

#### handle_initial_load_complete()
```
1. Set is_initial_load_complete = true
2. Set is_loading = false
```

#### handle_loading_progress()
```
1. Log progresso
```

#### handle_conversation_complete_fetched()
```
1. Update o append conversazione in list
2. Calcola max_seq da messaggi
3. Init sequences se max_seq > 0
4. Update cache messaggi
5. Se conversazione attiva: update UI messages
6. Rimuovi dm_stub se presente
```

#### handle_closed()
```
1. Close show_group_info_popup
2. Close show_invite_popup
```

#### handle_members_loaded()
```
1. Insert membri in members_list map
2. Set is_loading_members = false
```

#### handle_last_message_update()
```
1. Verifica conversazione esiste ancora
2. Update last_activity (max con messaggio)
3. Calcola unread: last_msg_seq - last_read_sequence
4. Update conversation_unread_counts
5. Aggiungi messaggio a cache
```

### 4. WebSocketHandler

**File**: `websocket_handler.rs`

**Dominio**: Gestione eventi WebSocket e sincronizzazione real-time

**Eventi Gestiti**:
- `WsControlReady(ctrl)` → salva control handle
- `WsConnected` → connessione stabilita
- `WsDisconnected` → connessione persa
- `WsError(error)` → errore WebSocket
- `WsIncoming(msg)` → nuovo messaggio real-time

**Logica Principale**:

#### handle()
```
WsControlReady:
    → Salva ws_ctrl in AppState

WsConnected:
    → ws_status = Connected
    → SequenceHandler::reset_sequence_system()
    → Log connessione

WsDisconnected:
    → ws_status = Disconnected
    → SequenceHandler::reset_sequence_on_disconnect()
    → Log disconnessione

WsError:
    → Log errore

WsIncoming:
    → handle_incoming_message()
```

#### handle_incoming_message()
```
1. VALIDAZIONE:
   - validate_incoming_message() (content, username, length)
   - Skip se invalido

2. BUFFER DI RIORDINO:
   Se sequence > expected:
     → Log warning
     → SequenceHandler::update_conversation_sequence() (without confirm)
     → BufferHandler::buffer_message_for_reorder()
     → EXIT
   
   Se sequence < expected:
     → Log debug (già processato)
     → EXIT
   
   Se sequence == expected:
     → SequenceHandler::update_conversation_sequence() (with confirm)
     → Continue processing

3. VERIFICA DUPLICATI CONVERSAZIONI:
   Se count > 1:
     → Error log
     → Deduplica conversations list
     → Keep first occurrence per ID

4. CONVERSIONE DM STUB:
   Se messaggio per dm_stub:
     → Estrai target_username da dm_stubs
     → Crea ConversationDto real
     → Determina title (author o target)
     → Remove stub da dm_stubs
     → Deduplica conversations list
     → Add real conversation
     → Add system message "Chat attiva!"

5. AGGIORNA CACHE:
   → update_message_cache() con insert ordinato
   → Return false se duplicato → EXIT
   → Binary search per posizione corretta
   → Insert messaggio
   → verify_cache_sequence_integrity()
   → check_and_send_auto_mark_read() ← AUTOMATICO

6. AGGIORNA UI:
   Se conversazione corrente:
     → update_ui_messages() con insert ordinato
   
   Altrimenti:
     → Increment unread_count (se non da user corrente)

7. SVUOTA BUFFER:
   Loop su BufferHandler::get_next_buffered_message():
     → SequenceHandler::update_conversation_sequence()
     → update_message_cache()
     → Se conversazione corrente: update_ui_messages()
     → Altrimenti: increment unread_count
```

#### check_and_send_auto_mark_read()
```
CONDIZIONI per auto-mark_read:
1. Messaggio appartiene a conversazione corrente
2. Messaggio NON dall'utente corrente
3. Messaggio ha sequence_num

Se tutte soddisfatte:
  → Invia Outgoing::MarkRead
  → Update local last_read_sequence
  → Azzera conversation_unread_counts
```

**IMPORTANTE**: Auto-mark_read viene chiamato automaticamente per OGNI messaggio aggiunto alla cache (sia live che bufferizzato)

#### update_message_cache()
```
1. Get or create cache entry
2. Check duplicati → return false se esiste
3. Binary search per posizione:
   - Sort per sequence_num (se presente)
   - Fallback: created_at + id
4. Insert ordinato
5. verify_cache_sequence_integrity()
6. check_and_send_auto_mark_read() ← AUTOMATICO
7. Return true
```

#### update_ui_messages()
```
1. Check duplicati → skip se esiste
2. Binary search per posizione UI:
   - Sort per sequence_num (se presente)
   - Fallback: created_at + id
3. Insert ordinato in state.messages
```

#### verify_cache_sequence_integrity()
```
1. Estrai sequence_num da messaggi
2. Check windows di 2 messaggi consecutivi
3. Log warning se gap > 1 trovato
```

### 5. SequenceHandler

**File**: `sequence_handler.rs`

**Dominio**: Sincronizzazione sequenze e gap detection

**Eventi Gestiti**:
- `PongReceived{...}` → risposta ping
- `UserEventsResume{...}` → eventi recuperati
- `MessagesResume{...}` → messaggi recuperati
- `SendPing` → ping manuale

**Metodi Pubblici**:

#### set_initial_user_sequence()
```
1. Set user_sequence_confirmed = sequence
2. Set user_sequence_received = sequence
3. Set user_sequence_shared (Arc atomic) = sequence
4. Log inizializzazione
```

#### update_user_sequence()
```
1. GAP DETECTION:
   Se sequence > current + 1:
     → Calcola gap_size
     → Log warning
     → Increment gaps_detected
     → Update last_gap_time
     
     Se gap_size >= 3:
       → Log large gap
       → request_user_events_resume(current)
     
     Altrimenti:
       → Log small gap
       → Handle at next ping

2. UPDATE RECEIVED:
   Se sequence > user_sequence_received:
     → user_sequence_received = sequence
     → Increment total_events_received

3. UPDATE CONFIRMED:
   Se sequence == user_sequence_confirmed + 1:
     → user_sequence_confirmed = sequence
     → user_sequence_shared.store(sequence)
     → Log confirmed
```

**Thresholds**:
- User events gap >= 3 → Immediate resume
- User events gap < 3 → Wait for next ping

#### update_conversation_sequence()
```
1. Get current sequence (default 0)

2. GAP DETECTION:
   Se sequence > current + 1:
     → Calcola gap_size
     → Log warning
     → Increment gaps_detected
     → Update last_gap_time
     
     Se gap_size >= 5:
       → Log large gap
       → request_messages_resume(cid, current)
     
     Altrimenti:
       → Log small gap
       → Handle at next ping

3. UPDATE SEQUENCES:
   Se sequence > current:
     → conversation_sequences[cid] = sequence

4. UPDATE CONFIRMED:
   Get confirmed = conversation_sequences_confirmed[cid]
   Se sequence == confirmed + 1:
     → conversation_sequences_confirmed[cid] = sequence
     → Log confirmed
```

**Thresholds**:
- Messages gap >= 5 → Immediate resume
- Messages gap < 5 → Wait for next ping

#### handle_pong()
```
1. Increment pong_count
2. Reset missed_pings = 0
3. Se user_events_gap presente e detected:
   → Log warning
   → Increment gaps_detected
   → request_user_events_resume(client_seq)
4. Log se gaps_detected
```

#### handle_user_events_resume()
```
1. Set is_recovering_user_events = false
2. Decrement pending_resume_requests
3. Increment events_recovered
4. Per ogni evento:
   → update_user_sequence()
   → Send UiEvent::UserNotification con recovery=true
5. Auto mark_read per conversazione corrente:
   → Ottieni seq corrente
   → Send Outgoing::MarkRead
```

**AUTO MARK READ**: Invia automaticamente mark_read dopo resume per conversazione corrente

#### handle_messages_resume()
```
1. Log detailed state (UI, pending, incoming)
2. Set is_recovering_messages[cid] = false
3. Decrement pending_resume_requests
4. Update conversation_sequence con max_seq
5. Build existing_ids set (cache + UI)
6. Process incoming messages:
   - Skip se già esiste per server ID
   - Se client_msg_id match pending:
     * Mark per removal da pending
     * Mark ottimistico per removal
   - Se client_msg_id match UI (no pending):
     * Mark ottimistico per removal
   - Add a truly_new_messages
7. Remove pending confirmations
8. Remove ottimistici da UI e cache
9. Merge truly_new_messages:
   - Add to cache
   - Sort cache per sequence
   - Se conversazione corrente: extend UI messages
   - Sort UI per sequence
10. Clear message_reorder_buffer[cid]
11. Auto mark_read per conversazione corrente
```

**DEDUPLICAZIONE AVANZATA**:
- Match per server ID (existing_ids)
- Match per client_msg_id (pending_confirmations)
- Match per client_msg_id in UI (ottimistici non confermati)
- Solo messaggi veramente nuovi vengono aggiunti

#### send_ping()
```
1. Get user_seq = user_sequence_confirmed
2. Increment ping_count
3. Build JSON ping message
4. Send via ws_ctrl.outgoing_tx (direct, bypasses rate limit)
```

**DIRECT CHANNEL**: Usa canale diretto WebSocket senza rate limiter

#### request_user_events_resume()
```
1. Check is_recovering_user_events → skip se già in corso
2. Set is_recovering_user_events = true
3. Increment pending_resume_requests
4. Build Outgoing::RequestUserResume con from_sequence e limit=100
5. Send via ui_to_net_tx
```

#### request_messages_resume()
```
1. Check is_recovering_messages[cid] → skip se già in corso
2. Set is_recovering_messages[cid] = true
3. Increment pending_resume_requests
4. Build Outgoing::RequestMessagesResume con cid, from_sequence, limit=100
5. Send via ui_to_net_tx
```

#### reset_sequence_system()
```
1. Clear is_recovering_user_events
2. Clear is_recovering_messages map
3. Reset pending_resume_requests = 0
4. Reset sequence_stats
5. Clear message_reorder_buffer
6. Clear user_event_reorder_buffer
7. Log reset con current sequences
```

**PRESERVA SEQUENCES**: Non azzera le sequenze, solo lo stato di recovery

#### reset_sequence_on_disconnect()
```
1. Log disconnect con current sequences
2. Call reset_sequence_system()
```

#### get_sequence_health()
```
Se ping_count == 0:
    → Return 1.0

pong_rate = pong_count / ping_count
gap_penalty = min(gaps_detected * 0.1, 0.5)
missed_penalty = (missed_pings / max_missed_pings) * 0.3

health = max(pong_rate - gap_penalty - missed_penalty, 0.0)
return clamp(health, 0.0, 1.0)
```

**COMPONENTS**:
- `pong_rate`: Percentuale pong ricevuti (baseline)
- `gap_penalty`: Max 50% per gap frequenti (0.1 per gap)
- `missed_penalty`: Max 30% per ping mancati

### 6. UserNotificationHandler

**File**: `user_notification_handler.rs`

**Dominio**: Gestione notifiche utente e eventi di sistema

**Eventi Gestiti**:
- `UserNotification{...}` → evento user-level

**Logica Principale**:

#### handle_user_notification()
```
1. Calculate expected = user_sequence_confirmed + 1

2. Se NOT recovery:
   Se sequence > expected:
     → Log warning
     → Buffer evento completo (JSON con metadata)
     → BufferHandler::buffer_user_event_for_reorder()
     → EXIT
   
   Se sequence < expected E sequence > 0:
     → Log debug (già processato)
     → EXIT

3. Se sequence > 0:
   → SequenceHandler::update_user_sequence()

4. Process evento: process_user_notification()

5. Deliver buffered events:
   → BufferHandler::try_deliver_buffered_user_events()
   → Per ogni evento bufferizzato:
     * update_user_sequence()
     * Estrai metadata
     * process_user_notification()
```

#### process_user_notification()
```
Event type dispatch:

"new_message":
    → handle_new_message()

"conversation_deleted":
    → Send UiEvent::ConversationDeleted

"member_kicked":
    → Send UiEvent::ConversationDeleted (rimuovi dalla lista)

"member_added":
    → handle_member_added()

"member_removed":
    → handle_member_removed()

"member_list_updated":
    → Se conversazione corrente:
      * Parse members
      * Send UiEvent::MembersLoaded
    → Altrimenti: skip (load quando serve)

"user_left_group":
    → handle_user_left_group()

"user_deleted_account":
    → handle_user_deleted_account()

"conversation_created_complete":
    → handle_conversation_created_complete()

"invitation_received":
    → handle_invitation_received()

Altri eventi: log unknown
```

#### handle_new_message()
```
1. Estrai message_data (nested o flat)
2. Parse MessageDto
3. Send UiEvent::WsIncoming per processing normale
```

**ROUTING**: Reinvia come WsIncoming per processing standard

#### handle_member_added()
```
1. Estrai user_id (con parsing robusto)
2. Estrai username
3. Estrai joined_at timestamp
4. Se members_list esiste per conversazione:
   - Check duplicati → skip se esiste
   - Crea ParticipantInfo con joined_at
   - Append a lista
   - Sort con owner primo, poi alfabetico
5. Add system message "X è stato aggiunto al gruppo"
```

**SORT LOGIC**: Owner sempre primo (se valido), poi alfabetico

#### handle_member_removed()
```
1. Estrai kicked_user_id
2. Estrai kicked_username
3. Remove da members_list (se presente)
4. Add system message "X è stato espulso dal gruppo"
```

#### handle_user_left_group()
```
1. Estrai user_id
2. Estrai username
3. Remove da members_list (se presente)
4. Add system message "X ha abbandonato il gruppo"
```

#### handle_user_deleted_account()
```
1. Estrai deleted_user_id e username
2. Rimuovi messaggi utente da cache:
   - conversation_messages.retain(author_id != deleted)
   - Count removed
3. Se conversazione corrente:
   - state.messages.retain(author_id != deleted)
   - Count removed
4. Remove da members_list
5. Add system message "X ha eliminato il proprio account"
```

**CLEANUP COMPLETO**: Rimuove messaggi e member info

#### handle_conversation_created_complete()
```
1. Parse ConversationDto
2. Parse Vec<MessageDto>
3. Send UiEvent::ConversationConfirmed con full data
```

#### handle_invitation_received()
```
1. Estrai sender_username
2. Estrai conversation_id
3. Fetch conversation via API
4. Se fetch OK:
   - Send UiEvent::ConversationSummaryFetched
5. Add system message "Sei stato invitato da X al gruppo Y"
```

### 7. BufferHandler

**File**: `buffer_handler.rs`

**Dominio**: Gestione buffer di riordinamento per messaggi e eventi out-of-order

**Metodi Pubblici**:

#### try_deliver_buffered_messages()
```
1. Get buffer per conversation_id
2. Get current_confirmed per conversation
3. current_expected = confirmed + 1
4. Loop consecutivo:
   - Get message con seq == current_expected
   - Add a messages_to_deliver
   - Mark seq per removal
   - Increment current_expected
5. Remove messaggi deliverizzati
6. Cleanup obsoleti (seq <= confirmed):
   - Filter keys <= confirmed
   - Remove da buffer
   - Log cleanup
7. Remove buffer se vuoto
8. Return messages_to_deliver
```

**CLEANUP AUTOMATICO**: Rimuove messaggi obsoleti (già confermati) dal buffer

#### try_deliver_buffered_user_events()
```
1. Get current_confirmed = user_sequence_confirmed
2. current_expected = confirmed + 1
3. Loop consecutivo:
   - Get events con seq == current_expected
   - Extend events_to_deliver
   - Mark seq per removal
   - Increment current_expected
4. Remove eventi deliverizzati
5. Cleanup obsoleti (seq <= confirmed):
   - Filter keys <= confirmed
   - Remove da buffer
   - Log cleanup
6. Return events_to_deliver
```

#### buffer_message_for_reorder()
```
1. Get sequence_num da messaggio
2. Calculate expected = confirmed + 1
3. Se seq > expected:
   - Log buffering
   - Insert in message_reorder_buffer[cid][seq]
```

#### buffer_user_event_for_reorder()
```
1. Calculate expected = user_sequence_confirmed + 1
2. Se seq > expected:
   - Log buffering
   - Append to user_event_reorder_buffer[seq]
```

#### get_next_buffered_message()
```
1. Get buffer per conversation_id
2. Se messaggio con expected_seq esiste:
   - Remove from buffer
   - Log retrieval
   - Se buffer vuoto: remove dalla map
   - Return Some(msg)
3. Altrimenti: Return None
```

**USO**: Chiamato in loop da WebSocketHandler per svuotare buffer

### 8. Utils Module

**File**: `utils.rs`

**Funzioni**:

#### move_conversation_to_top()
```
1. Find posizione conversazione in list
2. Se pos > 0:
   - Remove da posizione corrente
   - Insert(0) in cima
   - Log move
```

**USO**: Chiamato da dispatcher dopo processing WsIncoming

### 9. Helpers Module

**File**: `helpers.rs`

**Funzioni**:

#### add_system_message()
```
1. Crea MessageDto::system_message(content)
2. Append a state.messages
3. Se conversazione corrente esiste:
   - Append a conversation_messages cache
4. Log add
```

#### add_system_message_to_conversation()
```
1. Crea MessageDto::system_message(content)
2. Set conversation_id specifico
3. Se cache esiste:
   - Append messaggio
4. Altrimenti:
   - Create cache entry con messaggio
5. Se conversazione corrente match:
   - Append a state.messages
6. Log add
```

#### validate_incoming_message()
```
Validazioni:
1. content non vuoto
2. author_username non vuoto (se non system)
3. content.len() <= 50000

Return false se invalido + log warning
```

### 10. WebSocketHandler - Helper Methods

**File**: `websocket_handler.rs` (metodi privati)

#### update_message_cache()

Aggiorna la cache dei messaggi per una conversazione con inserimento ordinato e gestione duplicati.

**Signature**: `fn update_message_cache(state: &mut AppState, msg: &MessageDto) -> bool`

**Logica**:
```
1. Get or create conversation_messages[conversation_id]

2. Check duplicati:
   - Scan cache per msg.id esistente
   - Se duplicato: return false (skip)
   
3. Calcola insert position (binary search):
   - Se msg ha sequence_num:
     * Binary search per sequence_num (primary key)
     * Fallback su created_at + id (tie-breaker)
   - Altrimenti:
     * Binary search per created_at + id
     
4. Insert messaggio in posizione calcolata

5. Log insertion (chars, author, seq, position)

6. Se msg ha sequence_num:
   - Verify cache sequence integrity
   
7. Auto mark_read check:
   - Chiama check_and_send_auto_mark_read(msg)
   
8. Return true (messaggio aggiunto)
```

**Note**:
- Usa binary_search per O(log n) insertion
- Mantiene ordinamento per sequence o timestamp
- Integrato con auto mark_read system

#### check_and_send_auto_mark_read()

Invia automaticamente mark_read per messaggi in conversazione attiva.

**Signature**: `fn check_and_send_auto_mark_read(state: &mut AppState, msg: &MessageDto)`

**Condizioni** (tutte devono essere vere):
```
1. msg.conversation_id == state.cid (conversazione corrente)
2. msg.author_id != user_id (NON è messaggio dell'utente)
3. msg.sequence_num.is_some() (ha sequenza)
```

**Azioni** (se condizioni soddisfatte):
```
1. Invia Outgoing::MarkRead:
   - conversation_id = msg.conversation_id
   - sequence_num = msg.sequence_num
   
2. Se invio OK:
   - Aggiorna conv.last_read_sequence localmente
   - Azzera conversation_unread_counts[conversation_id]
   - Log auto-mark_read
   
3. Se invio FAIL:
   - Warning log
```

**Note**:
- Chiamato da update_message_cache() per OGNI messaggio aggiunto
- Garantisce mark_read immediato per messaggi in conversazione attiva
- Gestisce fallimenti gracefully (solo log warning)

#### update_ui_messages()

Aggiunge un messaggio alla UI della conversazione corrente.

**Signature**: `fn update_ui_messages(state: &mut AppState, msg: MessageDto)`

**Logica**:
```
1. Check duplicati in state.messages:
   - Se esiste: skip (return)
   
2. Calcola UI insert position (binary search):
   - Per sequence_num o created_at + id
   
3. Insert messaggio in state.messages

4. Log UI update
```

**Note**:
- Separato da update_message_cache (cache vs UI)
- Chiamato solo per messaggi in conversazione attiva

#### verify_cache_sequence_integrity()

Verifica l'integrità delle sequenze nella cache (debug/diagnostics).

**Signature**: `fn verify_cache_sequence_integrity(state: &AppState, conversation_id: Uuid)`

**Logica**:
```
1. Get cache per conversation_id
2. Extract tutti i sequence_num (skip None)
3. Verifica consecutività:
   - Check gaps tra sequenze
   - Log warning se non consecutivi
4. Diagnostics output
```

**Note**:
- Chiamato dopo ogni inserimento con sequence
- Utile per debugging gap issues


## Event Flow Examples

### Login Flow
```
1. User → LoginStarted
   → AuthHandler::handle_login_started()
   → LoginState = LoggingIn

2. API Response → Logged(token, user_id, seq)
   → AuthHandler::handle_logged()
   → Save token, user_id
   → Generate new session_id
   → Init sequences = seq
   → request_ws_reconnect = true
   → Page = Conversations

3. WebSocket → WsConnected
   → WebSocketHandler::handle()
   → ws_status = Connected
   → reset_sequence_system()

4. Server → InitialStateReceived
   → ConversationHandler::handle_initial_state_received()
   → Load conversations
   → Init conversation_sequences
   → Load members
   → Calculate unread counts
```

### Message Send Flow
```
1. User → Send message
   → Create optimistic MessageDto
   → client_msg_id = UUID v4
   → is_confirmed = None
   → Add to state.messages (immediate UI)
   → Add to pending_confirmations
   → Send Outgoing::SendMessage

2. Server → MessageConfirmation
   → MessageHandler::handle_message_confirmation()
   → Find by client_msg_id in pending
   → Update in-place:
     * id = server_msg_id
     * sequence_num = sequence
     * is_confirmed = Some(true)
   → Update conversation_sequence
   → Update ConversationDto metadata
   → Sort cache per sequence

3. WebSocket → Broadcast to others
   → WsIncoming(msg)
   → WebSocketHandler::handle_incoming_message()
   → Normal processing
```

### Gap Detection & Recovery Flow
```
1. Server → WsIncoming(seq=105)
   → Current confirmed = 100
   → Expected = 101
   → Gap detected! (missing 4)
   
2. WebSocketHandler:
   → update_conversation_sequence(105) - without confirm
   → buffer_message_for_reorder(msg)
   → EXIT (no UI update)

3. SequenceHandler:
   → Gap size = 4 < 5
   → Log "small gap, handle at next ping"
   → Do NOT trigger immediate resume

4. Next Ping Response → gaps_detected=true
   → request_messages_resume(from_seq=100)

5. Server → MessagesResume([101,102,103,104])
   → handle_messages_resume()
   → Process missing messages
   → Add to cache + UI
   → Update sequences

6. BufferHandler:
   → try_deliver_buffered_messages()
   → Now can deliver 105 (consecutive)
   → update_conversation_sequence(105) - with confirm
   → Add to cache + UI
```

### Conversation Fetch On-Demand Flow
```
1. Server → WsIncoming(msg) for unknown conv
   → Dispatcher checks: conversation NOT exists
   → Mark fetching_conversations.insert(conv_id)
   → FORCE-BUFFER msg (bypass gap check)
   → Spawn fetch task

2. API → get_conversation(conv_id)
   → Fetch ConversationDto + members + messages

3. Task Complete → ConversationSummaryFetched
   → Dispatcher processes:
     * Try_deliver_buffered_messages()
     * Process any buffered msgs
     * Update conversation_sequences
     * Insert conversation in list
     * Sort list by last_activity
     * Remove from fetching_conversations

4. Buffered messages delivered:
   → Normal WebSocket processing
   → Add to cache + UI
```

## Sequence System Design

### Dual-Sequence Tracking

**Per User Events**:
```
user_sequence_confirmed: Ultimo evento consecutivo ricevuto
user_sequence_received: Ultimo evento ricevuto (con gap)
user_sequence_shared: Arc<AtomicU64> per ping task
```

**Per Conversation Messages**:
```
conversation_sequences[cid]: Ultimo messaggio ricevuto (con gap)
conversation_sequences_confirmed[cid]: Ultimo messaggio consecutivo
```

### Gap Detection Thresholds

**User Events**:
- Gap < 3 → Wait for ping
- Gap >= 3 → Immediate resume

**Messages**:
- Gap < 5 → Wait for ping
- Gap >= 5 → Immediate resume

### Health Calculation

```rust
fn get_sequence_health(state: &AppState) -> f64 {
    if state.sequence_stats.ping_count == 0 {
        return 1.0;
    }
    
    let pong_rate = pong_count as f64 / ping_count as f64;
    let gap_penalty = min(gaps_detected as f64 * 0.1, 0.5);
    let missed_penalty = (missed_pings as f64 / max_missed_pings as f64) * 0.3;
    
    let health = (pong_rate - gap_penalty - missed_penalty).max(0.0);
    health.clamp(0.0, 1.0)
}
```

**Components**:
- `pong_rate`: Percentuale pong ricevuti vs ping inviati (baseline)
- `gap_penalty`: Max 50% penalty per gap frequenti (0.1 per gap, capped)
- `missed_penalty`: Max 30% penalty per ping mancati consecutivi

**Interpretation**:
- 1.0: Perfect sync (no gaps, all pongs)
- >0.9: Excellent (pochi gap, buona connessione)
- 0.7-0.9: Good (alcuni gap ma gestiti)
- 0.5-0.7: Fair (molti gap o missed pings, warning)
- <0.5: Poor (problemi seri, alert)

## Auto Mark Read System

### Trigger Points

**1. WebSocket Incoming Message**:
```
WebSocketHandler::handle_incoming_message()
    → update_message_cache()
    → check_and_send_auto_mark_read() per OGNI messaggio
```

**2. Buffered Message Delivery**:
```
WebSocketHandler::handle_incoming_message()
    → Loop su buffered messages
    → update_message_cache()
    → check_and_send_auto_mark_read() per OGNI buffered
```

**3. Conversation Open**:
```
ConversationHandler::handle_opened()
    → Se cached messages presenti
    → Se max_seq > last_read_sequence
    → Send mark_read per max_seq
```

**4. Resume Complete**:
```
SequenceHandler::handle_user_events_resume()
    → Auto mark_read per conversazione corrente

SequenceHandler::handle_messages_resume()
    → Auto mark_read per conversazione corrente
```

### Conditions

Auto mark_read viene inviato SE (via check_and_send_auto_mark_read):
1. Messaggio appartiene a `cid` (conversazione corrente)
2. Messaggio NON è dall'utente corrente
3. Messaggio ha `sequence_num`

### Implementation Details

La funzione `check_and_send_auto_mark_read()` in WebSocketHandler:
- Verifica le 3 condizioni sopra
- Invia Outgoing::MarkRead con conversation_id e sequence_num
- Aggiorna localmente conv.last_read_sequence
- Azzera conversation_unread_counts[conversation_id]
- Log success/failure

## Buffer Management

### Message Reorder Buffer

**Struttura**: `HashMap<Uuid, BTreeMap<u64, MessageDto>>`
- Key outer: conversation_id
- Key inner: sequence_num
- Value: MessageDto

**Operations**:
- `buffer_message_for_reorder()` → Insert se seq > expected
- `try_deliver_buffered_messages()` → Deliver consecutivi
- `get_next_buffered_message()` → Single retrieval
- Auto cleanup obsoleti (seq <= confirmed)
- Auto remove empty buffers

### User Event Reorder Buffer

**Struttura**: `BTreeMap<u64, Vec<serde_json::Value>>`
- Key: sequence_num
- Value: Lista eventi con quella seq (possibili duplicati)

**Operations**:
- `buffer_user_event_for_reorder()` → Append se seq > expected
- `try_deliver_buffered_user_events()` → Deliver consecutivi
- Auto cleanup obsoleti (seq <= confirmed)

### Cleanup Strategy

**Obsolete Messages**:
- Messaggi con seq <= confirmed
- Non verranno mai deliverizzati (già confermati)
- Rimossi durante try_deliver call

**Empty Buffers**:
- Rimossi immediatamente dopo ultimo delivery
- Previene memory leaks per conversazioni inattive



## Conclusioni

La business logic del client Rust Ruggine implementa:

**✅ Architettura**:
- Event-driven con dispatcher centrale e handler specializzati
- Routing esplicito tramite match exhaustive
- State mutations isolate negli handler
- Waker automatico per UI updates

**✅ Sincronizzazione**:
- Gap detection automatico con thresholds configurabili
- Recovery automatico tramite resume requests
- Buffer management per riordinamento out-of-order
- Health monitoring per diagnostics

**✅ Resilienza**:
- Optimistic UI updates per responsiveness
- Automatic retry e recovery da fallimenti
- Duplicate prevention a tutti i livelli
- Graceful degradation su errori

**✅ Performance**:
- Binary search per insert ordinati (O(log n))
- Buffer cleanup automatico (prevent memory leaks)
- Lazy loading conversazioni (fetch on-demand)
- Direct WebSocket channel per ping (no rate limit)

**✅ Usabilità**:
- Auto mark_read quando chat aperta
- Auto mark_read per messaggi real-time
- Unread counts automatici
- Member sorting intelligente (owner first)
- System messages per eventi importanti

**✅ Manutenibilità**:
- Handler isolati e testabili
- Event log per debugging
- Metrics e stats complete
- Documentation allineata al codice

Il design permette:
- Facile estensione (nuovi handler + eventi)
- Testing granulare (handlers pure functions)
- Debugging efficiente (event tracing)
- State consistency garantita
- Resilienza a network issues
