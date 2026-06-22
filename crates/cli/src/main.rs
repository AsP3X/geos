//! Entry point for the Geos CLI (`geos-cli`).
//!
//! Scaffold only: initializes structured logging and reports readiness. The
//! ratatui interactive TUI (lazygit/btop-style) is added by the `cli` work in
//! the implementation plan.

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tracing::info!(version = geos_core::VERSION, "geos-cli scaffold starting");
}
