//! USGS earthquake GeoJSON connector (live summary feed + FDSNWS historical API).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future::try_join_all;
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

use super::http::{build_client, send_with_retry, ConditionalCache, RetryConfig};
use super::{Connector, ConnectorError, HistoricalRange, RawRecord, Result, USGS_SOURCE};

/// Default live feed: earthquakes from the past day (GeoJSON summary).
const LIVE_FEED_URL: &str =
    "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson";

/// FDSNWS event query base (historical backfill).
const HISTORICAL_QUERY_URL: &str = "https://earthquake.usgs.gov/fdsnws/event/1/query";

/// Identifying User-Agent (USGS asks automated clients to identify themselves).
const USGS_USER_AGENT: &str = "Geos/0.1.0 (https://github.com/AsP3X/geos; ingestion)";

/// FDSNWS rejects queries matching more than this many events; we split windows
/// to stay under it (and pass it as the explicit `limit`).
const FDSNWS_MAX_RESULTS: u64 = 20_000;

/// Stop subdividing a window once it is this small, even if still over the cap
/// (avoids pathological recursion; earthquakes rarely exceed the cap in an hour).
const MIN_SPLIT: chrono::Duration = chrono::Duration::hours(1);

/// Hard cap on recursion depth as a final safety net.
const MAX_SPLIT_DEPTH: u32 = 24;

/// Maximum number of sub-windows a single over-cap window is divided into.
const MAX_SPLIT_FACTOR: i64 = 12;

/// Windows at or below this span skip the extra FDSNWS count request.
const SKIP_COUNT_MAX: chrono::Duration = chrono::Duration::hours(36);

/// Timeout for the FDSNWS `count` endpoint. Counting very large ranges can make
/// USGS hang for ~50s then 503 ("temp table full"); we cap the wait instead of
/// retry-storming and wedging the backfill task (see `count_window`). Generous
/// enough to tolerate slow yearly counts under concurrent ingestion load.
const COUNT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default maximum number of concurrent in-flight requests to USGS.
const DEFAULT_MAX_CONCURRENCY: usize = 4;

/// Fetches USGS earthquake feeds and returns raw GeoJSON features.
#[derive(Debug, Clone)]
pub struct UsgsEarthquakeConnector {
    client: Client,
    live_feed_url: String,
    historical_query_url: String,
    historical_count_url: String,
    request_delay: Duration,
    min_magnitude: Option<f64>,
    retry: RetryConfig,
    live_cache: Arc<Mutex<ConditionalCache>>,
    concurrency: Arc<tokio::sync::Semaphore>,
}

