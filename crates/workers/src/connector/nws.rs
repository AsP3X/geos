//! NOAA/NWS active weather alerts GeoJSON connector.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

use super::{Connector, ConnectorError, HistoricalRange, RawRecord, Result, NWS_SOURCE};

/// Default active alerts feed (actual status only).
const ACTIVE_ALERTS_URL: &str = "https://api.weather.gov/alerts/active?status=actual";

/// User-Agent required by NWS API policy.
const NWS_USER_AGENT: &str = "Geos/0.1.0 (https://github.com/geos; dev@localhost)";

/// Fetches NWS active alert GeoJSON and returns raw features.
#[derive(Debug, Clone)]
pub struct NwsWeatherConnector {
    client: Client,
    active_alerts_url: String,
}

impl Default for NwsWeatherConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl NwsWeatherConnector {
    /// Build a connector with the default NWS endpoint and required User-Agent.
    #[must_use]
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(NWS_USER_AGENT)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            active_alerts_url: ACTIVE_ALERTS_URL.to_owned(),
        }
    }

    /// Override the alerts URL (tests or mirrors); production uses NWS defaults.
    #[must_use]
    pub fn with_url(active_alerts_url: impl Into<String>) -> Self {
        let mut connector = Self::new();
        connector.active_alerts_url = active_alerts_url.into();
        connector
    }

    // Human: NWS returns GeoJSON FeatureCollection; id comes from properties.id
    // (URN) with feature.id URL as fallback. Agent: PARSES collection; SKIPS
    // features without id; RETURNS RawRecord vec.
    fn parse_feature_collection(body: &str) -> Result<Vec<RawRecord>> {
        let collection: FeatureCollection =
            serde_json::from_str(body).map_err(|e| ConnectorError::Parse(e.to_string()))?;

        let mut records = Vec::with_capacity(collection.features.len());
        for feature in collection.features {
            let id = feature
                .properties
                .as_ref()
                .and_then(|p| p.id.as_deref())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    feature
                        .id
                        .as_deref()
                        .and_then(|url| url.rsplit('/').next())
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                });

            let Some(id) = id else {
                warn!("skipping NWS feature without id");
                continue;
            };

            let payload =
                serde_json::to_value(feature).map_err(|e| ConnectorError::Parse(e.to_string()))?;
            records.push(RawRecord {
                source: NWS_SOURCE.to_owned(),
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
                "NWS returned HTTP {}",
                response.status()
            )));
        }
        let body = response.text().await?;
        Self::parse_feature_collection(&body)
    }
}

#[async_trait]
impl Connector for NwsWeatherConnector {
    fn source_key(&self) -> &'static str {
        NWS_SOURCE
    }

    async fn fetch_live(&self) -> Result<Vec<RawRecord>> {
        self.get_geojson(&self.active_alerts_url).await
    }

    async fn fetch_historical(&self, _range: HistoricalRange) -> Result<Vec<RawRecord>> {
        // NWS does not expose a simple public historical alerts API; backfill is
        // out of scope for v1.
        debug!("NWS historical fetch skipped (no public archive API in v1)");
        Ok(Vec::new())
    }
}

/// Minimal GeoJSON FeatureCollection shape for NWS alerts.
#[derive(Debug, Deserialize)]
struct FeatureCollection {
    features: Vec<Feature>,
}

/// Minimal GeoJSON Feature for NWS alert entries.
#[derive(Debug, Deserialize, Serialize)]
struct Feature {
    #[serde(rename = "type")]
    feature_type: Option<String>,
    id: Option<String>,
    properties: Option<NwsProperties>,
    geometry: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct NwsProperties {
    id: Option<String>,
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
}
