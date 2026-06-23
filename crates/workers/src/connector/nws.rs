//! NOAA/NWS active weather alerts GeoJSON connector.
//!
//! The public NWS API only retains the last ~7 days of alerts (`/alerts`); there
//! is no public deep archive, so historical backfill is bounded to that window.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tracing::{debug, warn};

use super::http::{build_client, send_with_retry, ConditionalCache, RetryConfig};
use super::nws_zones::NwsZoneResolver;
use super::{Connector, ConnectorError, HistoricalRange, RawRecord, Result, NWS_SOURCE};

/// Default active alerts feed (actual status only).
const ACTIVE_ALERTS_URL: &str = "https://api.weather.gov/alerts/active?status=actual";

/// Base alerts endpoint (covers active + the last ~7 days for historical fetch).
const ALERTS_URL: &str = "https://api.weather.gov/alerts";

/// User-Agent required by NWS API policy.
const NWS_USER_AGENT: &str = "Geos/0.1.0 (https://github.com/geos; dev@localhost)";

/// The public NWS API only serves alerts from roughly the last 7 days.
pub const NWS_HISTORY_DAYS: i64 = 7;

/// Safety cap on pagination pages per historical fetch.
const MAX_PAGES: usize = 100;

/// NWS alerts API page size (used to detect the final page).
const NWS_PAGE_LIMIT: usize = 500;

/// Fetches NWS alert GeoJSON and returns raw features.
#[derive(Debug, Clone)]
pub struct NwsWeatherConnector {
    client: Client,
    active_alerts_url: String,
    alerts_url: String,
    request_delay: Duration,
    retry: RetryConfig,
    live_cache: Arc<Mutex<ConditionalCache>>,
}

impl Default for NwsWeatherConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl NwsWeatherConnector {
    /// Build a connector with the default NWS endpoints and hardened client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: build_client(NWS_USER_AGENT),
            active_alerts_url: ACTIVE_ALERTS_URL.to_owned(),
            alerts_url: ALERTS_URL.to_owned(),
            request_delay: Duration::ZERO,
            retry: RetryConfig::default(),
            live_cache: Arc::new(Mutex::new(ConditionalCache::default())),
        }
    }

    /// Override the alerts URLs (tests or mirrors); production uses NWS defaults.
    #[must_use]
    pub fn with_url(active_alerts_url: impl Into<String>) -> Self {
        let mut connector = Self::new();
        connector.active_alerts_url = active_alerts_url.into();
        connector
    }

    /// Override both the active and base alerts URLs (tests or mirrors).
    #[must_use]
    pub fn with_urls(active_alerts_url: impl Into<String>, alerts_url: impl Into<String>) -> Self {
        let mut connector = Self::new();
        connector.active_alerts_url = active_alerts_url.into();
        connector.alerts_url = alerts_url.into();
        connector
    }

    /// Set a politeness delay applied before each outbound request (rate limiting).
    #[must_use]
    pub fn with_request_delay(mut self, delay: Duration) -> Self {
        self.request_delay = delay;
        self
    }

    // Human: NWS returns a GeoJSON FeatureCollection; id comes from properties.id
    // (URN) with the feature.id URL tail as fallback. Agent: PARSES the collection
    // as untyped JSON so the FULL feature (all properties + geometry) is preserved
    // in `payload` for the normalizer; SKIPS features without id; RETURNS (records,
    // next page url).
    fn parse_page(body: &str) -> Result<(Vec<RawRecord>, Option<String>)> {
        let root: Value =
            serde_json::from_str(body).map_err(|e| ConnectorError::Parse(e.to_string()))?;

        let empty = Vec::new();
        let features = root
            .get("features")
            .and_then(Value::as_array)
            .unwrap_or(&empty);

        let mut records = Vec::with_capacity(features.len());
        for feature in features {
            let id = feature
                .get("properties")
                .and_then(|p| p.get("id"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    feature
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|url| url.rsplit('/').next())
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                });

            let Some(id) = id else {
                warn!("skipping NWS feature without id");
                continue;
            };

            records.push(RawRecord {
                source: NWS_SOURCE.to_owned(),
                source_event_id: id,
                payload: feature.clone(),
            });
        }

        let next = root
            .get("pagination")
            .and_then(|p| p.get("next"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        Ok((records, next))
    }

    fn parse_feature_collection(body: &str) -> Result<Vec<RawRecord>> {
        Self::parse_page(body).map(|(records, _next)| records)
    }

    /// Fetch all pages starting at `url`, following `pagination.next` links.
    async fn get_all_pages(&self, first_url: String) -> Result<Vec<RawRecord>> {
        let mut url = Some(first_url);
        let mut all = Vec::new();
        let mut pages = 0;

        while let Some(current) = url.take() {
            if pages >= MAX_PAGES {
                warn!(pages, "NWS pagination hit page cap; stopping");
                break;
            }
            self.throttle().await;
            let response = send_with_retry(|| self.client.get(&current), &self.retry).await?;
            if !response.status().is_success() {
                return Err(ConnectorError::Parse(format!(
                    "NWS returned HTTP {}",
                    response.status()
                )));
            }
            let body = response.text().await?;
            let (records, next) = Self::parse_page(&body)?;
            let fetched = records.len();
            all.extend(records);
            pages += 1;
            // NWS always returns a `next` link; stop on empty or short (last) pages.
            url = if fetched == 0 || fetched < NWS_PAGE_LIMIT {
                None
            } else {
                next
            };
        }

        Ok(all)
    }

    async fn throttle(&self) {
        if !self.request_delay.is_zero() {
            tokio::time::sleep(self.request_delay).await;
        }
    }

    /// Resolve UGC zone centroids for alerts that omit feature geometry.
    async fn hydrate_zone_geometry(&self, records: &mut [RawRecord]) -> Result<()> {
        let mut resolver =
            NwsZoneResolver::new(self.client.clone(), self.request_delay, self.retry);
        resolver.hydrate(records).await?;
        Ok(())
    }
}

