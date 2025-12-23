use std::time::{Duration, Instant};

pub struct RateLimiter {
    messages_sent: u32,
    window_start: Instant,
    max_messages_per_window: u32,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            messages_sent: 0,
            window_start: Instant::now(),
            max_messages_per_window: 100,
            window_duration: Duration::from_secs(60),
        }
    }

    /// Verifica se il rate limit è stato superato
    pub fn check_rate_limit(&mut self) -> bool {
        let now = Instant::now();

        // Reset finestra se è trascorso abbastanza tempo
        if now.duration_since(self.window_start) > self.window_duration {
            if self.messages_sent > 0 {
                tracing::debug!("Rate limit window reset - sent {} messages in last window", 
                       self.messages_sent);
            }
            self.messages_sent = 0;
            self.window_start = now;
        }

        self.messages_sent < self.max_messages_per_window
    }

    /// Incrementa il contatore dei messaggi inviati
    pub fn increment_sent(&mut self) {
        self.messages_sent += 1;
    }

}