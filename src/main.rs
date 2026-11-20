use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use whatsapp_agent_hub::{
    build_router,
    config::{AppConfig, AppState},
    db,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("whatsapp_agent_hub=info,tower_http=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env();
    let port = config.port;

    tracing::info!(
        database_url = %config.database_url,
        port = port,
        "Starting WhatsApp Agent Hub (Multi-Tenant SaaS)"
    );

    let pool = db::create_pool(&config.database_url).await?;
    db::init_db(&pool).await?;

    let state = AppState::new(pool, config)?;
    let app = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(address = %addr, "HTTP server listening");

    axum::serve(listener, app).await?;
    Ok(())
}
