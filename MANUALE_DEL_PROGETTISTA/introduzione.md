# Rust Ruggine - Applicazione di Messaggistica Real-Time

## Introduzione

**Rust Ruggine** è un'applicazione di messaggistica istantanea sviluppata interamente in Rust, utilizzando un'architettura client-server con comunicazione HTTP REST e WebSocket per la sincronizzazione in tempo reale. Il progetto è stato realizzato come parte del corso di laurea magistrale in Computer Science al Politecnico di Torino.

## Obiettivi del Progetto

- Implementare un sistema di chat sicuro e performante basato su tecnologie Rust moderne
- Garantire **consistenza dei dati** tramite dual-level sequencing (conversazioni e notifiche utente)
- Fornire **affidabilità** attraverso gap detection, automatic recovery e message confirmation tracking
- Offrire un'interfaccia grafica nativa cross-platform con aggiornamenti ottimistici dell'UI
- Esplorare pattern avanzati come broadcast channels, actor model e backoff esponenziale

## Caratteristiche Tecniche Principali

### Backend
- **Framework**: Axum 0.7 con routing async e middleware stack
- **Database**: SQLite con WAL mode, foreign keys, indices ottimizzati
- **Async Runtime**: Tokio 1.x con multi-threaded executor
- **Security**: Argon2id per password hashing, JWT (HS256) per autenticazione
- **Real-time**: WebSocket bidirectional con pattern Actor (3 task concorrenti per connessione)
- **Dual Sequencing**: 
  - **Message sequences** per conversazioni (ordinamento messaggi, mark_read)
  - **User sequences** per eventi personali (confirmations, offline recovery)
- **Protection**: Rate limiting (60 msg/min server-side), client processing limit (200 msg/ciclo), heartbeat timeout (120s)

### Frontend
- **GUI**: egui/eframe (immediate mode) per interfaccia nativa
- **Networking**: reqwest per HTTP REST, tokio-tungstenite per WebSocket
- **State Management**: Single source of truth con `AppState` centralizzato
- **Sync Features**:
  - Gap detection con buffer e riordinamento automatico
  - Optimistic UI con conferme server
  - Automatic reconnection con backoff esponenziale (1s → 15s, forced logout al 6° tentativo)
  - Health monitoring (sequence health score 0.0-1.0)

### Database Schema
- **users**: Utenti con username univoco, password hash Argon2id
- **conversations**: Conversazioni (DM o gruppi) con owner e timestamp
- **participants**: Membri conversazioni con last_read_sequence
- **messages**: Messaggi con sequence_num incrementale per conversazione
- **message_sequences**: Contatore sequenze per conversazione
- **user_events**: Eventi personali con sequence_num incrementale per utente
- **user_sequences**: Contatore sequenze per eventi utente

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
│         │    Single Source of Truth         │               │
│         │         (AppState)                │               │
│         └─────────────────┴─────────────────┘               │
│                           │                                 │
│            Event-Driven Architecture:                       │
│         UI Events → Handlers → State Mutations              │
│                           │                                 │
└───────────────────────────┼─────────────────────────────────┘
                            │
                    ┌───────┴───────┐
                    │   HTTP REST   │ (Login, Register, Fetch)
                    │   WebSocket   │ (Real-time messages)
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
│         │  - User Notification Channels     │               │
│         │  - Conversation Broadcast Channels│               │
│         │  - Database Pool (SQLx)           │               │
│         └───────────────────────────────────┘               │
│                                                              │
│  WebSocket Actor Pattern (per connection):                  │
│    - Reader Task:  client input + rate limiting             │
│    - Writer Task:  client output + heartbeat (30s)          │
│    - Receiver Task: merge broadcast channels                │
└───────────────────────────┼─────────────────────────────────┘
                            │
