#[cfg(feature = "dev-seed")]
use std::env;

use learner_progress_service::{AppState, build_router, config::Config, db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "learner_progress_service=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    tracing::info!(bind_addr = %config.bind_addr, "starting learner-progress-service");

    let database = db::connect(config.database_url.expose()).await?;
    db::migrate(&database).await?;
    #[cfg(feature = "dev-seed")]
    if env::var("SEED_DEV_DATA").as_deref() == Ok("1") {
        db::seed_dev_data(&database).await?;
        tracing::info!("seeded dev data");
    }

    let bind_addr = config.bind_addr.clone();
    let state = AppState::new(database, config);
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(addr = %bind_addr, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