impl Default for UsgsEarthquakeConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl UsgsEarthquakeConnector {
    /// Build a connector with the default USGS endpoints and a hardened client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: build_client(USGS_USER_AGENT),
            live_feed_url: LIVE_FEED_URL.to_owned(),
            historical_query_url: HISTORICAL_QUERY_URL.to_owned(),
            historical_count_url: derive_count_url(HISTORICAL_QUERY_URL),
            request_delay: Duration::ZERO,
            min_magnitude: None,
            retry: RetryConfig::default(),
            live_cache: Arc::new(Mutex::new(ConditionalCache::default())),
            concurrency: Arc::new(tokio::sync::Semaphore::new(DEFAULT_MAX_CONCURRENCY)),
        }
    }

    /// Override endpoints (tests or mirrors); production uses USGS defaults.
    #[must_use]
    pub fn with_urls(
        live_feed_url: impl Into<String>,
        historical_query_url: impl Into<String>,
    ) -> Self {
        let historical_query_url = historical_query_url.into();
        let historical_count_url = derive_count_url(&historical_query_url);
        Self {
            client: build_client(USGS_USER_AGENT),
            live_feed_url: live_feed_url.into(),
            historical_query_url,
            historical_count_url,
            request_delay: Duration::ZERO,
            min_magnitude: None,
            retry: RetryConfig::default(),
            live_cache: Arc::new(Mutex::new(ConditionalCache::default())),
            concurrency: Arc::new(tokio::sync::Semaphore::new(DEFAULT_MAX_CONCURRENCY)),
        }
    }

    /// Set a politeness delay applied before each outbound request (rate limiting).
    #[must_use]
    pub fn with_request_delay(mut self, delay: Duration) -> Self {
        self.request_delay = delay;
        self
    }

    /// Restrict historical backfill to events at or above this magnitude.
    ///
    /// The FDSNWS catalog is dominated by micro-quakes; a floor (e.g. 2.5) cuts
    /// volume by an order of magnitude. Applies to the count and query endpoints
    /// only — the live summary feed is a fixed file and is unaffected.
    #[must_use]
    pub fn with_min_magnitude(mut self, min_magnitude: Option<f64>) -> Self {
        self.min_magnitude = min_magnitude;
        self
    }

    /// Override the maximum number of concurrent in-flight requests.
    #[must_use]
    pub fn with_max_concurrency(mut self, max: usize) -> Self {
        let permits = max.max(1);
        self.concurrency = Arc::new(tokio::sync::Semaphore::new(permits));
        self
    }

    /// FDSNWS `minmagnitude` query fragment, or empty when no floor is set.
    fn magnitude_param(&self) -> String {
        match self.min_magnitude {
            Some(m) => format!("&minmagnitude={m}"),
            None => String::new(),
        }
    }

    // Human: USGS GeoJSON is a FeatureCollection; each feature becomes one
    // RawRecord with `id` as source_event_id and the full feature in payload.
    // Agent: PARSES FeatureCollection; SKIPS features without id; RETURNS RawRecord vec.
    fn parse_feature_collection(body: &str) -> Result<Vec<RawRecord>> {
        let collection: FeatureCollection =
            serde_json::from_str(body).map_err(|e| ConnectorError::Parse(e.to_string()))?;

        let mut records = Vec::with_capacity(collection.features.len());
        for feature in collection.features {
            let Some(id) = feature
                .id
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
            else {
                warn!("skipping USGS feature without id");
                continue;
            };
            let payload =
                serde_json::to_value(feature).map_err(|e| ConnectorError::Parse(e.to_string()))?;
            records.push(RawRecord {
                source: USGS_SOURCE.to_owned(),
                source_event_id: id,
                payload,
            });
        }
        Ok(records)
    }

    /// GET with a concurrency permit, throttle, and retry; returns the response.
    async fn get(&self, url: &str) -> Result<Response> {
        let _permit = self.concurrency.acquire().await.ok();
        self.throttle().await;
        send_with_retry(|| self.client.get(url), &self.retry).await
    }

    async fn get_geojson(&self, url: &str) -> Result<Vec<RawRecord>> {
        let response = self.get(url).await?;
        if !response.status().is_success() {
            return Err(ConnectorError::Parse(format!(
                "USGS returned HTTP {}",
                response.status()
            )));
        }
        let body = response.text().await?;
        Self::parse_feature_collection(&body)
    }

    /// Count matching events in a window via the FDSNWS `count` method.
    ///
    /// Uses a short timeout and a single attempt (no retry): counting an
    /// over-large range makes USGS stall then return 503, and retrying that only
    /// wedges the calling backfill task. A fast failure lets callers fall back.
    async fn count_window(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<u64> {
        let url = format!(
            "{}?starttime={}&endtime={}{}",
            self.historical_count_url,
            format_fdsnws_time(start),
            format_fdsnws_time(end),
            self.magnitude_param(),
        );
        let _permit = self.concurrency.acquire().await.ok();
        self.throttle().await;
        let response = self.client.get(&url).timeout(COUNT_TIMEOUT).send().await?;
        if !response.status().is_success() {
            return Err(ConnectorError::Parse(format!(
                "USGS count returned HTTP {}",
                response.status()
            )));
        }
        let body = response.text().await?;
        body.trim().parse::<u64>().map_err(|e| {
            ConnectorError::Parse(format!("invalid USGS count '{}': {e}", body.trim()))
        })
    }

    fn query_url(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> String {
        format!(
            "{}?format=geojson&starttime={}&endtime={}&orderby=time&limit={}{}",
            self.historical_query_url,
            format_fdsnws_time(start),
            format_fdsnws_time(end),
            FDSNWS_MAX_RESULTS,
            self.magnitude_param(),
        )
    }

    /// Fetch a window, splitting adaptively when it exceeds the FDSNWS cap.
    ///
    /// Small windows fetch directly. Larger windows are sized using the `count`
    /// endpoint: an over-cap window is divided into `ceil(count / cap)` equal
    /// sub-windows (bounded) which are fetched concurrently, subject to the
    /// connector's request-concurrency limit.
    fn fetch_window<'a>(
        &'a self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        depth: u32,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<RawRecord>>> + Send + 'a>>
    {
        Box::pin(async move {
            let span = end - start;
            if span <= chrono::Duration::zero() {
                return Ok(Vec::new());
            }

            // Small windows go straight to fetch (saves the count round-trip).
            if span <= SKIP_COUNT_MAX {
                return self.get_geojson(&self.query_url(start, end)).await;
            }

            let count = self.count_window(start, end).await?;
            if count == 0 {
                return Ok(Vec::new());
            }

            let splittable = span > MIN_SPLIT && depth < MAX_SPLIT_DEPTH;
            if count > FDSNWS_MAX_RESULTS && splittable {
                let factor = split_factor(count);
                let bounds = subdivide(start, end, factor);
                debug!(%start, %end, count, factor, "subdividing USGS backfill window");
                let futures = bounds
                    .into_iter()
                    .map(|(s, e)| self.fetch_window(s, e, depth + 1));
                let parts = try_join_all(futures).await?;
                let mut records = Vec::new();
                for part in parts {
                    records.extend(part);
                }
                return Ok(records);
            }

            self.get_geojson(&self.query_url(start, end)).await
        })
    }

    async fn throttle(&self) {
        if !self.request_delay.is_zero() {
            tokio::time::sleep(self.request_delay).await;
        }
    }
}

#[async_trait]
impl Connector for UsgsEarthquakeConnector {
    fn source_key(&self) -> &'static str {
        USGS_SOURCE
    }

    async fn fetch_live(&self) -> Result<Vec<RawRecord>> {
        let _permit = self.concurrency.acquire().await.ok();
        self.throttle().await;

        let cache = lock_cache(&self.live_cache).clone();
        let url = self.live_feed_url.clone();
        let response = send_with_retry(|| cache.apply(self.client.get(&url)), &self.retry).await?;

        if response.status() == StatusCode::NOT_MODIFIED {
            debug!("USGS live feed unchanged (304); skipping parse");
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(ConnectorError::Parse(format!(
                "USGS returned HTTP {}",
                response.status()
            )));
        }

        lock_cache(&self.live_cache).update(&response);
        let body = response.text().await?;
        Self::parse_feature_collection(&body)
    }

    async fn fetch_historical(&self, range: HistoricalRange) -> Result<Vec<RawRecord>> {
        if range.end < range.start {
            return Err(ConnectorError::InvalidRecord(
                "historical range end before start".to_owned(),
            ));
        }
        self.fetch_window(range.start, range.end, 0).await
    }

    async fn count_historical(&self, range: HistoricalRange) -> Result<Option<u64>> {
        if range.end <= range.start {
            return Ok(Some(0));
        }
        let count = self.count_window(range.start, range.end).await?;
        Ok(Some(count))
    }
}