┌───────────────────────────┼─────────────────────────────────┐
│                    DATABASE (SQLite)                         │
│  ┌──────────────┬──────────────┬──────────────┐             │
│  │    users     │conversations │   messages   │             │
│  ├──────────────┼──────────────┼──────────────┤             │
│  │participants  │user_events   │msg_sequences │             │
│  ├──────────────┼──────────────┴──────────────┤             │
│  │user_sequences│                              │             │
│  └──────────────┴─────────────────────────────┘             │
└─────────────────────────────────────────────────────────────┘
```

### Flusso Dati Chiave

**Invio Messaggio**:
```
Client A: User types → Optimistic UI update
    ↓
WebSocket: ChatMessage with client_msg_id
    ↓
Server Reader: Validate + Rate limit check
    ↓
Handler: INSERT message + sequence_num
    ↓
Dual Broadcast:
  1. User notification channel → Confirmation to Client A
  2. Conversation channel → Message to all participants
    ↓
Client A: Confirma messaggio (client_id → server_id)
Client B: Riceve nuovo messaggio
```

**Gap Detection & Recovery**:
```
Client: Expected seq 42, Received seq 45
    ↓
Gap detected: missing 43, 44
    ↓
Buffer message 45 in reorder_buffer
    ↓
Send RequestMessagesResume(from: 43)
    ↓
Server: Query messages WHERE seq >= 43
    ↓
