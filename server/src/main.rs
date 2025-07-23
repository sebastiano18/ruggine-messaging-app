use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tracing::{info, error};
use tracing_subscriber;

mod server;
mod models;
mod database;
mod auth;
mod websocket;
mod logging;

use server::ChatServer;

#[derive(Parser, Debug)]
#[command(name = "ruggine-server")]
#[command(about = "Ruggine Chat Server")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    address: SocketAddr,
    
    #[arg(short, long, default_value = "ruggine.db")]
    database: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();
    
    info!("Starting Ruggine Chat Server on {}", args.address);
    
    // Initialize database
    let db = database::init_database(&args.database).await?;
    
    // Clean up stale invites
    database::cleanup_stale_invites(&db).await?;
    info!("Cleaned up stale invites");
    
    // Start logging task (every 2 minutes)
    let logging_handle = tokio::spawn(logging::start_cpu_logging());
    
    // Start server
    let server_handle = tokio::spawn(async move {
        let server = ChatServer::new(db);
        server.run(args.address).await
    });
    
    // Wait for both tasks
    tokio::select! {
        result = server_handle => {
            if let Err(e) = result? {
                error!("Server error: {}", e);
            }
        }
        result = logging_handle => {
            if let Err(e) = result? {
                error!("Logging error: {}", e);
            }
        }
    }
    
    Ok(())
}
