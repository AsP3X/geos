//! Ingestion pipeline: fetch -> normalize -> upsert (live feeds and backfill).

use std::collections::HashMap;

use geos_core::db::{
    self, add_backfill_events_ingested, touch_live_run, upsert_events_backfill, PgPool,
};
use geos_core::events::Event;
use geos_core::meili::{self, MeiliClient};
use geos_core::AppError;
use tracing::{debug, warn};

use crate::connector::{
    Connector, ConnectorError, HistoricalRange, RawRecord, UsgsEarthquakeConnector,
};
use crate::normalizer::normalize_record;

/// How often to flush the backfill event counter to the DB during a large chunk.
const BACKFILL_EVENTS_FLUSH_EVERY: usize = 10;

/// Batches backfill event-count updates so the progress UI ticks during long chunks.
pub struct BackfillEventsFlush<'a> {
    pool: &'a PgPool,
    tenant_id: uuid::Uuid,
    source_key: &'a str,
    pending: usize,
}

impl<'a> BackfillEventsFlush<'a> {
    /// Track incremental upserts for `(tenant_id, source_key)`.
    pub fn new(pool: &'a PgPool, tenant_id: uuid::Uuid, source_key: &'a str) -> Self {
        Self {
            pool,
            tenant_id,
            source_key,
            pending: 0,
        }
    }

    /// Call after each successful upsert during backfill.
    pub async fn on_upsert(&mut self) -> Result<(), IngestError> {
        self.on_upsert_batch(1).await
    }

    /// Record a batch of successful upserts.
    pub async fn on_upsert_batch(&mut self, count: usize) -> Result<(), IngestError> {
        if count == 0 {
            return Ok(());
        }
        self.pending += count;
        if self.pending >= BACKFILL_EVENTS_FLUSH_EVERY {
            self.flush_pending().await?;
        }
        Ok(())
    }

    /// Flush any remaining counted events (call once at end of chunk).
    pub async fn finish(mut self) -> Result<(), IngestError> {
        self.flush_pending().await
    }

    async fn flush_pending(&mut self) -> Result<(), IngestError> {
        if self.pending == 0 {
            return Ok(());
        }
        let batch = self.pending as i64;
        self.pending = 0;
        add_backfill_events_ingested(self.pool, self.tenant_id, self.source_key, batch).await?;
        Ok(())
    }
}

/// Outcome counters for one ingestion pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestStats {
    /// Raw records returned by the connector.
    pub fetched: usize,
    /// Rows successfully upserted.
    pub upserted: usize,
    /// Records skipped due to normalization or upsert errors.
    pub skipped: usize,
}

