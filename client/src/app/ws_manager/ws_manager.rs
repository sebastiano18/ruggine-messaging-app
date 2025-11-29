use tracing::info;
use crate::app::events::sequence_handler::SequenceHandler;
use crate::app::ws_manager::connection_manager;
use crate::app::ws_manager::connection_manager::ConnectionManager;
use crate::app::ws_manager::health_monitor::HealthMonitor;
use crate::app::ws_manager::message_processor::MessageProcessor;
use crate::app::ws_manager::rate_limiter::RateLimiter;
use crate::models::WsStatus;
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

    // In ws_manager.rs
    fn manage_ping_cycle(&mut self, state: &mut AppState) {
        // DEBUG: Log sempre per capire cosa succede
        tracing::debug!("manage_ping_cycle called - ws_status: {:?}, elapsed: {:?}, interval: {:?}",
                    state.ws_status,
                    state.last_ping_time.elapsed(),
                    state.ping_interval);

        let should_ping = SequenceHandler::should_send_ping(state);
        tracing::debug!("should_send_ping returned: {}", should_ping);

        if !should_ping {
            return;
        }

        tracing::info!("🎯 TRYING TO SEND PING");

        if state.missed_pings > 0 && state.last_ping_time.elapsed() > state.ping_timeout {
            tracing::warn!("Pong timeout detected");
            SequenceHandler::handle_pong_timeout(state);
            return;
        }

        // DEBUG: Verifica ws_ctrl
        if state.ws_ctrl.is_none() {
            tracing::error!("❌ Cannot send ping: ws_ctrl is None");
            return;
        }

        tracing::info!("🔌 ws_ctrl is available");

        // INVIA PING DIRETTAMENTE AL WEBSOCKET
        if let Some(ref ws_ctrl) = state.ws_ctrl {
            let user_seq = state.user_sequence_confirmed;

            tracing::info!("📦 Creating ping JSON with user_seq: {}", user_seq);

            let json_msg = serde_json::json!({
            "type": "ping",
            "timestamp": chrono::Utc::now().timestamp(),
            "user_sequence": user_seq
        });

            match serde_json::to_string(&json_msg) {
                Ok(msg_str) => {
                    tracing::info!("📨 Sending ping: {}", msg_str);

                    match ws_ctrl.outgoing_tx.send(msg_str) {
                        Ok(_) => {
                            state.missed_pings += 1;
                            SequenceHandler::update_ping_time(state);
                            tracing::info!("✅ PING SENT - user_seq: {}, missed_pings: {}",
                                      user_seq, state.missed_pings);
                        }
                        Err(e) => {
                            tracing::error!("❌ Failed to send ping to channel: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("❌ Failed to serialize ping JSON: {}", e);
                }
            }
        }
    }

    pub fn get_connection_stats(&self) -> &connection_manager::ConnectionStats {
        self.connection_manager.get_stats()
    }

    pub fn reset_stats(&mut self) {
        self.connection_manager.reset_stats();
        info!("WebSocket manager stats reset");
    }
}