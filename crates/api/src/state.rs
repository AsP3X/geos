//! Shared application state for handlers.

use geos_core::config::Config;
use geos_core::db::PgPool;
use geos_core::meili::MeiliClient;
use geos_core::storage::StorageClient;

use crate::routes::tiles::TileFlight;
use crate::stream::EventStreamHub;

/// Dependencies injected into Axum handlers via [`axum::Extension`].
#[derive(Clone)]
pub struct AppState {
    /// Validated process configuration.
    pub config: Config,
    /// Postgres connection pool.
    pub pool: PgPool,
    /// Meilisearch client for search indexing and queries.
    pub meili: MeiliClient,
    /// Live event notification fan-out.
    pub stream: EventStreamHub,
    /// Shared HTTP client for outbound upstream fetches (e.g. imagery tiles).
    pub http: reqwest::Client,
    /// nebular-os object-storage client (tile cache backend).
    pub storage: StorageClient,
    /// Per-key single-flight gate for the caching tile proxy.
    pub tile_flight: TileFlight,
}
