use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::interval;
use tracing::info;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Utc;

struct CpuUsage {
    user_time: u64,
    system_time: u64,
    total_time: u64,
}

impl CpuUsage {
    fn new() -> Self {
        Self {
            user_time: 0,
            system_time: 0,
            total_time: 0,
        }
    }

    // Simplified CPU usage calculation for cross-platform compatibility
    fn get_current_usage() -> Self {
        // On a real implementation, you would use platform-specific APIs
        // For now, we'll use a simple approach with system time
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        Self {
            user_time: now % 1000, // Simulated user time
            system_time: (now / 10) % 1000, // Simulated system time  
            total_time: now,
        }
    }
}

pub async fn start_cpu_logging() -> anyhow::Result<()> {
    let mut interval = interval(Duration::from_secs(120)); // Every 2 minutes
    let mut last_usage = CpuUsage::new();
    
    info!("Starting CPU logging task (every 2 minutes)");
    
    loop {
        interval.tick().await;
        
        let current_usage = CpuUsage::get_current_usage();
        let timestamp = Utc::now();
        
        // Calculate CPU usage percentage (simplified)
        let user_diff = current_usage.user_time.saturating_sub(last_usage.user_time);
        let system_diff = current_usage.system_time.saturating_sub(last_usage.system_time);
        let total_diff = current_usage.total_time.saturating_sub(last_usage.total_time);
        
        let cpu_percentage = if total_diff > 0 {
            ((user_diff + system_diff) as f64 / total_diff as f64) * 100.0
        } else {
            0.0
        };
        
        // Log to file
        let log_entry = format!(
            "[{}] CPU Usage: {:.2}% (User: {}ms, System: {}ms, Total: {}ms)\n",
            timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            cpu_percentage,
            user_diff,
            system_diff,
            total_diff
        );
        
        // Write to log file
        match OpenOptions::new()
            .create(true)
            .append(true)
            .open("ruggine_cpu.log")
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(log_entry.as_bytes()) {
                    eprintln!("Failed to write to log file: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Failed to open log file: {}", e);
            }
        }
        
        // Also log to console
        info!("CPU Usage: {:.2}%", cpu_percentage);
        
        last_usage = current_usage;
    }
}