/// Errors that abort an entire ingestion pass (not single-record skips).
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// Connector fetch failed.
    #[error(transparent)]
    Connector(#[from] ConnectorError),
    /// Database or core layer failure.
    #[error(transparent)]
    Core(#[from] AppError),
}

/// Tallies records skipped during normalization, grouped by failure reason.
///
/// Avoids one log line per skipped record (some feeds, e.g. NWS zone-only
/// alerts, can skip hundreds per batch and flood the logs).
#[derive(Default)]
struct SkipTally {
    reasons: HashMap<String, usize>,
}

impl SkipTally {
    fn record(&mut self, source: &str, source_event_id: &str, err: &ConnectorError) {
        *self.reasons.entry(err.to_string()).or_insert(0) += 1;
        debug!(
            source = %source,
            source_event_id = %source_event_id,
            error = %err,
            "skipping malformed connector record"
        );
    }

    fn total(&self) -> usize {
        self.reasons.values().sum()
    }

    fn log_summary(&self, source: &str) {
        if self.reasons.is_empty() {
            return;
        }
        let mut breakdown: Vec<String> = self
            .reasons
            .iter()
            .map(|(reason, count)| format!("{count}× {reason}"))
            .collect();
        breakdown.sort();
        warn!(
            source = %source,
            total = self.total(),
            "skipped malformed connector records: {}",
            breakdown.join("; ")
        );
    }
}

/// Normalize + idempotently upsert a batch of raw records into one tenant (live path).
async fn ingest_records_live(
    pool: &PgPool,
    records: Vec<RawRecord>,
    tenant_id: uuid::Uuid,
    meili: Option<&MeiliClient>,
) -> Result<IngestStats, IngestError> {
    let mut stats = IngestStats {
        fetched: records.len(),
        ..IngestStats::default()
    };
    let source = records
        .first()
        .map(|r| r.source.clone())
        .unwrap_or_default();
    let mut skips = SkipTally::default();

    for record in records {
        let event = match normalize_record(&record, tenant_id) {
            Ok(event) => event,
            Err(err) => {
                skips.record(&record.source, &record.source_event_id, &err);
                stats.skipped += 1;
                continue;
            }
        };

        if let Err(err) = db::upsert_event(pool, &event).await {
            warn!(
                source_event_id = %event.source_event_id,
                error = %err,
                "skipping record after upsert failure"
            );
            stats.skipped += 1;
            continue;
        }

        stats.upserted += 1;

        if let Some(meili) = meili {
            if let Err(err) = meili::upsert_event_document(meili.client(), &event).await {
                warn!(
                    source_event_id = %event.source_event_id,
                    error = %err,
                    "meilisearch indexing failed"
                );
            }
        }
    }

    skips.log_summary(&source);
    Ok(stats)
}

/// Fast backfill path: normalize, batch upsert (no NOTIFY / Meilisearch).
async fn ingest_records_backfill(
    pool: &PgPool,
    records: Vec<RawRecord>,
    tenant_id: uuid::Uuid,
    mut backfill_flush: Option<&mut BackfillEventsFlush<'_>>,
) -> Result<IngestStats, IngestError> {
    let mut stats = IngestStats {
        fetched: records.len(),
        ..IngestStats::default()
    };

    let source = records
        .first()
        .map(|r| r.source.clone())
        .unwrap_or_default();
    let mut skips = SkipTally::default();

    let mut events: Vec<Event> = Vec::with_capacity(records.len());
    for record in records {
        match normalize_record(&record, tenant_id) {
            Ok(event) => events.push(event),
            Err(err) => {
                skips.record(&record.source, &record.source_event_id, &err);
                stats.skipped += 1;
            }
        }
    }
    skips.log_summary(&source);

    const BATCH: usize = 50;
    for chunk in events.chunks(BATCH) {
        match upsert_events_backfill(pool, chunk).await {
            Ok(written) => {
                stats.upserted += written;
                if let Some(ref mut flush) = backfill_flush {
                    flush.on_upsert_batch(written).await?;
                }
            }
            Err(err) => {
                warn!(error = %err, batch_size = chunk.len(), "backfill batch upsert failed");
                stats.skipped += chunk.len();
            }
        }
    }

    Ok(stats)
}

/// Fetch a connector live feed, normalize each record, and upsert idempotently.
pub async fn ingest_connector_live(
    pool: &PgPool,
    connector: &dyn Connector,
    tenant_id: uuid::Uuid,
    meili: Option<&MeiliClient>,
) -> Result<IngestStats, IngestError> {
    let source_key = connector.source_key();
    let records = connector.fetch_live().await?;
    let stats = ingest_records_live(pool, records, tenant_id, meili).await?;
    touch_live_run(pool, tenant_id, source_key).await?;
    Ok(stats)
}

/// Fetch one historical window, normalize, and upsert idempotently.
///
/// Ensures the monthly `events` partitions covering the window exist before
/// inserting (backfill can reach far into the past). Cursor/progress bookkeeping
/// is handled by the caller after a successful chunk.
///
/// Search indexing and live-stream NOTIFY are skipped during backfill.
pub async fn ingest_connector_historical(
    pool: &PgPool,
    connector: &dyn Connector,
    tenant_id: uuid::Uuid,
    range: HistoricalRange,
    backfill_flush: Option<&mut BackfillEventsFlush<'_>>,
) -> Result<IngestStats, IngestError> {
    db::ensure_event_partitions(pool, range.start, range.end).await?;
    let records = connector.fetch_historical(range).await?;
    ingest_records_backfill(pool, records, tenant_id, backfill_flush).await
}

/// Fetch the USGS live feed, normalize each feature, and upsert idempotently.
pub async fn ingest_usgs_live(
    pool: &PgPool,
    connector: &UsgsEarthquakeConnector,
    tenant_id: uuid::Uuid,
    meili: Option<&MeiliClient>,
) -> Result<IngestStats, IngestError> {
    ingest_connector_live(pool, connector, tenant_id, meili).await
}
