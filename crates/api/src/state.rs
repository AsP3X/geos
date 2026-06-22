//! Shared application state for handlers.

use geos_core::config::Config;
use geos_core::db::PgPool;

/// Dependencies injected into Axum handlers via [`axum::Extension`].
#[derive(Clone)]
pub struct AppState {
    /// Validated process configuration.
    pub config: Config,
    /// Postgres connection pool.
    pub pool: PgPool,
}
