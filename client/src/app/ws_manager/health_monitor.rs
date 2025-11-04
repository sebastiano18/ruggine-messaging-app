use crate::state::AppState;
use crate::models::WsStatus;
use std::time::{Duration, Instant};
use tracing::{info, warn, debug};
use crate::app::events::sequence_handler::SequenceHandler;
use super::connection_manager::ConnectionManager;

pub struct HealthMonitor {
    last_health_check: Instant,
    health_check_interval: Duration,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            last_health_check: Instant::now(),
            health_check_interval: Duration::from_secs(60),
        }
    }

    /// Esegue health check periodico del sistema
    pub fn perform_health_check(&mut self, state: &AppState, connection_manager: &ConnectionManager) {
        if self.last_health_check.elapsed() < self.health_check_interval {
            return;
        }

        self.last_health_check = Instant::now();

        let health = SequenceHandler::get_sequence_health(state);
        let total_messages = state.get_total_cached_messages();
        let ws_connected = state.ws_status == WsStatus::Connected;

        info!(
            "Health check - WS: {}, Seq Health: {:.2}, Cached Msgs: {}, DM Stubs: {}, Missed Pings: {}/{}",
            ws_connected, health, total_messages, state.dm_stubs.len(),
            state.missed_pings, state.max_missed_pings
        );

        // Alert per problemi di salute
        if health < 0.7 && ws_connected {
            warn!("Poor sequence health detected: {:.2}", health);
            self.log_sequence_issues(state);
        }

        if total_messages > 10000 {
            warn!("High memory usage: {} cached messages", total_messages);
            self.log_memory_stats(state);
        }

        // Log statistiche connessione
        debug!("Connection stats: {:?}", connection_manager.get_stats());

        // Check per problemi specifici
        self.check_sequence_issues(state);
        self.check_memory_issues(state);
    }

    /// Logga dettagli sui problemi di sequenza
    fn log_sequence_issues(&self, state: &AppState) {
        let stats = &state.sequence_stats;
        warn!(
            "Sequence issues - Pings: {}, Pongs: {}, Gaps: {}, Avg Gap Size: {:.1}",
            stats.ping_count, stats.pong_count, stats.gaps_detected, stats.average_gap_size
        );

        if let Some(last_gap) = stats.last_gap_time {
            let gap_age = last_gap.elapsed();
            warn!("Last gap was {:?} ago", gap_age);
        }
    }

    /// Logga statistiche di memoria
    fn log_memory_stats(&self, state: &AppState) {
        let conversation_count = state.conversations.as_ref().map_or(0, |c| c.len());
        let cached_conversations = state.conversation_messages.len();
        let dm_stubs = state.dm_stubs.len();

        warn!(
            "Memory stats - Conversations: {}, Cached: {}, DM Stubs: {}",
            conversation_count, cached_conversations, dm_stubs
        );
    }

    /// Controlla problemi specifici delle sequenze
    fn check_sequence_issues(&self, state: &AppState) {
        // Troppi ping senza pong
        if state.sequence_stats.ping_count > 0 {
            let pong_rate = state.sequence_stats.pong_count as f64 / state.sequence_stats.ping_count as f64;
            if pong_rate < 0.5 {
                warn!(
                    "Low pong response rate: {:.1}% ({} pongs / {} pings)",
                    pong_rate * 100.0, state.sequence_stats.pong_count, state.sequence_stats.ping_count
                );
            }
        }

        // Gap frequenti
        if state.sequence_stats.gaps_detected > 5 {
            warn!(
                "Frequent sequence gaps: {} gaps detected, {} events recovered",
                state.sequence_stats.gaps_detected, state.sequence_stats.events_recovered
            );
        }

        // Ping mancanti consecutivi
        if state.missed_pings > state.max_missed_pings / 2 {
            warn!(
                "High consecutive missed pings: {}/{} (threshold warning)",
                state.missed_pings, state.max_missed_pings
            );
        }
    }

    /// Controlla problemi di memoria
    fn check_memory_issues(&self, state: &AppState) {
        // Troppi messaggi cached
        let total_messages = state.get_total_cached_messages();
        if total_messages > 50000 {
            warn!(
                "Very high message cache: {} messages (consider cleanup)",
                total_messages
            );
        }

        // Troppi stub DM orfani
        if state.dm_stubs.len() > 20 {
            warn!(
                "High number of DM stubs: {} (possible memory leak)",
                state.dm_stubs.len()
            );
        }

        // Conversazioni cached vs conversazioni attive
        let active_conversations = state.conversations.as_ref().map_or(0, |c| c.len());
        let cached_conversations = state.conversation_messages.len();

        if cached_conversations > active_conversations * 2 {
            warn!(
                "Cache bloat detected: {} cached vs {} active conversations",
                cached_conversations, active_conversations
            );
        }
    }

    /// Restituisce un rapporto di salute completo
    pub fn get_health_report(&self, state: &AppState, connection_manager: &ConnectionManager) -> HealthReport {
        let sequence_health = SequenceHandler::get_sequence_health(state);
        let memory_usage = state.get_total_cached_messages();
        let connection_stats = connection_manager.get_stats();

        HealthReport {
            sequence_health,
            memory_usage,
            connection_success_rate: connection_stats.connection_success_rate(),
            missed_pings: state.missed_pings,
            max_missed_pings: state.max_missed_pings,
            gaps_detected: state.sequence_stats.gaps_detected,
            dm_stubs_count: state.dm_stubs.len(),
            cached_conversations: state.conversation_messages.len(),
            active_conversations: state.conversations.as_ref().map_or(0, |c| c.len()),
        }
    }
}

#[derive(Debug)]
pub struct HealthReport {
    pub sequence_health: f64,
    pub memory_usage: usize,
    pub connection_success_rate: f64,
    pub missed_pings: u32,
    pub max_missed_pings: u32,
    pub gaps_detected: u32,
    pub dm_stubs_count: usize,
    pub cached_conversations: usize,
    pub active_conversations: usize,
}

impl HealthReport {
    /// Valuta se il sistema è in buona salute
    pub fn is_healthy(&self) -> bool {
        self.sequence_health > 0.8 &&
            self.memory_usage < 10000 &&
            self.connection_success_rate > 0.7 &&
            self.missed_pings < self.max_missed_pings / 2
    }

    /// Restituisce una valutazione testuale dello stato
    pub fn status_text(&self) -> &'static str {
        if self.is_healthy() {
            "Healthy"
        } else if self.sequence_health < 0.5 || self.missed_pings >= self.max_missed_pings {
            "Critical"
        } else {
            "Warning"
        }
    }
}