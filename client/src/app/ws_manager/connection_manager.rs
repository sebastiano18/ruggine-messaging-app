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
    is_connecting_in_progress: bool,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            stats: ConnectionStats::default(),
            consecutive_failures: 0,
            last_attempt: None,
            next_retry_delay: Duration::from_secs(0),
            last_ws_status: WsStatus::Disconnected,
            is_connecting_in_progress: false,
        }
    }

    /// Gestisce lo stato della connessione WebSocket
    pub fn manage_connection(&mut self, state: &mut AppState) {
        let current_status = state.ws_status.clone();

        if current_status != self.last_ws_status {
            // Reset del flag quando cambia lo stato
            match &current_status {
                WsStatus::Connected | WsStatus::Disconnected => {
                    self.is_connecting_in_progress = false;
                }
                _ => {}
            }

            match (&self.last_ws_status, &current_status) {
                (_, WsStatus::Connected) => {
                    self.reset_backoff();
                    // Clear connection timeout timer - connection successful
                    state.connection_attempt_start = None;
                    debug!("Connection established, cleared connection timeout timer");
                }
                (WsStatus::Connecting, WsStatus::Disconnected) => {
                    let should_logout = self.increment_backoff();
                    if should_logout {
                        tracing::error!("Too many connection failures, forcing logout");
                        let _ = state.ui_tx.send(UiEvent::Error(
                            crate::models::ErrorType::Auth(
                                "Impossibile connettersi al server. Effettua nuovamente il login.".to_string()
                            )
                        ));
                        let _ = state.ui_tx.send(UiEvent::LoggedOut);

                        // CRITICAL: Reset backoff COMPLETO dopo il logout per evitare loop
                        self.consecutive_failures = 0;
                        self.next_retry_delay = Duration::from_secs(0);
                        self.last_attempt = None;
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
                self.disconnect_websocket(state, true); // Notifica UI (logout reale)
            }
            // CRITICAL: Reset backoff COMPLETO quando non c'è token (dopo logout)
            if self.consecutive_failures > 0 {
                self.consecutive_failures = 0;
                self.next_retry_delay = Duration::from_secs(0);
                self.last_attempt = None;
            }
            return;
        }

        // FIX: Reset la flag IMMEDIATAMENTE per prevenire race condition
        if state.request_ws_reconnect {
            info!("Reconnection requested");
            state.request_ws_reconnect = false;

            // Solo se NON stiamo già connettendo, disconnetti e riconnetti
            if state.ws_status != WsStatus::Connecting {
                // FIX: Disconnessione silenziosa per reconnect interni
                // Non notifica l'UI per evitare flash "disconnected" durante login
                self.disconnect_websocket(state, false);
            } else {
                debug!("Reconnection already in progress, ignoring duplicate request");
            }
        }

        // Avvia connessione se necessario
        match state.ws_status {
            WsStatus::Disconnected => {
                // CHECK CRITICO: Previene doppie connessioni
                if state.is_authenticated() && !self.is_connecting_in_progress {
                    // Backoff esponenziale: controlla se è il momento di riprovare
                    if let Some(last_attempt) = self.last_attempt {
                        let elapsed = last_attempt.elapsed();
                        if elapsed < self.next_retry_delay {
                            return;
                        }
                    }

                    self.start_websocket_connection(state);
                }
            }
            WsStatus::Connecting => {
                // Timeout check per connessione usando timer dedicato
                if let Some(start) = state.connection_attempt_start {
                    if start.elapsed() > Duration::from_secs(30) {
                        error!("Connection timeout, retrying");
                        self.disconnect_websocket(state, true); // Notifica UI (errore reale)
                        self.stats.last_error = Some("Connection timeout".into());
                    }
                }
            }
            WsStatus::Connected => {
                self.monitor_active_connection(state);
            }
        }
        (state.egui_waker)();
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
        // LOCK IMMEDIATO: Previene altre chiamate mentre questa è in corso
        if self.is_connecting_in_progress {
            debug!("Connection already in progress, skipping duplicate attempt");
            return;
        }

        self.is_connecting_in_progress = true;

        let base = state.base.clone();
        let token = match state.token.clone() {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                error!("Cannot start WebSocket: invalid token");
                self.stats.last_error = Some("Invalid token".into());
                self.is_connecting_in_progress = false; // Reset on error
                let _ = state
                    .ui_tx
                    .send(UiEvent::WsError("Token non valido".into()));
                return;
            }
        };

        let tx = state.ui_tx.clone();
        let session_id = state.current_session_id; // Passa session_id corrente
        let user_seq_shared = state.user_sequence_shared.clone(); // Clone PRIMA dell'async block
        let waker = state.egui_waker.clone(); // ✅ NUOVO: Clone waker per svegliare egui
        state.ws_status = WsStatus::Connecting;
        state.connection_attempt_start = Some(Instant::now()); // Start connection timeout timer

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
            match crate::api::ws::connect(&base, &token, session_id).await {
                Ok(mut ws) => {
                    debug!("WebSocket connected successfully");

                    match crate::api::ws::subscribe(&mut ws).await {
                        Ok(_) => {
                            info!("WebSocket subscribed successfully");

                            let _ = tx.send(UiEvent::WsConnected);
                            waker(); // ✅ Sveglia egui per mostrare stato connesso
                            let tx_reader = tx.clone();
                            let tx_disconnect = tx.clone();
                            let waker_clone = waker.clone(); // ✅ Clone per il callback

                            let ctrl = crate::api::ws::spawn_bidirectional_handler(
                                ws,
                                move |msg| {
                                    super::message_handlers::handle_websocket_message(
                                        &tx_reader, msg,
                                    );
                                    waker_clone(); // ✅ SVEGLIA EGUI dopo ogni messaggio!
                                },
                                Some(tx_disconnect),
                                user_seq_shared,
                            );

                            let _ = tx.send(UiEvent::WsControlReady(ctrl));
                            waker(); // ✅ Sveglia egui per processare WsControlReady
                        }
                        Err(e) => {
                            error!("WebSocket subscribe failed: {}", e);
                            let _ =
                                tx.send(UiEvent::WsError(format!("Sottoscrizione fallita: {}", e)));
                            let _ = tx.send(UiEvent::WsDisconnected);
                            waker(); // ✅ Sveglia egui per mostrare errore subscribe
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket connection failed: {}", e);
                    let _ = tx.send(UiEvent::WsError(format!("Connessione fallita: {}", e)));
                    let _ = tx.send(UiEvent::WsDisconnected);
                    waker(); // ✅ Sveglia egui per mostrare errore connessione
                }
            }
        });
    }

    /// Disconnette WebSocket pulendo lo stato
    ///
    /// # Arguments
    /// * `state` - Lo stato dell'applicazione
    /// * `notify_ui` - Se true, invia evento WsDisconnected all'UI.
    ///                 Usare false per disconnessioni interne (es. reconnect durante login)
    ///                 per evitare flash momentanei di "disconnected" nell'interfaccia.
    fn disconnect_websocket(&mut self, state: &mut AppState, notify_ui: bool) {
        if notify_ui {
            info!("Disconnecting WebSocket (notifying UI)");
        } else {
            debug!("Disconnecting WebSocket silently for reconnect");
        }

        self.is_connecting_in_progress = false;
        state.ws_status = WsStatus::Disconnected;

        // Invia evento solo se richiesto (non per reconnect interni)
        if notify_ui {
            let _ = state.ui_tx.send(UiEvent::WsDisconnected);
            (state.egui_waker)(); // ✅ Sveglia egui per mostrare disconnessione
        }

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

        if self.consecutive_failures >= 6 {
            tracing::error!(
                "Connection failed {} times, forcing logout",
                self.consecutive_failures
            );
            return true;
        }

        self.next_retry_delay = match self.consecutive_failures {
            1 => Duration::from_secs(1),
            2 => Duration::from_secs(2),
            3 => Duration::from_secs(5),
            4 => Duration::from_secs(10),
            5 => Duration::from_secs(15),
            _ => Duration::from_secs(30),
        };

        if self.consecutive_failures >= 3 {
            tracing::warn!(
                "Multiple connection failures ({}), next retry in {:?}",
                self.consecutive_failures,
                self.next_retry_delay
            );
        }

        false
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