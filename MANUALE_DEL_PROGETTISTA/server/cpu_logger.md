# CPU Logger Module Documentation

## Panoramica

Modulo Rust per il monitoraggio continuo delle risorse di sistema (CPU e memoria) del processo server. Registra periodicamente metriche di utilizzo in un file di log.

## Dipendenze

```toml
sysinfo = "*"
chrono = "*"
tokio = { version = "*", features = ["time"] }
```

## Funzionalità Principale

### `spawn_cpu_logger()`

Avvia un task asincrono Tokio che monitora le risorse del processo corrente.

**Comportamento:**
- Esegue in background come task separato
- Intervallo di campionamento: 120 secondi (2 minuti)
- Output: file `server_cpu.log` nella directory corrente

## Metriche Raccolte

### CPU Usage
- **Calcolo:** `proc.cpu_usage() / sys.cpus().len()`
- **Unità:** Percentuale (%)
- **Nota:** Normalizzato per il numero di core CPU

### Memory Usage
- **Fonte:** Memoria residente del processo (RSS)
- **Conversione:** Bytes → Megabytes
- **Formula:** `memory_bytes / (1024.0 * 1024.0)`

## Formato Log

```
[YYYY-MM-DD HH:MM:SS] Server CPU (Tempo/Uso): X.XX% | Dimensione App (Memoria): X.XX MB
```

**Esempio:**
```
[2025-12-23 11:34:30] Server CPU (Tempo/Uso): 1.33% | Dimensione App (Memoria): 95.03 MB
```

## Utilizzo

```rust
use cpu_logger::spawn_cpu_logger;

#[tokio::main]
async fn main() {
    // Avvia il logger
    spawn_cpu_logger();
    
    // Il server continua normalmente
    // Il logging avviene in background
}
```

## Dettagli Implementativi

### Inizializzazione
1. Crea istanza `System` con `new_all()`
2. Ottiene PID del processo corrente
3. Entra in loop infinito

### Ciclo di Monitoraggio
1. **Refresh:** `sys.refresh_all()` aggiorna tutti i dati di sistema
2. **Query:** Recupera informazioni del processo tramite PID
3. **Calcolo:** Estrae CPU e memoria, normalizza valori
4. **Log:** Apre file in append mode, scrive entry formattata
5. **Sleep:** Attende 120 secondi prima dell'iterazione successiva

### Gestione File
- **Mode:** Append (`.append(true)`)
- **Creation:** Creato automaticamente se non esistente
- **Path:** `./server_cpu.log` (directory corrente)
- **Locking:** Nessun lock esplicito (operazioni atomiche del filesystem)

## Analisi Performance

Dai log forniti si osservano:

### Pattern di Utilizzo CPU
- **Idle:** ~0.00-0.01% (server in attesa)
- **Startup:** 3.12-11.11% (inizializzazione)
- **Activity:** 1.33% (carico normale)

### Pattern Memoria
- **Range operativo:** 23-95 MB
- **Baseline:** ~35 MB (stato stabile)
- **Peak:** 95 MB (picco attività)

### Osservazioni
- Memoria stabile nel tempo (~35 MB per ore)
- No memory leak evidenti nel periodo 18:47-19:07
- Spike memoria da 23 MB → 95 MB tra 11:32-11:34 potrebbe indicare:
  - Caricamento dati
  - Connessioni multiple
  - Cache allocation

## Note

- **Thread Safety:** Task isolato, no shared state
- **Overhead:** Minimo (2 minuti tra campionamenti)
- **Accuracy:** CPU usage può variare significativamente tra campionamenti
- **Platform:** Dipende da `sysinfo` - verifica compatibilità OS
