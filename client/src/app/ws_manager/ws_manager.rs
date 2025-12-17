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
        //self.manage_ping_cycle(state);

        // 3. Health check periodico
        self.health_monitor.perform_health_check(state, &self.connection_manager);

        // 4. Gestisce connessione
        self.connection_manager.manage_connection(state);

        (state.egui_waker)();
        
    }

}