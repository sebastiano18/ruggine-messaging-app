use tracing::info;
use crate::app::ws_manager::connection_manager;
use crate::app::ws_manager::connection_manager::ConnectionManager;
use crate::app::ws_manager::health_monitor::HealthMonitor;
use crate::app::ws_manager::message_processor::MessageProcessor;
use crate::app::ws_manager::rate_limiter::RateLimiter;
use crate::state::AppState;

pub struct WebSocketManager {
    connection_manager: ConnectionManager,
    message_processor: MessageProcessor,
    health_monitor: HealthMonitor,
    rate_limiter: RateLimiter,
}

impl WebSocketManager {
    pub fn new() -> Self {
        Self {
            connection_manager: ConnectionManager::new(),
            message_processor: MessageProcessor::new(),
            health_monitor: HealthMonitor::new(),
            rate_limiter: RateLimiter::new(),
        }
    }

    /// Punto di ingresso principale per la gestione del ciclo di vita WebSocket
    pub fn ensure_ws_lifecycle(&mut self, state: &mut AppState) {
        // 1. Processa messaggi in uscita
        self.message_processor.process_outgoing_messages(state, &mut self.rate_limiter);

        // 2. Gestisce ping cycle per sincronizzazione
        self.manage_ping_cycle(state);

        // 3. Health check periodico
        self.health_monitor.perform_health_check(state, &self.connection_manager);

        // 4. Gestisce connessione
        self.connection_manager.manage_connection(state);
    }

    /// Gestione ping cycle per sincronizzazione sequenze
    fn manage_ping_cycle(&mut self, state: &mut AppState) {
        if !state.should_send_ping() {
            return;
        }

        // Check per timeout pong precedenti
        if state.missed_pings > 0 && state.last_ping_time.elapsed() > state.ping_timeout {
            state.handle_pong_timeout();
            return;
        }
        
        state.send_ping();
        state.update_ping_time();

        tracing::debug!("Ping sent - user_seq: {}", 
               state.user_sequence_confirmed);
    }

    pub fn get_connection_stats(&self) -> &connection_manager::ConnectionStats {
        self.connection_manager.get_stats()
    }

    pub fn reset_stats(&mut self) {
        self.connection_manager.reset_stats();
        info!("WebSocket manager stats reset");
    }
}