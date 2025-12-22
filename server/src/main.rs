use axum::{Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::time::{interval, Duration};  
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod config;
mod controllers;
mod cpu_logger;
mod db;
mod error;
mod models;
mod repositories;
mod routers;
mod services;
mod state;
mod web_socket;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    //Setup logging
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

    // CLEANUP TASK: Elimina eventi vecchi ogni 24 ore
    {
        let cleanup_pool = state.pool.clone();
        tokio::spawn(async move {
            // Cleanup iniziale al startup
            tracing::info!("Running initial user_events cleanup...");
            match cleanup_old_user_events(&cleanup_pool, 30).await {
                Ok(deleted) => {
                    if deleted > 0 {
                        tracing::info!("Initial cleanup: removed {} old user events", deleted);
                    }
                }
                Err(e) => tracing::warn!("Initial cleanup failed: {}", e),
            }

            // Cleanup periodico ogni 24 ore
            let mut interval = interval(Duration::from_secs(24 * 60 * 60));
            interval.tick().await; // Salta il primo tick (già fatto cleanup iniziale)

            loop {
                interval.tick().await;

                tracing::info!("Running scheduled user_events cleanup...");
                match cleanup_old_user_events(&cleanup_pool, 30).await {
                    Ok(deleted) => {
                        if deleted > 0 {
                            tracing::info!("Cleaned up {} old user events (>30 days)", deleted);
                        } else {
                            tracing::info!("No old user events to clean");
                        }
                    }
                    Err(e) => tracing::error!("Scheduled cleanup failed: {}", e),
                }
            }
        });

        tracing::info!("User events cleanup task started (runs every 24h, keeps last 30 days)");
    }

    cpu_logger::spawn_cpu_logger();

    // Costruisci i router parziali
    let api_router = Router::new()
        .merge(routers::user_route::router())
        .merge(routers::conversation_route::router())
        .merge(routers::invite_route::router())
        .merge(routers::message_route::router());

    // Combina i router
    let app = Router::new()
        .nest("/api", api_router)
        .route("/ws", axum::routing::get(web_socket::ws_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let addr: SocketAddr = cfg.bind.parse()?;
    let listener = TcpListener::bind(addr).await?;
    println!("Server in ascolto su {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}


/// Elimina eventi user più vecchi di `retention_days` giorni
async fn cleanup_old_user_events(
    pool: &sqlx::SqlitePool,
    retention_days: i64,
) -> Result<u64, sqlx::Error> {
    let cutoff_timestamp = chrono::Utc::now().timestamp() - (retention_days * 24 * 60 * 60);

    let result = sqlx::query("DELETE FROM user_events WHERE created_at < ?")
        .bind(cutoff_timestamp)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}