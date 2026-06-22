//! Live ingestion pipeline: fetch -> normalize -> upsert.

use geos_core::db::{self, touch_live_run, PgPool};
use geos_core::meili::{self, MeiliClient};
use geos_core::AppError;
use tracing::warn;

use crate::connector::{Connector, ConnectorError, UsgsEarthquakeConnector, USGS_SOURCE};
use crate::normalizer::normalize_record;

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

/// Fetch the USGS live feed, normalize each feature, and upsert idempotently.
pub async fn ingest_usgs_live(
    pool: &PgPool,
    connector: &UsgsEarthquakeConnector,
    tenant_id: uuid::Uuid,
    meili: Option<&MeiliClient>,
) -> Result<IngestStats, IngestError> {
    let records = connector.fetch_live().await?;
    let mut stats = IngestStats {
        fetched: records.len(),
        ..IngestStats::default()
    };

    for record in records {
        let event = match normalize_record(&record, tenant_id) {
            Ok(event) => event,
            Err(err) => {
                warn!(
                    source = %record.source,
                    source_event_id = %record.source_event_id,
                    error = %err,
                    "skipping malformed USGS record"
                );
                stats.skipped += 1;
                continue;
            }
        };

        if let Err(err) = db::upsert_event(pool, &event).await {
            warn!(
                source_event_id = %event.source_event_id,
                error = %err,
                "skipping USGS record after upsert failure"
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

    touch_live_run(pool, tenant_id, USGS_SOURCE).await?;
    Ok(stats)
}
