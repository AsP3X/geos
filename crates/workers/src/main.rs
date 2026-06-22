//! Entry point for the Geos ingestion workers.
//!
//! Scaffold only: initializes structured logging and reports readiness. The
//! `Connector` trait, USGS connector, normalizer, enrichment, correlation
//! engine, and schedulers are added by the `workers` work in the plan.

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
        "geos-workers scaffold starting"
    );
}
