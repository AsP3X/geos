//! Entry point for the Geos API service.
//!
//! Scaffold only: initializes structured logging, reports readiness, then waits
//! for a shutdown signal so the process stays alive (rather than exiting and
//! crash-looping under Docker's restart policy). The Axum router, auth, tenant
//! scoping, and endpoints are added by the `api` work in the implementation plan.

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tracing::info!(
        version = geos_core::VERSION,
        "geos-api scaffold started; awaiting shutdown signal"
    );

    // Human: No server loop exists yet, so we block until SIGINT/SIGTERM to keep
    // the container running instead of exiting and being restarted repeatedly.
    // Agent: AWAITS Ctrl-C or SIGTERM; replaced by the Axum serve loop later.
    wait_for_shutdown().await;

    tracing::info!("geos-api shutting down");
}

// Human: Resolves on the first of Ctrl-C (SIGINT) or SIGTERM (e.g. `docker
// stop`); on signal-registration error we park forever so we never busy-exit.
// Agent: RETURNS on SIGINT|SIGTERM; unix uses SignalKind::terminate; non-unix waits on ctrl_c only.
async fn wait_for_shutdown() {
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
