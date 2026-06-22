//! Historical backfill orchestration.
//!
//! Walks a connector's history newest -> oldest in bounded chunks, advancing the
//! resumable cursor in `connectors_state` and reporting progress. Each task run
//! processes up to [`BackfillConfig::chunks_per_run`] windows so work stays
//! bounded; the scheduler re-enqueues until the cursor reaches the window start
//! (`connector-contract.mdc`).

use std::time::Duration;

use chrono::{DateTime, Utc};
use geos_core::db::{
    self, advance_backfill_count, advance_backfill_cursor, get_connector_state,
    init_backfill_window, mark_backfill_complete, reconcile_backfill_window, ConnectorState,
    PgPool,
};

use crate::connector::{Connector, HistoricalRange};
use crate::ingest::{ingest_connector_historical, BackfillEventsFlush, IngestError};

/// Sub-window span counted per request when building the total denominator.
///
/// USGS FDSNWS can count ~1 year (~10^5 events) reliably but 503s on multi-year
/// ranges, so the catalog total is summed one year at a time.
const COUNT_BUCKET: chrono::Duration = chrono::Duration::days(365);

/// Number of count sub-windows issued (concurrently) per backfill run.
///
/// Bounds the per-run counting cost so chunk ingestion still makes progress; the
/// count cursor persists, so counting resumes on the next run until complete.
/// Kept modest to avoid hammering USGS alongside the chunk/live requests.
const COUNT_BUCKETS_PER_RUN: usize = 4;

/// Default earliest target for USGS backfill: the full USGS catalog (~1900).
///
/// The ANSS ComCat earthquake catalog effectively starts in 1900; targeting it
/// pulls every quake. Override with `BACKFILL_USGS_START` for a shorter window.
fn default_usgs_start() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("1900-01-01T00:00:00Z")
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now() - chrono::Duration::days(365))
}

/// Tunable backfill behavior, sourced from environment variables.
#[derive(Debug, Clone)]
pub struct BackfillConfig {
    /// Whether historical backfill runs at all.
    pub enabled: bool,
    /// Earliest target time for USGS earthquake backfill.
    pub usgs_start: DateTime<Utc>,
    /// Time span fetched per chunk (the connector may split it further).
    pub chunk: chrono::Duration,
    /// Maximum chunks processed in a single task run.
    pub chunks_per_run: u32,
    /// Politeness delay applied between chunk fetches.
    pub request_delay: Duration,
    /// Minimum earthquake magnitude for USGS backfill (`None` = no floor).
    pub usgs_min_magnitude: Option<f64>,
    /// Maximum concurrent in-flight USGS requests during backfill.
    pub usgs_max_concurrency: usize,
}

impl Default for BackfillConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            usgs_start: default_usgs_start(),
            chunk: chrono::Duration::hours(6),
            chunks_per_run: 4,
            request_delay: Duration::ZERO,
            usgs_min_magnitude: None,
            usgs_max_concurrency: 4,
        }
    }
}

impl BackfillConfig {
    /// Build configuration from environment variables, falling back to defaults.
    ///
    /// - `BACKFILL_ENABLED` (bool, default true)
    /// - `BACKFILL_USGS_START` (RFC3339 or `YYYY-MM-DD`, default 1900-01-01 / full catalog)
    /// - `BACKFILL_CHUNK_HOURS` (u32 hours, default 6; overrides `BACKFILL_CHUNK_DAYS` when set)
    /// - `BACKFILL_CHUNK_DAYS` (u32 days, legacy fallback when hours unset)
    /// - `BACKFILL_CHUNKS_PER_RUN` (u32, default 4)
    /// - `BACKFILL_REQUEST_DELAY_MS` (u64 ms, default 0)
    /// - `BACKFILL_USGS_MIN_MAGNITUDE` (f64, default none/all magnitudes; `<= 0` disables the floor)
    /// - `BACKFILL_USGS_MAX_CONCURRENCY` (usize, default 4)
    #[must_use]
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let chunk = env_parse::<u32>("BACKFILL_CHUNK_HOURS")
            .filter(|hours| *hours > 0)
            .map(|hours| chrono::Duration::hours(i64::from(hours)))
            .or_else(|| {
                env_parse::<i64>("BACKFILL_CHUNK_DAYS")
                    .filter(|days| *days > 0)
                    .map(chrono::Duration::days)
            })
            .unwrap_or(defaults.chunk);
        Self {
            enabled: env_bool("BACKFILL_ENABLED").unwrap_or(defaults.enabled),
            usgs_start: std::env::var("BACKFILL_USGS_START")
                .ok()
                .filter(|raw| !raw.trim().is_empty())
                .and_then(|raw| parse_start(&raw))
                .unwrap_or(defaults.usgs_start),
            chunk,
            chunks_per_run: env_parse::<u32>("BACKFILL_CHUNKS_PER_RUN")
                .filter(|n| *n > 0)
                .unwrap_or(defaults.chunks_per_run),
            request_delay: env_parse::<u64>("BACKFILL_REQUEST_DELAY_MS")
                .map(Duration::from_millis)
                .unwrap_or(defaults.request_delay),
            usgs_min_magnitude: match env_parse::<f64>("BACKFILL_USGS_MIN_MAGNITUDE") {
                Some(m) if m > 0.0 => Some(m),
                Some(_) => None,
                None => defaults.usgs_min_magnitude,
            },
            usgs_max_concurrency: env_parse::<usize>("BACKFILL_USGS_MAX_CONCURRENCY")
                .filter(|n| *n > 0)
                .unwrap_or(defaults.usgs_max_concurrency),
        }
    }
}

