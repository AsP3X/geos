//! Shared [`Connector`] trait and ingestion error types (`connector-contract.mdc`).

mod http;
mod nws;
mod usgs;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use thiserror::Error;

pub use nws::{NwsWeatherConnector, NWS_HISTORY_DAYS};
pub use usgs::UsgsEarthquakeConnector;

/// Stable source key for idempotency (`source + source_event_id`).
pub const USGS_SOURCE: &str = "usgs";

/// NOAA/NWS active weather alerts source key.
pub const NWS_SOURCE: &str = "nws";

/// Inclusive UTC window for historical backfill fetches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoricalRange {
    /// Earliest event time to include.
    pub start: DateTime<Utc>,
    /// Latest event time to include.
    pub end: DateTime<Utc>,
}

/// A single raw record from an external source before normalization.
#[derive(Debug, Clone, PartialEq)]
pub struct RawRecord {
    /// Connector source key (e.g. `"usgs"`).
    pub source: String,
    /// Source-native identifier for upsert dedupe.
    pub source_event_id: String,
    /// Original payload retained in `Event.raw`.
    pub payload: Value,
}

/// Errors from connector fetch/normalize paths (not HTTP API errors).
#[derive(Debug, Error)]
pub enum ConnectorError {
    /// HTTP transport or status failure talking to the source.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// Response body could not be parsed.
    #[error("parse error: {0}")]
    Parse(String),
    /// A single record was malformed and skipped upstream.
    #[error("invalid record: {0}")]
    InvalidRecord(String),
    /// Configuration prevented the fetch (e.g. air-gapped mode).
    #[error("connector disabled: {0}")]
    Disabled(String),
}

/// Convenience result alias for connector operations.
pub type Result<T> = std::result::Result<T, ConnectorError>;

/// Contract every data source must implement (`connector-contract.mdc`).
///
/// `fetch_*` returns raw records only; normalization and DB writes are separate
/// pipeline stages.
#[async_trait]
pub trait Connector: Send + Sync {
    /// Stable source identifier matching `Event.source` and upsert keys.
    fn source_key(&self) -> &'static str;

    /// Fetch the latest live feed (recent events).
    async fn fetch_live(&self) -> Result<Vec<RawRecord>>;

    /// Fetch events in a historical time window for backfill.
    async fn fetch_historical(&self, range: HistoricalRange) -> Result<Vec<RawRecord>>;

    /// Total events matching a window, when the source can report it up front.
    ///
    /// Used to seed accurate count-based backfill progress. Returns `None` by
    /// default for sources without a count capability (progress then falls back
    /// to time coverage).
    async fn count_historical(&self, _range: HistoricalRange) -> Result<Option<u64>> {
        Ok(None)
    }
}