/// Lock the conditional cache, recovering from a poisoned mutex.
fn lock_cache(cache: &Mutex<ConditionalCache>) -> std::sync::MutexGuard<'_, ConditionalCache> {
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Number of sub-windows for an over-cap window, bounded to `MAX_SPLIT_FACTOR`.
fn split_factor(count: u64) -> i64 {
    let needed = count.div_ceil(FDSNWS_MAX_RESULTS);
    (needed as i64).clamp(2, MAX_SPLIT_FACTOR)
}

/// Divide `[start, end]` into `factor` adjacent sub-windows (last absorbs rounding).
fn subdivide(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    factor: i64,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let factor = factor.max(1);
    let step = (end - start) / (factor as i32);
    let mut bounds = Vec::with_capacity(factor as usize);
    let mut cursor = start;
    for i in 0..factor {
        let next = if i == factor - 1 { end } else { cursor + step };
        bounds.push((cursor, next));
        cursor = next;
    }
    bounds
}

/// Derive the FDSNWS `count` endpoint from a `query` endpoint URL.
fn derive_count_url(query_url: &str) -> String {
    match query_url.strip_suffix("query") {
        Some(prefix) => format!("{prefix}count"),
        None => format!("{}/count", query_url.trim_end_matches('/')),
    }
}