Client: Receive missing messages, flush buffer
```

## Stack Tecnologico

### Backend Dependencies

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Web Framework | Axum | 0.7 | HTTP routing, middleware, extractors |
| Async Runtime | Tokio | 1.x | Multi-threaded async executor |
| Database | SQLite | 3.x | Embedded relational database |
| Database Driver | SQLx | 0.7 | Compile-time checked SQL queries |
| WebSocket | tokio-tungstenite | 0.24 | WebSocket protocol (RFC 6455) |
| Authentication | jsonwebtoken | 9.x | JWT token generation/validation |
| Password Hashing | argon2 | 0.5 | Secure password hashing (Argon2id) |
| Serialization | serde + serde_json | 1.x | JSON serialization/deserialization |
| Logging | tracing + tracing-subscriber | 0.1 | Structured logging |
| Error Handling | anyhow + thiserror | 1.x | Error propagation and custom errors |
| HTTP Middleware | tower + tower-http | 0.5 | Middleware stack (CORS, logging) |
| UUID Generation | uuid | 1.x | Unique identifier generation |
| Date/Time | chrono | 0.4 | Timestamp handling |
| System Monitoring | sysinfo | latest | CPU/Memory monitoring |

### Frontend Dependencies

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| GUI Framework | eframe + egui | 0.27 | Immediate mode GUI, native rendering |
| HTTP Client | reqwest | 0.12 | REST API communication |
| WebSocket | tokio-tungstenite | 0.23 | Real-time bidirectional communication |
| Async Runtime | tokio | 1.x | Async operations |
| Icons | egui-remixicon | 0.27.2 | Icon library for UI |
| Serialization | serde + serde_json | 1.x | JSON handling |
| Logging | tracing + env_logger | 0.11 | Client-side logging |

## Funzionalità Implementate

### Core Features
- ✅ **Autenticazione**: Registro, login, logout, eliminazione account
- ✅ **Conversazioni**: Creazione DM e gruppi, lista con pagination
- ✅ **Messaggi**: Invio, ricezione, eliminazione con conferme
- ✅ **Real-time**: WebSocket con dual sequencing e gap detection
- ✅ **Multi-device**: Session tracking con forced logout su conflitto
- ✅ **Recovery**: Automatic recovery da gap tramite resume requests
- ✅ **Gruppi**: Invita utenti, rimuovi membri, abbandona gruppo
- ✅ **UI**: Toast notifications, modal dialogs, optimistic updates

### Advanced Features
- ✅ **Rate Limiting**: 60 msg/min per client (sliding window, server-side)
- ✅ **Client Processing Limit**: Max 200 msg processati per ciclo (anti-loop client)
- ✅ **Heartbeat**: Server → Client (30s + jitter), Client timeout (120s)
- ✅ **Health Monitoring**: Sequence health (0.0-1.0), memory alerts, cache bloat detection
- ✅ **Backoff Esponenziale**: 1s → 2s → 5s → 10s → 15s, forced logout al 6° tentativo
- ✅ **Batch Operations**: Batch insert user_events per gruppi ≥10 utenti
- ✅ **Zombie Detection**: >5 ping senza pong → reconnect
- ✅ **CPU Monitoring**: Background logging ogni 120s in `server_cpu.log` (CPU % normalizzato per core, memoria RSS in MB)

### Security Features
- ✅ JWT authentication con session tracking
- ✅ Password hashing con Argon2id
- ✅ Input validation (max 10KB per messaggio, max 100KB per frame WebSocket)
- ✅ Rate limiting per prevenire abusi
- ✅ Forced logout su multi-device conflict
- ✅ No information leakage in error messages

## Metriche di Performance

### Tipiche (Client)
- **Memory Usage**: ~500KB-1MB base
- **Cached Messages**: ~1000 messaggi = 500KB
- **Health Alerts**: >10k messaggi cached (warning), >50k (critical)
- **Connection Uptime**: Tracciato con `ConnectionStats`

### Tipiche (Server)
- **CPU Usage**: 
  - Idle: ~0.01%
  - Normal load: ~1-2%
  - Startup: 3-12%
- **Memory Usage**:
  - Baseline: ~35 MB
  - Peak: ~95 MB (caricamento dati)
- **Database**: SQLite con WAL mode, query ottimizzate con JOIN (100-400ms → 10-50ms)

### Ottimizzazioni Implementate
- Initial state query con JOIN invece di 6 subquery
- Batch insert user_events per ≥10 utenti
- Broadcast channels invece di iterazione manuale
- Message reorder buffer con BTreeMap
- Cleanup automatico stub timeout (30s)
- Lazy loading messaggi con pagination

## Testing e Debug

### Logging System
- **Levels**: `error!`, `warn!`, `info!`, `debug!`
- **Key Points**: Connection lifecycle, message flow, sequence tracking, health monitoring
- **Example**: `RUST_LOG=debug cargo run` per debug dettagliato

### Health Monitoring
- Sequence health score: 0.0-1.0 (warning <0.7)
- Cached messages count (alert >10k)
- DM stubs count (warning >20)
- Pong response rate (warning <50%)
- Sequence gaps count (warning >5)
- Cache bloat: cached > active * 2

### Common Issues
- **Zombie connection**: >5 ping senza pong → auto-reconnect
- **Too many failures**: ≥6 tentativi consecutivi → forced logout
- **Rate limit exceeded**: >60 msg/min → requeue automatico
- **Sequence gap**: Fuori ordine → buffer + resume request
- **High memory**: >10k messages → warning log

## Struttura Documentazione

Questa introduzione fa parte di un set completo di documenti tecnici:

- **api-layer.md**: Endpoint HTTP REST, autenticazione, validazione
- **websocket_manager_architecture.md**: Architettura client completa, WebSocket manager, handlers
- **business_logic_layer.md**: Services e logica di business (da creare)
- **state_management.md**: AppState, canali comunicazione, sequence tracking
- **ui_layer.md**: Componenti UI, rendering, interazioni utente
- **websocket_architecture.md**: Actor pattern, broadcast channels, dual sequencing
- **websocket_handlers.md**: Handler specifici per messaggi WebSocket
- **http_architecture.md**: Controller HTTP, routing, middleware
- **services_and_repositories.md**: Services layer e data access layer
- **benchmarks.md**: Performance testing e ottimizzazioni
- **cpu_logger.md**: Sistema di monitoraggio risorse server

## Conclusioni

Rust Ruggine dimostra come sia possibile costruire un'applicazione di messaggistica real-time sicura e performante utilizzando l'ecosistema Rust. L'architettura implementa pattern avanzati (Actor model, broadcast channels, dual sequencing) garantendo affidabilità attraverso gap detection, automatic recovery e health monitoring. Il progetto rappresenta un'implementazione completa e production-ready di un sistema di chat moderno.

