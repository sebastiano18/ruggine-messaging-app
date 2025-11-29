use sysinfo::{System, Process}; 
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;
use chrono::Local;
use tokio;
use sysinfo::Pid;

pub fn spawn_cpu_logger() {
    tokio::spawn(async move {
        let mut sys = System::new_all();
        let pid: Pid = sysinfo::get_current_pid().unwrap();

        loop {
            // Aggiorna tutti i dati
            sys.refresh_all();

            if let Some(proc) = sys.process(pid) {
                let cpu_percent = proc.cpu_usage() / sys.cpus().len() as f32;
                let memory_mb = proc.memory() as f64 / (1024.0 * 1024.0);

                // Scrive sul file di log
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("server_cpu.log")
                    .unwrap();

                writeln!(
                    file,
                    "[{}] Server CPU (Tempo/Uso): {:.2}% | Dimensione App (Memoria): {:.2} MB",
                    Local::now().format("%Y-%m-%d %H:%M:%S"),
                    cpu_percent,
                    memory_mb
                ).unwrap();
            }

            // Aspetta 2 minuti prima della prossima scrittura
            tokio::time::sleep(Duration::from_secs(120)).await;
        }
    });
}
