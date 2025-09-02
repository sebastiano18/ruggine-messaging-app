use axum::Router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod config;
mod controllers;
mod cpu_logger;
mod db;
mod error;
mod models;
mod repositories;
mod routers; // contiene build_router
mod services;
mod state;
mod ws;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=trace".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = config::Config::from_env();
    let pool = db::init_pool(&cfg.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = state::AppState::new(pool, cfg.jwt_secret.clone());

    // Router principale
    let app: Router<state::AppState> = routers::build_router(state);

    let addr: SocketAddr = cfg.bind.parse()?;
    let listener = TcpListener::bind(addr).await?;
    println!("🚀 Server in ascolto su {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}


