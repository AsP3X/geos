//! Connector ingestion status, including historical backfill progress.
//!
//! Public source feeds (USGS, NWS) ingest into the shared system tenant, so the
//! progress they report is global ingestion status rather than tenant-private
//! data. Any authenticated member with `events.read` may view how much history
//! has been pulled and how much remains.

use axum::{extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use geos_core::db::{list_connector_states, ConnectorState};
use geos_core::rbac::Permission;
use geos_core::tenancy::SYSTEM_TENANT_ID;
use serde::Serialize;

use crate::error::ApiError;
use crate::middleware::auth::AuthContext;
use crate::state::AppState;

/// Response wrapper for `GET /api/v1/connectors`.
#[derive(Debug, Serialize)]
pub struct ConnectorStatusResponse {
    /// Status for every registered connector, ordered by source key.
    pub connectors: Vec<ConnectorStatus>,
}

/// Live + backfill status for a single connector.
#[derive(Debug, Serialize)]
pub struct ConnectorStatus {
    /// Stable source key (e.g. `"usgs"`).
    pub source_key: String,
    /// Human-readable source name.
    pub source_name: String,
    /// Whether the source is enabled for ingestion.
    pub enabled: bool,
    /// Timestamp of the last successful live poll, if any.
    pub last_live_run_at: Option<DateTime<Utc>>,
    /// Historical backfill progress.
    pub backfill: BackfillStatus,
}

/// Backfill progress: how much history has been pulled vs. how much remains.
#[derive(Debug, Serialize)]
pub struct BackfillStatus {
    /// Whether a target window has been set (backfill has started at least once).
    pub initialized: bool,
    /// Whether backfill has fully completed.
    pub complete: bool,
    /// Fraction of the backfill completed, in `[0.0, 1.0]`.
    ///
    /// Count-based (`events_ingested / total_estimate`) when the source reported
    /// a total, otherwise time coverage (the cursor's position across the window).
    pub fraction: f64,
    /// Convenience integer percentage (0–100) derived from `fraction`.
    pub percent_complete: u8,
    /// Count of events upserted by the backfill pipeline so far.
    pub events_ingested: i64,
    /// Running total of events across the window (`None` when unknown). While
    /// `counting` is true this is a partial sum, not yet a valid denominator.
    pub total_estimate: Option<i64>,
    /// Whether `total_estimate` is the final catalog total (counting finished),
    /// so `fraction` is count-based. When false, `fraction` is time coverage.
    pub total_final: bool,
    /// Whether the total denominator is still being counted in the background.
    pub counting: bool,
    /// Earliest target time for backfill (the oldest event we intend to pull).
    pub window_start: Option<DateTime<Utc>>,
    /// Latest target time for backfill (set when backfill first starts).
    pub window_end: Option<DateTime<Utc>>,
    /// Oldest `occurred_at` the backfill has reached so far.
    pub cursor: Option<DateTime<Utc>>,
    /// When backfill was first initialized.
    pub started_at: Option<DateTime<Utc>>,
    /// When backfill completed, if it has.
    pub completed_at: Option<DateTime<Utc>>,
    /// When this connector's progress row was last updated on the server.
    pub progress_updated_at: DateTime<Utc>,
}

impl From<ConnectorState> for ConnectorStatus {
    fn from(state: ConnectorState) -> Self {
        let fraction = state.backfill_fraction();
        let percent_complete = (fraction * 100.0).round().clamp(0.0, 100.0) as u8;
        let total_final = state.backfill_total_final();
        // Counting is in progress once it has started but not yet finished, and
        // only while backfill is still running.
        let counting = !state.backfill_complete
            && !total_final
            && state.backfill_count_cursor.is_some()
            && match (state.backfill_count_cursor, state.backfill_window_start) {
                (Some(cursor), Some(start)) => cursor > start,
                _ => false,
            };
        Self {
            source_key: state.source_key,
            source_name: state.source_name,
            enabled: state.enabled,
            last_live_run_at: state.last_live_run_at,
            backfill: BackfillStatus {
                initialized: state.backfill_window_end.is_some(),
                complete: state.backfill_complete,
                fraction,
                percent_complete,
                events_ingested: state.backfill_events_ingested,
                total_estimate: state.backfill_total_estimate,
                total_final,
                counting,
                window_start: state.backfill_window_start,
                window_end: state.backfill_window_end,
                cursor: state.last_backfill_cursor,
                started_at: state.backfill_started_at,
                completed_at: state.backfill_completed_at,
                progress_updated_at: state.updated_at,
            },
        }
    }
}

/// `GET /api/v1/connectors` — ingestion + backfill progress for shared feeds.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<ConnectorStatusResponse>, ApiError> {
    auth.require_permission(Permission::EventsRead)?;

    let states = list_connector_states(&state.pool, SYSTEM_TENANT_ID).await?;
    let connectors = states.into_iter().map(ConnectorStatus::from).collect();

    Ok(Json(ConnectorStatusResponse { connectors }))
}
