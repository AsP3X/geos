//! Connector cursor / backfill progress tracking (`connectors_state` table).
//!
//! Tracks both live polling (`last_live_run_at`) and resumable historical
//! backfill. Backfill walks a source's history newest -> oldest: `last_backfill_cursor`
//! is the oldest `occurred_at` reached so far, and backfill is complete once the
//! cursor reaches `backfill_window_start` (`connector-contract.mdc`).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// A connector's live + backfill state joined with its source catalog entry.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConnectorState {
    /// Owning tenant (system tenant for shared public feeds).
    pub tenant_id: Uuid,
    /// Stable source key (e.g. `"usgs"`).
    pub source_key: String,
    /// Human-readable source name from the `sources` catalog.
    pub source_name: String,
    /// Whether the source is enabled for ingestion.
    pub enabled: bool,
    /// Timestamp of the last successful live poll, if any.
    pub last_live_run_at: Option<DateTime<Utc>>,
    /// Oldest `occurred_at` the backfill has reached (moves toward the window start).
    pub last_backfill_cursor: Option<DateTime<Utc>>,
    /// Whether historical backfill has fully completed.
    pub backfill_complete: bool,
    /// Earliest target time for backfill (the oldest event we intend to pull).
    pub backfill_window_start: Option<DateTime<Utc>>,
    /// Latest target time for backfill (set when backfill first starts).
    pub backfill_window_end: Option<DateTime<Utc>>,
    /// When backfill was first initialized.
    pub backfill_started_at: Option<DateTime<Utc>>,
    /// When backfill completed, if it has.
    pub backfill_completed_at: Option<DateTime<Utc>>,
    /// Count of events upserted by the backfill pipeline so far.
    pub backfill_events_ingested: i64,
    /// Running total of events across the window (`None` = unknown). Built
    /// incrementally; only final once `backfill_count_cursor` reaches the start.
    pub backfill_total_estimate: Option<i64>,
    /// Oldest time the incremental total-count pass has reached (`None` = not
    /// started). The total is final once this is at or before the window start.
    pub backfill_count_cursor: Option<DateTime<Utc>>,
    /// Row last updated (cursor move, event count flush, live poll).
    pub updated_at: DateTime<Utc>,
}