/// Run a bounded batch of backfill chunks for one connector + tenant.
///
/// `earliest` is the oldest time to target (e.g. [`BackfillConfig::usgs_start`]
/// for USGS, or `now - 7d` for NWS). Returns the number of events upserted in
/// this run. No-ops when disabled, the source is disabled, or backfill is done.
pub async fn run_backfill(
    pool: &PgPool,
    connector: &dyn Connector,
    tenant_id: uuid::Uuid,
    earliest: DateTime<Utc>,
    config: &BackfillConfig,
) -> Result<u64, IngestError> {
    let source_key = connector.source_key();

    if !config.enabled {
        return Ok(0);
    }

    let Some(state) = get_connector_state(pool, tenant_id, source_key).await? else {
        tracing::warn!(%source_key, "no connector state row; skipping backfill");
        return Ok(0);
    };
    if !state.enabled {
        return Ok(0);
    }

    // Establish (or re-point) the target window. The total denominator is built
    // incrementally afterward (see `advance_total_count`), not queried up front.
    let state = if !state.backfill_initialized() {
        let window_end = Utc::now();
        let window_start = earliest.min(window_end);
        tracing::info!(%source_key, %window_start, %window_end, "initializing backfill window");
        init_backfill_window(pool, tenant_id, source_key, window_start, window_end)
            .await?
            .unwrap_or(state)
    } else if state.backfill_window_start != Some(earliest.min(state_window_end(&state))) {
        let window_start = earliest.min(state_window_end(&state));
        tracing::info!(%source_key, %window_start, "re-pointing backfill window start");
        reconcile_backfill_window(pool, tenant_id, source_key, window_start)
            .await?
            .unwrap_or(state)
    } else {
        state
    };

    if state.backfill_complete {
        return Ok(0);
    }

    let Some(window_start) = state.backfill_window_start else {
        return Ok(0);
    };

    // Advance the total-count denominator a bounded amount this run (resumable).
    advance_total_count(pool, connector, tenant_id, &state, window_start).await?;
    let mut cursor = state
        .last_backfill_cursor
        .or(state.backfill_window_end)
        .unwrap_or_else(Utc::now);

    let mut total_upserted: u64 = 0;
    for _ in 0..config.chunks_per_run {
        if cursor <= window_start {
            break;
        }
        let chunk_start = window_start.max(cursor - config.chunk);
        let range = HistoricalRange {
            start: chunk_start,
            end: cursor,
        };

        let mut flush = BackfillEventsFlush::new(pool, tenant_id, source_key);
        let stats =
            ingest_connector_historical(pool, connector, tenant_id, range, Some(&mut flush))
                .await?;
        flush.finish().await?;
        // Event counts were flushed incrementally; only move the cursor here.
        advance_backfill_cursor(pool, tenant_id, source_key, chunk_start, 0).await?;
        total_upserted += stats.upserted as u64;
        cursor = chunk_start;

        if !config.request_delay.is_zero() {
            tokio::time::sleep(config.request_delay).await;
        }
    }

    if cursor <= window_start {
        mark_backfill_complete(pool, tenant_id, source_key).await?;
        tracing::info!(%source_key, total_upserted, "backfill complete");
    }

    Ok(total_upserted)
}

/// Whether a connector still has backfill work outstanding (for scheduling).
pub async fn backfill_pending(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    source_key: &str,
) -> geos_core::Result<bool> {
    let Some(state) = db::get_connector_state(pool, tenant_id, source_key).await? else {
        return Ok(false);
    };
    Ok(state.enabled && !state.backfill_complete)
}

