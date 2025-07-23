# Ruggine - Multi-platform Chat Application

![Ruggine Logo](https://img.shields.io/badge/Ruggine-Chat%20App-blue?style=for-the-badge&logo=rust)

**Ruggine** è un'applicazione di chat multi-piattaforma sviluppata in Rust (backend) e React (frontend) per il corso di Programmazione di Sistema dell'Università Politecnica di Torino.

## 🚀 Caratteristiche Principali

- **Architettura Client/Server**: Backend asincrono in Rust con WebSocket, frontend React
- **Multi-piattaforma**: Funziona su Windows, Linux, macOS, Android, ChromeOS, iOS (tramite browser)
- **Chat di gruppo**: Creazione e gestione di gruppi di chat
- **Sistema di autenticazione**: Registrazione e login con password crittografate
- **Messaggi in tempo reale**: Comunicazione bidirezionale tramite WebSocket
- **Logging avanzato**: Monitoraggio CPU ogni 2 minuti
- **Interfaccia moderna**: UI responsiva con Bootstrap e React

## 🏗️ Architettura

```
Ruggine/
├── server/          # Backend Rust
│   ├── src/
│   │   ├── main.rs
│   │   ├── server.rs
│   │   ├── models.rs
│   │   ├── database.rs
│   │   ├── auth.rs
│   │   ├── websocket.rs
│   │   └── logging.rs
│   └── Cargo.toml
├── client/          # Frontend React
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── services/
│   │   └── main.jsx
│   └── package.json
└── shared/          # Tipi condivisi (futuro)
```

## 🛠️ Tecnologie Utilizzate

### Backend (Rust)
- **tokio**: Runtime asincrono
- **tokio-tungstenite**: WebSocket server
- **sqlx**: Database SQLite asincrono
- **serde**: Serializzazione JSON
- **bcrypt**: Crittografia password
- **uuid**: Generazione ID univoci
- **chrono**: Gestione date/ore
- **tracing**: Logging strutturato

### Frontend (React)
- **React 19**: Framework UI
- **Vite**: Build tool veloce
- **Bootstrap 5**: Styling e componenti
- **Bootstrap Icons**: Icone
- **dayjs**: Gestione date
- **react-router**: Navigazione

## 🚀 Installazione e Avvio

### Prerequisiti
- **Rust** (latest stable)
- **Node.js** (v18+) e npm
- **Git**

### 1. Clone del Repository
```powershell
git clone https://github.com/PdS2425-C2/G39.git
cd G39
```

### 2. Avvio del Server (Backend)
```powershell
cd server
cargo run
```
Il server sarà disponibile su `ws://localhost:8080`

### 3. Avvio del Client (Frontend)
Apri un nuovo terminale:
```powershell
cd client
npm install
npm run dev
```
Il client sarà disponibile su `http://localhost:3000`

## 📱 Utilizzo

1. **Registrazione**: Crea un nuovo account con username, email e password
2. **Login**: Accedi con le tue credenziali
3. **Crea Gruppo**: Crea un nuovo gruppo di chat (pubblico o privato)
4. **Chat**: Invia e ricevi messaggi in tempo reale
5. **Invita Utenti**: Invita altri utenti nei tuoi gruppi (funzionalità in sviluppo)

## 🔧 Comandi Utili

### Server
```powershell
cd server
cargo build --release    # Build ottimizzata
cargo test               # Esegui test
cargo run -- --help     # Mostra opzioni CLI
```

### Client
```powershell
cd client
npm run build           # Build per produzione
npm run preview         # Preview build di produzione
npm run lint            # Controllo codice
```

## 📊 Logging e Monitoraggio

Il server genera automaticamente:
- **Console logs**: Informazioni in tempo reale
- **ruggine_cpu.log**: Monitoraggio CPU ogni 2 minuti
- **Database**: Persistenza messaggi e utenti

## 🎯 Caratteristiche Avanzate Implementate

- ✅ **Programmazione Asincrona**: Utilizzo di `tokio` per concorrenza
- ✅ **Autenticazione con Password**: Hash bcrypt sicuro
- ✅ **Persistenza Dati**: Database SQLite con `sqlx`
- ✅ **Comunicazione Real-time**: WebSocket bidirezionali
- ✅ **Logging CPU**: Monitoraggio prestazioni automatico
- ✅ **UI Professionale**: Design moderno e responsivo
- ✅ **Gestione Errori**: Error handling robusto
- ✅ **Multi-piattaforma**: Funziona ovunque ci sia un browser

## 🔒 Sicurezza

- **Password Hashing**: bcrypt con salt automatico
- **Validazione Input**: Sanitizzazione lato client e server
- **WebSocket Sicuri**: Gestione connessioni robusta
- **SQL Injection Prevention**: Query parametrizzate con sqlx

## 📈 Prestazioni

- **CPU Usage**: Monitoraggio continuo ogni 2 minuti
- **Memory Efficient**: Uso ottimizzato della memoria con Rust
- **Async I/O**: Non-blocking operations per alta concorrenza
- **Build Size**: 
  - Server: ~15MB (release build)
  - Client: ~2MB (gzipped)

## 🤝 Contributi

Progetto sviluppato dal **Gruppo G39**:
- Implementazione backend Rust
- Sviluppo frontend React
- Integrazione WebSocket
- Testing e debugging

## 📚 Documentazione Aggiuntiva

- [Server API Documentation](./server/README.md)
- [Client Setup Guide](./client/README.md)
- [Deployment Guide](./docs/deployment.md)

## 🐛 Troubleshooting

### Problemi Comuni

1. **Server non si avvia**:
   ```powershell
   # Verifica che la porta 8080 sia libera
   netstat -an | findstr 8080
   ```

2. **Client non si connette**:
   - Verifica che il server sia in esecuzione
   - Controlla la URL del WebSocket in `websocket.js`

3. **Errori di build**:
   ```powershell
   # Client
   cd client && rm -rf node_modules && npm install
   
   # Server
   cd server && cargo clean && cargo build
   ```

## 📄 Licenza

Questo progetto è sviluppato per scopi accademici nell'ambito del corso di Programmazione di Sistema - Politecnico di Torino.

---

*Developed with ❤️ in Rust and React by Team G39*