/// Format a timestamp as FDSNWS expects (UTC, no zone suffix).
fn format_fdsnws_time(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Minimal GeoJSON FeatureCollection shape for USGS feeds.
#[derive(Debug, Deserialize)]
struct FeatureCollection {
    features: Vec<Feature>,
}

/// Minimal GeoJSON Feature for USGS earthquake entries.
#[derive(Debug, Deserialize, Serialize)]
struct Feature {
    #[serde(rename = "type")]
    feature_type: Option<String>,
    id: Option<String>,
    properties: Option<Value>,
    geometry: Option<Geometry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Geometry {
    #[serde(rename = "type")]
    geometry_type: Option<String>,
    coordinates: Option<Vec<f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample_feature_collection() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let body = include_str!("../../tests/fixtures/usgs_sample.geojson");
        let records = UsgsEarthquakeConnector::parse_feature_collection(body)?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source, USGS_SOURCE);
        assert_eq!(records[0].source_event_id, "us7000example");
        Ok(())
    }

    #[test]
    fn count_url_derives_from_query_url() {
        assert_eq!(
            derive_count_url("https://earthquake.usgs.gov/fdsnws/event/1/query"),
            "https://earthquake.usgs.gov/fdsnws/event/1/count"
        );
        assert_eq!(
            derive_count_url("http://localhost/usgs"),
            "http://localhost/usgs/count"
        );
    }

    #[test]
    fn fdsnws_time_has_no_zone_suffix() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let ts = DateTime::parse_from_rfc3339("2024-03-01T12:30:45Z")?.with_timezone(&Utc);
        assert_eq!(format_fdsnws_time(ts), "2024-03-01T12:30:45");
        Ok(())
    }

    #[test]
    fn magnitude_param_present_only_when_set() {
        let plain = UsgsEarthquakeConnector::new();
        assert_eq!(plain.magnitude_param(), "");

        let filtered = UsgsEarthquakeConnector::new().with_min_magnitude(Some(2.5));
        assert_eq!(filtered.magnitude_param(), "&minmagnitude=2.5");
    }

    #[test]
    fn split_factor_scales_with_count() {
        assert_eq!(split_factor(FDSNWS_MAX_RESULTS), 2);
        assert_eq!(split_factor(FDSNWS_MAX_RESULTS * 3 + 1), 4);
        // Huge counts clamp to the bound.
        assert_eq!(split_factor(FDSNWS_MAX_RESULTS * 1_000), MAX_SPLIT_FACTOR);
    }

    #[test]
    fn subdivide_covers_window_without_gaps() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let start = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")?.with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2020-01-05T00:00:00Z")?.with_timezone(&Utc);
        let bounds = subdivide(start, end, 4);
        assert_eq!(bounds.len(), 4);
        assert_eq!(bounds[0].0, start);
        assert_eq!(bounds[3].1, end);
        for pair in bounds.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "sub-windows must be contiguous");
        }
        Ok(())
    }
}
