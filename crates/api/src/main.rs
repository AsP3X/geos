//! Entry point for the Geos API service.
//!
//! Scaffold only: initializes structured logging and reports readiness. The
//! Axum router, auth, tenant scoping, and endpoints are added by the `api`
//! work in the implementation plan.

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tracing::info!(version = geos_core::VERSION, "geos-api scaffold starting");
}