#[async_trait]
impl Connector for NwsWeatherConnector {
    fn source_key(&self) -> &'static str {
        NWS_SOURCE
    }

    async fn fetch_live(&self) -> Result<Vec<RawRecord>> {
        self.throttle().await;

        let cache = lock_cache(&self.live_cache).clone();
        let url = self.active_alerts_url.clone();
        let response = send_with_retry(|| cache.apply(self.client.get(&url)), &self.retry).await?;

        if response.status() == StatusCode::NOT_MODIFIED {
            debug!("NWS active alerts unchanged (304); skipping parse");
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(ConnectorError::Parse(format!(
                "NWS returned HTTP {}",
                response.status()
            )));
        }

        lock_cache(&self.live_cache).update(&response);
        let body = response.text().await?;
        let mut records = Self::parse_feature_collection(&body)?;
        self.hydrate_zone_geometry(&mut records).await?;
        Ok(records)
    }

    async fn fetch_historical(&self, range: HistoricalRange) -> Result<Vec<RawRecord>> {
        // NWS only exposes the last ~7 days of alerts; clamp the requested window
        // to what is actually retrievable (no public deep archive).
        let now = Utc::now();
        let earliest = now - chrono::Duration::days(NWS_HISTORY_DAYS);
        let start = range.start.max(earliest);
        let end = range.end.min(now);
        if end <= start {
            debug!(%start, %end, "NWS historical window outside the available 7-day archive");
            return Ok(Vec::new());
        }

        let url = format!(
            "{}?status=actual&limit=500&start={}&end={}",
            self.alerts_url,
            format_nws_time(start),
            format_nws_time(end),
        );
        let mut records = self.get_all_pages(url).await?;
        self.hydrate_zone_geometry(&mut records).await?;
        Ok(records)
    }
}

/// Lock the conditional cache, recovering from a poisoned mutex.
fn lock_cache(cache: &Mutex<ConditionalCache>) -> std::sync::MutexGuard<'_, ConditionalCache> {
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Format a timestamp as ISO-8601 UTC with a `Z` suffix (NWS query params).
fn format_nws_time(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample_feature_collection() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let body = include_str!("../../tests/fixtures/nws_sample.geojson");
        let records = NwsWeatherConnector::parse_feature_collection(body)?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source, NWS_SOURCE);
        assert!(records[0]
            .source_event_id
            .starts_with("urn:oid:2.49.0.1.840.0.1234567890"));
        Ok(())
    }

    #[test]
    fn nws_time_has_z_suffix() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let ts = DateTime::parse_from_rfc3339("2024-03-01T12:30:45Z")?.with_timezone(&Utc);
        assert_eq!(format_nws_time(ts), "2024-03-01T12:30:45Z");
        Ok(())
    }

    #[test]
    fn parse_page_preserves_full_feature_payload(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        // Regression: the payload must retain ALL properties + geometry so the
        // normalizer can read onset/effective/sent/event/etc. Earlier the typed
        // re-serialization stripped properties down to just `id`, which made the
        // normalizer reject every record ("invalid or missing time").
        let body = r#"{
            "features": [{
                "type": "Feature",
                "id": "https://api.weather.gov/alerts/urn:oid:test.1",
                "geometry": { "type": "Point", "coordinates": [-100.0, 40.0] },
                "properties": {
                    "id": "urn:oid:test.1",
                    "event": "Severe Thunderstorm Warning",
                    "onset": "2026-06-22T18:00:00-05:00",
                    "severity": "Severe",
                    "areaDesc": "Somewhere, US"
                }
            }],
            "pagination": {}
        }"#;
        let (records, _next) = NwsWeatherConnector::parse_page(body)?;
        assert_eq!(records.len(), 1);
        let props = &records[0].payload["properties"];
        assert_eq!(props["onset"], "2026-06-22T18:00:00-05:00");
        assert_eq!(props["event"], "Severe Thunderstorm Warning");
        assert_eq!(props["severity"], "Severe");
        assert_eq!(records[0].payload["geometry"]["type"], "Point");
        Ok(())
    }

    #[test]
    fn parse_page_extracts_next_link() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let body = r#"{
            "features": [],
            "pagination": { "next": "https://api.weather.gov/alerts?cursor=abc" }
        }"#;
        let (records, next) = NwsWeatherConnector::parse_page(body)?;
        assert!(records.is_empty());
        assert_eq!(
            next.as_deref(),
            Some("https://api.weather.gov/alerts?cursor=abc")
        );
        Ok(())
    }
}