impl ConnectorState {
    /// Whether the total-count denominator is final (counting has finished).
    ///
    /// Counting walks bounded sub-windows newest -> oldest, summing into
    /// `backfill_total_estimate`. The running sum is only a valid denominator
    /// once the count cursor has reached the window start; a `None` cursor with a
    /// known total covers totals set directly (e.g. small/legacy windows).
    #[must_use]
    pub fn backfill_total_final(&self) -> bool {
        match self.backfill_total_estimate {
            Some(total) if total > 0 => {
                match (self.backfill_count_cursor, self.backfill_window_start) {
                    (Some(cursor), Some(start)) => cursor <= start,
                    (None, _) => true,
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Fraction of the backfill already completed, in `[0.0, 1.0]`.
    ///
    /// When the total denominator is final (`backfill_total_final`), progress is
    /// measured by event count (`events_ingested / total`) for an accurate bar.
    /// Otherwise it falls back to time coverage (cursor position across the
    /// window) — including while the total is still being counted.
    #[must_use]
    pub fn backfill_fraction(&self) -> f64 {
        if self.backfill_complete {
            return 1.0;
        }
        if self.backfill_total_final() {
            if let Some(total) = self.backfill_total_estimate {
                let done = self.backfill_events_ingested.max(0) as f64;
                return (done / total as f64).clamp(0.0, 1.0);
            }
        }
        match (self.backfill_window_start, self.backfill_window_end) {
            (Some(start), Some(end)) if end > start => {
                let total = (end - start).num_seconds() as f64;
                let cursor = self.last_backfill_cursor.unwrap_or(end);
                let done = (end - cursor).num_seconds() as f64;
                (done / total).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// Whether backfill has been initialized (a target window is set).
    #[must_use]
    pub fn backfill_initialized(&self) -> bool {
        self.backfill_window_end.is_some()
    }
}

const SELECT_CONNECTOR_STATE: &str = r#"
    SELECT
        cs.tenant_id,
        cs.source_key,
        s.name              AS source_name,
        s.enabled           AS enabled,
        cs.last_live_run_at,
        cs.last_backfill_cursor,
        cs.backfill_complete,
        cs.backfill_window_start,
        cs.backfill_window_end,
        cs.backfill_started_at,
        cs.backfill_completed_at,
        cs.backfill_events_ingested,
        cs.backfill_total_estimate,
        cs.backfill_count_cursor,
        cs.updated_at
    FROM connectors_state cs
    JOIN sources s ON s.key = cs.source_key
"#;

/// Record a successful live poll for `(tenant_id, source_key)`.
pub async fn touch_live_run(pool: &PgPool, tenant_id: Uuid, source_key: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET last_live_run_at = $3, updated_at = $3
        WHERE tenant_id = $1 AND source_key = $2
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch one connector's state, or `None` if not registered for the tenant.
pub async fn get_connector_state(
    pool: &PgPool,
    tenant_id: Uuid,
    source_key: &str,
) -> Result<Option<ConnectorState>> {
    let query = format!("{SELECT_CONNECTOR_STATE} WHERE cs.tenant_id = $1 AND cs.source_key = $2");
    let state = sqlx::query_as::<_, ConnectorState>(&query)
        .bind(tenant_id)
        .bind(source_key)
        .fetch_optional(pool)
        .await?;
    Ok(state)
}

/// List every connector's state for a tenant, ordered by source key.
pub async fn list_connector_states(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<ConnectorState>> {
    let query = format!("{SELECT_CONNECTOR_STATE} WHERE cs.tenant_id = $1 ORDER BY cs.source_key");
    let states = sqlx::query_as::<_, ConnectorState>(&query)
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;
    Ok(states)
}

/// Initialize the backfill target window on first run (no-op once set).
///
/// Sets the window bounds, the start timestamp, and seeds the cursor at the
/// window end. The total denominator is left unknown; it is built afterward by
/// the incremental counting pass (`advance_backfill_count`). Returns the
/// resulting state so callers can resume from the cursor.
pub async fn init_backfill_window(
    pool: &PgPool,
    tenant_id: Uuid,
    source_key: &str,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<Option<ConnectorState>> {
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET backfill_window_start   = $3,
            backfill_window_end     = $4,
            backfill_started_at     = now(),
            last_backfill_cursor    = $4,
            backfill_total_estimate = NULL,
            backfill_count_cursor   = NULL,
            updated_at              = now()
        WHERE tenant_id = $1 AND source_key = $2 AND backfill_window_end IS NULL
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .bind(window_start)
    .bind(window_end)
    .execute(pool)
    .await?;

    get_connector_state(pool, tenant_id, source_key).await
}

/// Advance the backfill cursor toward the window start and add to the count.
///
/// The cursor only ever moves backward (oldest reached), so re-running a chunk
/// is idempotent with respect to progress.
pub async fn advance_backfill_cursor(
    pool: &PgPool,
    tenant_id: Uuid,
    source_key: &str,
    new_cursor: DateTime<Utc>,
    events_ingested: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET last_backfill_cursor     = LEAST(COALESCE(last_backfill_cursor, $3), $3),
            backfill_events_ingested = backfill_events_ingested + $4,
            updated_at               = now()
        WHERE tenant_id = $1 AND source_key = $2
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .bind(new_cursor)
    .bind(events_ingested)
    .execute(pool)
    .await?;
    Ok(())
}

/// Increment the backfill event counter without moving the cursor.
///
/// Used during long-running chunks so the UI can show live progress while a
/// single time window is still being ingested.
pub async fn add_backfill_events_ingested(
    pool: &PgPool,
    tenant_id: Uuid,
    source_key: &str,
    events_ingested: i64,
) -> Result<()> {
    if events_ingested <= 0 {
        return Ok(());
    }
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET backfill_events_ingested = backfill_events_ingested + $3,
            updated_at               = now()
        WHERE tenant_id = $1 AND source_key = $2
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .bind(events_ingested)
    .execute(pool)
    .await?;
    Ok(())
}

/// Re-point the backfill target window start to the configured earliest time.
///
/// Handles both directions: shrinking (configured earliest is more recent) and
/// expanding (configured earliest is older, e.g. switching to the full catalog).
/// Completion is recomputed from the cursor so expanding re-opens an otherwise
/// "complete" backfill. The total denominator and count cursor are reset so the
/// incremental count restarts for the new window. Returns the refreshed state.
/// A no-op when the window already matches.
pub async fn reconcile_backfill_window(
    pool: &PgPool,
    tenant_id: Uuid,
    source_key: &str,
    configured_earliest: DateTime<Utc>,
) -> Result<Option<ConnectorState>> {
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET backfill_window_start   = $3,
            backfill_total_estimate = NULL,
            backfill_count_cursor   = NULL,
            backfill_complete       =
                (last_backfill_cursor IS NOT NULL AND last_backfill_cursor <= $3),
            backfill_completed_at   = CASE
                WHEN last_backfill_cursor IS NOT NULL AND last_backfill_cursor <= $3
                    THEN COALESCE(backfill_completed_at, now())
                ELSE NULL
            END,
            updated_at              = now()
        WHERE tenant_id = $1
          AND source_key = $2
          AND backfill_window_end IS NOT NULL
          AND backfill_window_start IS DISTINCT FROM $3
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .bind(configured_earliest)
    .execute(pool)
    .await?;

    get_connector_state(pool, tenant_id, source_key).await
}

/// Advance the incremental total-count pass: add `count_delta` to the running
/// total and move the count cursor back to `new_cursor`.
///
/// `count_delta` is the number of events counted in the sub-window just covered
/// (`0` is allowed to mark counting finished for sources that cannot count,
/// leaving the total untouched). The count cursor only moves backward toward the
/// window start. Returns the refreshed state.
pub async fn advance_backfill_count(
    pool: &PgPool,
    tenant_id: Uuid,
    source_key: &str,
    count_delta: i64,
    new_cursor: DateTime<Utc>,
) -> Result<Option<ConnectorState>> {
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET backfill_total_estimate = CASE
                WHEN $4 > 0 THEN COALESCE(backfill_total_estimate, 0) + $4
                ELSE backfill_total_estimate
            END,
            backfill_count_cursor   = LEAST(COALESCE(backfill_count_cursor, $3), $3),
            updated_at              = now()
        WHERE tenant_id = $1 AND source_key = $2
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .bind(new_cursor)
    .bind(count_delta)
    .execute(pool)
    .await?;

    get_connector_state(pool, tenant_id, source_key).await
}

/// Mark backfill complete and pin the cursor at the window start.
pub async fn mark_backfill_complete(
    pool: &PgPool,
    tenant_id: Uuid,
    source_key: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET backfill_complete     = true,
            backfill_completed_at = now(),
            last_backfill_cursor  = COALESCE(backfill_window_start, last_backfill_cursor),
            updated_at            = now()
        WHERE tenant_id = $1 AND source_key = $2
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Ensure monthly `events` partitions exist across `[start, end]`.
///
/// Backfilling far into the past requires the matching monthly partitions to
/// exist before inserts (the table is range-partitioned by `occurred_at`).
/// Returns the number of months touched.
pub async fn ensure_event_partitions(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<i32> {
    let touched: i32 = sqlx::query_scalar("SELECT ensure_events_partitions($1, $2)")
        .bind(start)
        .bind(end)
        .fetch_one(pool)
        .await?;
    Ok(touched)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_state() -> ConnectorState {
        ConnectorState {
            tenant_id: Uuid::nil(),
            source_key: "usgs".to_owned(),
            source_name: "USGS".to_owned(),
            enabled: true,
            last_live_run_at: None,
            last_backfill_cursor: None,
            backfill_complete: false,
            backfill_window_start: None,
            backfill_window_end: None,
            backfill_started_at: None,
            backfill_completed_at: None,
            backfill_events_ingested: 0,
            backfill_total_estimate: None,
            backfill_count_cursor: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn fraction_is_zero_when_uninitialized() {
        assert_eq!(base_state().backfill_fraction(), 0.0);
    }

    #[test]
    fn fraction_is_one_when_complete() {
        let mut state = base_state();
        state.backfill_complete = true;
        assert_eq!(state.backfill_fraction(), 1.0);
    }

    #[test]
    fn fraction_is_count_based_when_total_known() {
        let mut state = base_state();
        state.backfill_total_estimate = Some(1000);
        state.backfill_events_ingested = 250;
        assert!((state.backfill_fraction() - 0.25).abs() < f64::EPSILON);

        // Overshoot (re-runs) clamps to 1.0, not above.
        state.backfill_events_ingested = 1500;
        assert_eq!(state.backfill_fraction(), 1.0);
    }

    #[test]
    fn partial_total_is_not_final_during_counting(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let start = DateTime::parse_from_rfc3339("1900-01-01T00:00:00Z")?.with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")?.with_timezone(&Utc);
        let mid = DateTime::parse_from_rfc3339("2010-01-01T00:00:00Z")?.with_timezone(&Utc);
        let mut state = base_state();
        state.backfill_window_start = Some(start);
        state.backfill_window_end = Some(end);
        state.last_backfill_cursor = Some(end);
        // Counting in progress: cursor still above window start -> not final.
        state.backfill_total_estimate = Some(500_000);
        state.backfill_count_cursor = Some(mid);
        state.backfill_events_ingested = 100_000;
        assert!(!state.backfill_total_final());
        // Falls back to time coverage (cursor at end -> 0%), not 100000/500000.
        assert_eq!(state.backfill_fraction(), 0.0);

        // Counting reaches the window start -> total becomes final, count-based.
        state.backfill_count_cursor = Some(start);
        assert!(state.backfill_total_final());
        assert!((state.backfill_fraction() - 0.2).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn fraction_ignores_zero_total_and_falls_back(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let start = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")?.with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")?.with_timezone(&Utc);
        let mut state = base_state();
        state.backfill_total_estimate = Some(0);
        state.backfill_window_start = Some(start);
        state.backfill_window_end = Some(end);
        state.last_backfill_cursor = Some(end);
        // Zero total is meaningless; fall back to time coverage (0% at window end).
        assert_eq!(state.backfill_fraction(), 0.0);
        Ok(())
    }

    #[test]
    fn fraction_tracks_cursor_position() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let start = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")?.with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")?.with_timezone(&Utc);
        let mid = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")?.with_timezone(&Utc);

        let mut state = base_state();
        state.backfill_window_start = Some(start);
        state.backfill_window_end = Some(end);
        state.last_backfill_cursor = Some(end);
        assert_eq!(state.backfill_fraction(), 0.0);

        state.last_backfill_cursor = Some(mid);
        let fraction = state.backfill_fraction();
        assert!(fraction > 0.2 && fraction < 0.3, "fraction was {fraction}");

        state.last_backfill_cursor = Some(start);
        assert_eq!(state.backfill_fraction(), 1.0);
        Ok(())
    }
}