/// The stored backfill window end, or now if not yet initialized.
fn state_window_end(state: &db::ConnectorState) -> DateTime<Utc> {
    state.backfill_window_end.unwrap_or_else(Utc::now)
}

/// Build the total-count denominator incrementally, a bounded amount per run.
///
/// USGS cannot count the whole catalog in one request, so the total is summed
/// over [`COUNT_BUCKET`]-sized sub-windows walked newest -> oldest. Progress is
/// persisted in `backfill_count_cursor` and resumes across runs; up to
/// [`COUNT_BUCKETS_PER_RUN`] windows are counted (concurrently) here so chunk
/// ingestion still proceeds. A source that cannot count (`count_historical`
/// returns `None`) marks counting finished so it is not retried every run.
async fn advance_total_count(
    pool: &PgPool,
    connector: &dyn Connector,
    tenant_id: uuid::Uuid,
    state: &ConnectorState,
    window_start: DateTime<Utc>,
) -> Result<(), IngestError> {
    let source_key = connector.source_key();
    let frontier = state
        .backfill_count_cursor
        .unwrap_or_else(|| state_window_end(state));
    if frontier <= window_start {
        return Ok(()); // Counting already complete.
    }

    // Build a wave of consecutive sub-windows from the frontier backward.
    let mut buckets = Vec::with_capacity(COUNT_BUCKETS_PER_RUN);
    let mut edge = frontier;
    for _ in 0..COUNT_BUCKETS_PER_RUN {
        if edge <= window_start {
            break;
        }
        let bucket_start = window_start.max(edge - COUNT_BUCKET);
        buckets.push((bucket_start, edge));
        edge = bucket_start;
    }

    let counts = futures::future::join_all(
        buckets
            .iter()
            .map(|&(start, end)| connector.count_historical(HistoricalRange { start, end })),
    )
    .await;

    // Apply only the leading run of successful buckets (newest -> oldest). A
    // transient failure stops the wave but keeps prior successes, so counting
    // still advances and the failed sub-window is retried on the next run.
    let mut sum: i64 = 0;
    let mut new_cursor = frontier;
    for ((bucket_start, _), result) in buckets.iter().zip(counts) {
        match result {
            Ok(Some(count)) => {
                sum += count.min(i64::MAX as u64) as i64;
                new_cursor = *bucket_start;
            }
            Ok(None) => {
                // Source cannot count; finish counting without a total so the UI
                // stays on time-based progress and we stop probing every run.
                advance_backfill_count(pool, tenant_id, source_key, 0, window_start).await?;
                return Ok(());
            }
            Err(err) => {
                tracing::warn!(%source_key, error = %err, "backfill count sub-window failed; will retry");
                break;
            }
        }
    }

    if new_cursor >= frontier {
        return Ok(()); // First sub-window failed; no progress this run.
    }

    advance_backfill_count(pool, tenant_id, source_key, sum, new_cursor).await?;
    if new_cursor <= window_start {
        tracing::info!(%source_key, "backfill total count complete");
    } else {
        tracing::debug!(%source_key, %new_cursor, counted = sum, "advanced backfill total count");
    }
    Ok(())
}

fn parse_start(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    let naive = date.and_hms_opt(0, 0, 0)?;
    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

fn env_bool(key: &str) -> Option<bool> {
    match std::env::var(key)
        .ok()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_parse<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok()?.trim().parse::<T>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_usgs_start_is_full_catalog() {
        let start = default_usgs_start();
        assert_eq!(start.format("%Y-%m-%d").to_string(), "1900-01-01");
    }

    #[test]
    fn default_min_magnitude_pulls_all_quakes() {
        assert_eq!(BackfillConfig::default().usgs_min_magnitude, None);
    }

    #[test]
    fn parse_start_accepts_date_and_rfc3339() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let date = parse_start("2010-05-01").ok_or("expected a parsed date")?;
        assert_eq!(date.format("%Y-%m-%d").to_string(), "2010-05-01");

        let datetime = parse_start("2010-05-01T06:00:00Z").ok_or("expected a parsed datetime")?;
        assert_eq!(datetime.format("%H").to_string(), "06");

        assert!(parse_start("not-a-date").is_none());
        Ok(())
    }

    #[test]
    fn default_config_is_enabled_with_six_hour_chunks() {
        let config = BackfillConfig::default();
        assert!(config.enabled);
        assert_eq!(config.chunk.num_hours(), 6);
        assert_eq!(config.chunks_per_run, 4);
    }
}
