use axum::{routing::{get, post}, Router};
use std::{net::SocketAddr, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config; mod db; mod models; mod auth; mod api; mod ws; mod cpu_logger; mod error;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Inizializza tracing con timestamp automatico e livelli INFO
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,tower_http=trace".into()),
        ))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let cfg = config::Config::from_env();
    let pool = db::init_pool(&cfg.database_url).await?;
    sqlx::query("PRAGMA foreign_keys = ON;").execute(&pool).await?;

    cpu_logger::spawn_cpu_logger();

    let state = Arc::new(api::AppState::new(pool, cfg.jwt_secret));

    let app = Router::new()
        // auth
        .route("/api/register", post(api::register))
        .route("/api/login", post(api::login))
        // groups
        .route("/api/groups", post(api::create_group))
        .route("/api/groups/invite", post(api::create_invite))
        .route("/api/groups/join", post(api::join_group_by_token))
        // dm & messages
        .route("/api/dm/open", post(api::open_dm))
        .route("/api/conversations/:id/messages", get(api::get_messages).post(api::post_message))
        .route("/api/conversations/:id/read", post(api::post_read))
        // websocket realtime
        .route("/ws", get(ws::ws_handler))
        // Layer che logga automaticamente ogni richiesta/risposta
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = cfg.bind.parse()?;
    tracing::info!("listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}