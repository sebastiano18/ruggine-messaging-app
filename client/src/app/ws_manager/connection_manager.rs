use crate::state::AppState;
use crate::models::{UiEvent, WsStatus};
use std::time::{Duration, Instant};
use tracing::{info, error, debug};

#[derive(Debug, Default)]
pub struct ConnectionStats {
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub connection_attempts: u32,
    pub successful_connections: u32,
    pub disconnections: u32,
    pub last_error: Option<String>,
    pub uptime_start: Option<Instant>,
}

pub struct ConnectionManager {
    stats: ConnectionStats,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            stats: ConnectionStats::default(),
        }
    }

    /// Gestisce lo stato della connessione WebSocket
    pub fn manage_connection(&mut self, state: &mut AppState) {
        // Gestisce stato di autenticazione
        if state.token.is_none() {
            if state.ws_status != WsStatus::Disconnected {
                info!("No token available, disconnecting WebSocket");
                self.disconnect_websocket(state);
            }
            return;
        }

        // Gestisce richieste di riconnessione
        if state.request_ws_reconnect && state.ws_status != WsStatus::Connecting {
            info!("Reconnection requested");
            state.request_ws_reconnect = false;
            self.disconnect_websocket(state);
        }

        // Avvia connessione se necessario
        match state.ws_status {
            WsStatus::Disconnected => {
                if state.is_authenticated() {
                    self.start_websocket_connection(state);
                }
            }
            WsStatus::Connecting => {
                // Timeout check per connessione
                if state.last_ping_time.elapsed() > Duration::from_secs(30) {
                    error!("Connection timeout, retrying");
                    self.disconnect_websocket(state);
                    self.stats.last_error = Some("Connection timeout".into());
                }
            }
            WsStatus::Connected => {
                // Monitoraggio connessione attiva
                self.monitor_active_connection(state);
            }
        }
    }

    /// Monitoraggio connessione attiva
    fn monitor_active_connection(&mut self, state: &mut AppState) {
        // Check per connessione "zombie"
        if state.sequence_stats.pong_count == 0 &&
            state.sequence_stats.ping_count > 5 {
            tracing::warn!("Connection appears to be zombie (no pongs received)");
            state.request_ws_reconnect = true;
        }

        // Check per gap di sequenza eccessivi
        if let Some(last_gap_time) = state.sequence_stats.last_gap_time {
            if last_gap_time.elapsed() < Duration::from_secs(5) &&
                state.sequence_stats.average_gap_size > 50.0 {
                tracing::warn!("Frequent large sequence gaps, connection may be unstable");
            }
        }
    }

    /// Avvia connessione WebSocket
    fn start_websocket_connection(&mut self, state: &mut AppState) {
        let base = state.base.clone();
        let token = match state.token.clone() {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                error!("Cannot start WebSocket: invalid token");
                self.stats.last_error = Some("Invalid token".into());
                let _ = state.ui_tx.send(UiEvent::WsError("Token non valido".into()));
                return;
            }
        };

        let tx = state.ui_tx.clone();
        state.ws_status = WsStatus::Connecting;
        state.last_ping_time = Instant::now();

        self.stats.connection_attempts += 1;
        info!("Starting WebSocket connection attempt #{} to {}",
              self.stats.connection_attempts, base);

        state.rt.spawn(async move {
            match crate::api::ws::connect(&base, &token).await {
                Ok(mut ws) => {
                    debug!("WebSocket connected successfully");

                    match crate::api::ws::subscribe(&mut ws).await {
                        Ok(_) => {
                            info!("WebSocket subscribed successfully");
                            let _ = tx.send(UiEvent::WsConnected);
                            let tx_reader = tx.clone();

                            let ctrl = crate::api::ws::spawn_bidirectional_handler(ws, move |msg| {
                                super::message_handlers::handle_websocket_message(&tx_reader, msg);
                            });

                            let _ = tx.send(UiEvent::WsControlReady(ctrl));
                        }
                        Err(e) => {
                            error!("WebSocket subscribe failed: {}", e);
                            let _ = tx.send(UiEvent::WsError(format!("Sottoscrizione fallita: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket connection failed: {}", e);
                    let _ = tx.send(UiEvent::WsError(format!("Connessione fallita: {}", e)));
                }
            }
        });
    }

    /// Disconnette WebSocket pulendo lo stato
    fn disconnect_websocket(&mut self, state: &mut AppState) {
        info!("Disconnecting WebSocket");

        state.ws_status = WsStatus::Disconnected;

        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

        state.reset_sequence_system();
        self.stats.disconnections += 1;

        if let Some(uptime_start) = self.stats.uptime_start.take() {
            let uptime = uptime_start.elapsed();
            info!("WebSocket uptime was: {:?}", uptime);
        }
    }

    pub fn get_stats(&self) -> &ConnectionStats {
        &self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = ConnectionStats::default();
    }

    pub fn mark_message_sent(&mut self) {
        self.stats.total_messages_sent += 1;
    }

    pub fn mark_message_received(&mut self) {
        self.stats.total_messages_received += 1;
    }

    pub fn mark_send_error(&mut self, error: String) {
        self.stats.last_error = Some(error);
    }
}

impl ConnectionStats {
    pub fn connection_success_rate(&self) -> f64 {
        if self.connection_attempts == 0 {
            0.0
        } else {
            self.successful_connections as f64 / self.connection_attempts as f64
        }
    }

    pub fn current_uptime(&self) -> Option<Duration> {
        self.uptime_start.map(|start| start.elapsed())
    }
}