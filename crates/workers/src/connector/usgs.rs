//! USGS earthquake GeoJSON connector (live summary feed + FDSNWS historical API).

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use super::{Connector, ConnectorError, HistoricalRange, RawRecord, Result, USGS_SOURCE};

/// Default live feed: earthquakes from the past day (GeoJSON summary).
const LIVE_FEED_URL: &str =
    "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson";

/// FDSNWS event query base (historical backfill).
const HISTORICAL_QUERY_URL: &str = "https://earthquake.usgs.gov/fdsnws/event/1/query";

/// Fetches USGS earthquake feeds and returns raw GeoJSON features.
#[derive(Debug, Clone)]
pub struct UsgsEarthquakeConnector {
    client: Client,
    live_feed_url: String,
    historical_query_url: String,
}

impl Default for UsgsEarthquakeConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl UsgsEarthquakeConnector {
    /// Build a connector with the default USGS endpoints and a shared HTTP client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            live_feed_url: LIVE_FEED_URL.to_owned(),
            historical_query_url: HISTORICAL_QUERY_URL.to_owned(),
        }
    }

    /// Override endpoints (tests or mirrors); production uses USGS defaults.
    #[must_use]
    pub fn with_urls(
        live_feed_url: impl Into<String>,
        historical_query_url: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            live_feed_url: live_feed_url.into(),
            historical_query_url: historical_query_url.into(),
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

    async fn get_geojson(&self, url: &str) -> Result<Vec<RawRecord>> {
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(ConnectorError::Parse(format!(
                "USGS returned HTTP {}",
                response.status()
            )));
        }
        let body = response.text().await?;
        Self::parse_feature_collection(&body)
    }
}

#[async_trait]
impl Connector for UsgsEarthquakeConnector {
    fn source_key(&self) -> &'static str {
        USGS_SOURCE
    }

    async fn fetch_live(&self) -> Result<Vec<RawRecord>> {
        self.get_geojson(&self.live_feed_url).await
    }

    async fn fetch_historical(&self, range: HistoricalRange) -> Result<Vec<RawRecord>> {
        if range.end < range.start {
            return Err(ConnectorError::InvalidRecord(
                "historical range end before start".to_owned(),
            ));
        }
        let url = format!(
            "{}?format=geojson&starttime={}&endtime={}&orderby=time",
            self.historical_query_url,
            range.start.format("%Y-%m-%dT%H:%M:%S"),
            range.end.format("%Y-%m-%dT%H:%M:%S"),
        );
        self.get_geojson(&url).await
    }
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
}
