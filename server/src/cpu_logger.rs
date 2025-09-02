use std::time::Duration;
use sysinfo::{System};
use tracing::info;


pub fn spawn_cpu_logger() {
    tokio::spawn(async move {
        let mut sys = System::new_all();
        let pid = sysinfo::get_current_pid().unwrap();
        loop {
            sys.refresh_process(pid);
            if let Some(p) = sys.process(pid) {
                info!(target: "cpu", usage = %p.cpu_usage(), "server_cpu_usage_pct");
            }
            tokio::time::sleep(Duration::from_secs(120)).await;
        }
    });
}
