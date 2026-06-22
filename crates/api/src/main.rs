//! Entry point for the Geos API service.

use std::net::SocketAddr;

use geos_api::stream::{run_event_listener, EventStreamHub};
use geos_api::{build_router, AppState};
use geos_core::config::Config;
use geos_core::db::{connect_pool, run_migrations};
use geos_core::meili::{self, MeiliClient};

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        tracing::error!(error = %err, "geos-api failed to start");
        std::process::exit(1);
    }
}

async fn run() -> geos_core::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let config = Config::from_env()?;
    let pool = connect_pool(&config.database_url).await?;
    run_migrations(&pool).await?;

    let meili = MeiliClient::new(&config.meili_url, &config.meili_master_key)?;
    meili::ensure_events_index(meili.client()).await?;

    let stream = EventStreamHub::default();
    let listener_url = config.database_url.clone();
    let listener_tx = stream.publisher();
    tokio::spawn(async move {
        run_event_listener(&listener_url, listener_tx).await;
    });

    let state = AppState {
        config: config.clone(),
        pool,
        meili,
        stream,
    };

    let app = build_router(state);
    let addr: SocketAddr = config
        .bind_addr
        .parse()
        .map_err(|err| geos_core::AppError::Config(format!("invalid GEOS_BIND_ADDR: {err}")))?;

    tracing::info!(%addr, version = geos_core::VERSION, "geos-api listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| geos_core::AppError::internal(format!("bind failed: {err}")))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| geos_core::AppError::internal(format!("server error: {err}")))?;

    tracing::info!("geos-api shutting down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
