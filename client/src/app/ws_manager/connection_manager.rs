use crate::models::{UiEvent, WsStatus};
use crate::state::AppState;
use std::time::{Duration, Instant};
use tracing::{debug, error, info};

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
    consecutive_failures: u32,
    last_attempt: Option<Instant>,
    next_retry_delay: Duration,
    last_ws_status: WsStatus,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            stats: ConnectionStats::default(),
            consecutive_failures: 0,
            last_attempt: None,
            next_retry_delay: Duration::from_secs(0),
            last_ws_status: WsStatus::Disconnected,
        }
    }

    /// Gestisce lo stato della connessione WebSocket
    pub fn manage_connection(&mut self, state: &mut AppState) {
        let current_status = state.ws_status.clone();

        if current_status != self.last_ws_status {
            match (&self.last_ws_status, &current_status) {
                (_, WsStatus::Connected) => {
                    self.reset_backoff();
                }
                (WsStatus::Connecting, WsStatus::Disconnected) => {
                    let should_logout = self.increment_backoff();
                    if should_logout {
                        tracing::error!("Too many connection failures, forcing logout");
                        let _ = state.ui_tx.send(UiEvent::Error(
                            "Impossibile connettersi al server. Effettua nuovamente il login."
                                .into(),
                        ));
                        let _ = state.ui_tx.send(UiEvent::LoggedOut);

                        // CRITICAL: Reset backoff COMPLETO dopo il logout per evitare loop
                        self.consecutive_failures = 0;
                        self.next_retry_delay = Duration::from_secs(0);
                        self.last_attempt = None; // ← Aggiungi questo
                        self.last_ws_status = current_status;
                        return;
                    }
                }
                _ => {}
            }
            self.last_ws_status = current_status;
        }

        // Gestisce stato di autenticazione
        if state.token.is_none() {
            if state.ws_status != WsStatus::Disconnected {
                info!("No token available, disconnecting WebSocket");
                self.disconnect_websocket(state);
            }
            // CRITICAL: Reset backoff COMPLETO quando non c'è token (dopo logout)
            if self.consecutive_failures > 0 {
                self.consecutive_failures = 0;
                self.next_retry_delay = Duration::from_secs(0);
                self.last_attempt = None; // ← Aggiungi questo
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
                    // Backoff esponenziale: controlla se è il momento di riprovare
                    if let Some(last_attempt) = self.last_attempt {
                        let elapsed = last_attempt.elapsed();
                        if elapsed < self.next_retry_delay {
                            // Troppo presto, aspetta ancora
                            return;
                        }
                    }

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
        if state.sequence_stats.pong_count == 0 && state.sequence_stats.ping_count > 5 {
            tracing::warn!("Connection appears to be zombie (no pongs received)");
            state.request_ws_reconnect = true;
        }

        // Check per gap di sequenza eccessivi
        if let Some(last_gap_time) = state.sequence_stats.last_gap_time {
            if last_gap_time.elapsed() < Duration::from_secs(5)
                && state.sequence_stats.average_gap_size > 50.0
            {
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
                let _ = state
                    .ui_tx
                    .send(UiEvent::WsError("Token non valido".into()));
                return;
            }
        };

        let tx = state.ui_tx.clone();
        state.ws_status = WsStatus::Connecting;
        state.last_ping_time = Instant::now();

        // Salva timestamp del tentativo
        self.last_attempt = Some(Instant::now());

        self.stats.connection_attempts += 1;
        if self.consecutive_failures > 0 {
            info!("Starting WebSocket connection attempt #{} to {} (retry after {} failures, next retry in {:?})",
                  self.stats.connection_attempts, base, self.consecutive_failures, self.next_retry_delay);
        } else {
            info!(
                "Starting WebSocket connection attempt #{} to {}",
                self.stats.connection_attempts, base
            );
        }

        state.rt.spawn(async move {
            match crate::api::ws::connect(&base, &token).await {
                Ok(mut ws) => {
                    debug!("WebSocket connected successfully");

                    match crate::api::ws::subscribe(&mut ws).await {
                        Ok(_) => {
                            info!("WebSocket subscribed successfully");

                            // Connessione riuscita - invia evento per resettare backoff
                            let _ = tx.send(UiEvent::WsConnected);
                            let tx_reader = tx.clone();
                            let tx_disconnect = tx.clone();

                            let ctrl = crate::api::ws::spawn_bidirectional_handler(
                                ws,
                                move |msg| {
                                    super::message_handlers::handle_websocket_message(
                                        &tx_reader, msg,
                                    );
                                },
                                Some(tx_disconnect),
                            );

                            let _ = tx.send(UiEvent::WsControlReady(ctrl));
                        }
                        Err(e) => {
                            error!("WebSocket subscribe failed: {}", e);
                            let _ =
                                tx.send(UiEvent::WsError(format!("Sottoscrizione fallita: {}", e)));
                            let _ = tx.send(UiEvent::WsDisconnected);
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket connection failed: {}", e);
                    let _ = tx.send(UiEvent::WsError(format!("Connessione fallita: {}", e)));
                    // Importante: torna a Disconnected per permettere il prossimo tentativo
                    let _ = tx.send(UiEvent::WsDisconnected);
                }
            }
        });
    }

    /// Disconnette WebSocket pulendo lo stato
    fn disconnect_websocket(&mut self, state: &mut AppState) {
        info!("Disconnecting WebSocket");

        state.ws_status = WsStatus::Disconnected;

        // Invia evento di disconnessione all'UI
        let _ = state.ui_tx.send(UiEvent::WsDisconnected);

        if let Some(ctrl) = state.ws_ctrl.take() {
            let _ = ctrl.shutdown.send(());
        }

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

    /// Incrementa il backoff dopo un fallimento
    /// Ritorna true se deve eseguire il logout (troppi fallimenti)
    pub fn increment_backoff(&mut self) -> bool {
        self.consecutive_failures += 1;

        // Dopo 6 tentativi falliti, forza il logout
        if self.consecutive_failures >= 6 {
            tracing::error!(
                "Connection failed {} times, forcing logout",
                self.consecutive_failures
            );
            return true; // Segnala che deve fare logout
        }

        // Backoff esponenziale con cap
        self.next_retry_delay = match self.consecutive_failures {
            1 => Duration::from_secs(1),  // Primo retry: 1 secondo
            2 => Duration::from_secs(2),  // Secondo: 2 secondi
            3 => Duration::from_secs(5),  // Terzo: 5 secondi
            4 => Duration::from_secs(10), // Quarto: 10 secondi
            5 => Duration::from_secs(15), // Quinto: 15 secondi (ultimo prima del logout)
            _ => Duration::from_secs(30), // Non dovrebbe arrivare qui
        };

        if self.consecutive_failures >= 3 {
            tracing::warn!(
                "Multiple connection failures ({}), next retry in {:?}",
                self.consecutive_failures,
                self.next_retry_delay
            );
        }

        false // Continua a riprovare
    }

    /// Resetta il backoff dopo una connessione riuscita
    pub fn reset_backoff(&mut self) {
        if self.consecutive_failures > 0 {
            info!(
                "Connection successful after {} failures, resetting backoff",
                self.consecutive_failures
            );
        }
        self.consecutive_failures = 0;
        self.next_retry_delay = Duration::from_secs(0);
        self.stats.successful_connections += 1;
        self.stats.uptime_start = Some(Instant::now());
    }

    /// Ottiene informazioni sul prossimo retry
    pub fn get_retry_info(&self) -> Option<(u32, Duration)> {
        if self.consecutive_failures > 0 {
            let remaining = if let Some(last) = self.last_attempt {
                self.next_retry_delay.saturating_sub(last.elapsed())
            } else {
                Duration::from_secs(0)
            };
            Some((self.consecutive_failures, remaining))
        } else {
            None
        }
    }

    /// Gestisce evento di connessione riuscita
    pub fn handle_connected(&mut self) {
        self.reset_backoff();
    }

    /// Gestisce evento di disconnessione/errore
    /// Ritorna true se deve eseguire il logout
    pub fn handle_disconnected(&mut self) -> bool {
        self.increment_backoff()
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
