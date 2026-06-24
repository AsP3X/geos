//! Resolve NWS forecast/county/marine zone polygons for alerts with null geometry.
//!
//! Many NWS alerts are "zone-only": the feed omits `geometry` and lists UGC zone
//! codes under `properties.geocode`. We look up each zone via the public NWS
//! `/zones/{type}/{code}` API and inject a centroid `Point` so normalization
//! can place the alert on the globe.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};
use tracing::debug;

use super::http::{send_with_retry, RetryConfig};
use super::{RawRecord, Result};

/// Outcome counters for zone geometry hydration.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HydrateStats {
    /// Records that arrived without feature geometry.
    pub null_geometry: usize,
    /// Records that received an injected centroid.
    pub hydrated: usize,
    /// Records that could not be resolved (no UGC or zone lookup failed).
    pub unresolved: usize,
}

/// Cached UGC zone lookups for one ingest batch.
pub(crate) struct NwsZoneResolver {
    client: Client,
    zones_base: String,
    cache: HashMap<String, Option<Value>>,
    request_delay: Duration,
    retry: RetryConfig,
}

impl NwsZoneResolver {
    /// Build a resolver sharing the connector HTTP client and politeness settings.
    pub(crate) fn new(client: Client, request_delay: Duration, retry: RetryConfig) -> Self {
        Self {
            client,
            zones_base: "https://api.weather.gov/zones".to_owned(),
            cache: HashMap::new(),
            request_delay,
            retry,
        }
    }

    /// Inject centroid geometry for zone-only alert features.
    pub(crate) async fn hydrate(&mut self, records: &mut [RawRecord]) -> Result<HydrateStats> {
        let mut stats = HydrateStats::default();
        for record in records.iter_mut() {
            if feature_has_geometry(&record.payload) {
                continue;
            }
            stats.null_geometry += 1;
            let ugc_codes = extract_ugc_codes(&record.payload);
            if ugc_codes.is_empty() {
                stats.unresolved += 1;
                continue;
            }
            if let Some(geometry) = self.resolve_zone_geometry(&ugc_codes).await? {
                inject_geometry(&mut record.payload, geometry);
                stats.hydrated += 1;
            } else {
                stats.unresolved += 1;
            }
        }
        if stats.hydrated > 0 || stats.unresolved > 0 {
            debug!(
                null_geometry = stats.null_geometry,
                hydrated = stats.hydrated,
                unresolved = stats.unresolved,
                "NWS zone geometry hydration finished"
            );
        }
        Ok(stats)
    }

    async fn resolve_zone_geometry(&mut self, ugc_codes: &[String]) -> Result<Option<Value>> {
        let mut polygon_rings: Vec<Value> = Vec::new();
        for ugc in ugc_codes {
            if let Some(geom) = self.fetch_zone_geometry(ugc).await? {
                match geom.get("type").and_then(Value::as_str) {
                    Some("Polygon") => {
                        if let Some(coords) = geom.get("coordinates") {
                            polygon_rings.push(coords.clone());
                        }
                    }
                    Some("MultiPolygon") => {
                        if let Some(parts) = geom.get("coordinates").and_then(Value::as_array) {
                            polygon_rings.extend(parts.iter().cloned());
                        }
                    }
                    _ => {}
                }
            }
        }
        if polygon_rings.is_empty() {
            Ok(None)
        } else if polygon_rings.len() == 1 {
            Ok(Some(json!({
                "type": "Polygon",
                "coordinates": polygon_rings[0],
            })))
        } else {
            Ok(Some(json!({
                "type": "MultiPolygon",
                "coordinates": polygon_rings,
            })))
        }
    }

    async fn fetch_zone_geometry(&mut self, ugc: &str) -> Result<Option<Value>> {
        if let Some(cached) = self.cache.get(ugc) {
            return Ok(cached.clone());
        }
        let Some((zone_type, code)) = ugc_zone_path(ugc) else {
            self.cache.insert(ugc.to_owned(), None);
            return Ok(None);
        };
        if !self.request_delay.is_zero() {
            tokio::time::sleep(self.request_delay).await;
        }
        let url = format!("{}/{}/{}", self.zones_base, zone_type, code);
        let response = send_with_retry(|| self.client.get(&url), &self.retry).await?;
        let geometry = if response.status().is_success() {
            let body: Value = response.json().await?;
            body.get("geometry")
                .cloned()
                .filter(|g| !g.is_null() && geometry_centroid(g).is_some())
        } else {
            debug!(%ugc, status = %response.status(), "NWS zone lookup failed");
            None
        };
        self.cache.insert(ugc.to_owned(), geometry.clone());
        Ok(geometry)
    }
}

fn feature_has_geometry(payload: &Value) -> bool {
    payload
        .get("geometry")
        .filter(|g| !g.is_null())
        .and_then(|g| g.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|t| !t.is_empty())
}

fn extract_ugc_codes(payload: &Value) -> Vec<String> {
    payload
        .get("properties")
        .and_then(|p| p.get("geocode"))
        .and_then(|g| g.get("UGC"))
        .and_then(|u| u.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Map a six-character UGC code to the NWS zones API path segment.
fn ugc_zone_path(ugc: &str) -> Option<(&'static str, &str)> {
    let ugc = ugc.trim();
    if ugc.len() < 6 {
        return None;
    }
    let zone_type = match ugc.as_bytes().get(2).copied()? {
        b'Z' => "forecast",
        b'C' => "county",
        b'M' => "marine",
        _ => return None,
    };
    Some((zone_type, ugc))
}

pub(crate) fn inject_geometry(payload: &mut Value, geometry: Value) {
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("geometry".to_owned(), geometry);
    }
}

fn geometry_centroid(geometry: &Value) -> Option<(f64, f64)> {
    match geometry.get("type")?.as_str()? {
        "Point" => point_from_value(geometry.get("coordinates")?),
        "Polygon" => {
            let ring = geometry
                .get("coordinates")?
                .as_array()?
                .first()?
                .as_array()?;
            ring_centroid(ring)
        }
        "MultiPolygon" => {
            let ring = geometry
                .get("coordinates")?
                .as_array()?
                .first()?
                .as_array()?
                .first()?
                .as_array()?;
            ring_centroid(ring)
        }
        _ => None,
    }
}

fn point_from_value(value: &Value) -> Option<(f64, f64)> {
    let arr = value.as_array()?;
    if arr.len() >= 2 {
        Some((arr[0].as_f64()?, arr[1].as_f64()?))
    } else {
        None
    }
}

fn ring_centroid(ring: &[Value]) -> Option<(f64, f64)> {
    let mut lon_sum = 0.0;
    let mut lat_sum = 0.0;
    let mut count = 0usize;

    for coord in ring {
        let arr = coord.as_array()?;
        if arr.len() < 2 {
            continue;
        }
        let lon = arr[0].as_f64()?;
        let lat = arr[1].as_f64()?;
        lon_sum += lon;
        lat_sum += lat;
        count += 1;
    }

    if count == 0 {
        None
    } else {
        Some((lon_sum / count as f64, lat_sum / count as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ugc_zone_path_maps_forecast_and_county() {
        assert_eq!(ugc_zone_path("VAZ007"), Some(("forecast", "VAZ007")));
        assert_eq!(ugc_zone_path("ARC125"), Some(("county", "ARC125")));
        assert!(ugc_zone_path("BAD").is_none());
    }

    #[test]
    fn extract_ugc_codes_reads_properties_geocode() {
        let payload = json!({
            "properties": {
                "geocode": { "UGC": ["VAZ007", "WVZ042"] }
            }
        });
        assert_eq!(
            extract_ugc_codes(&payload),
            vec!["VAZ007".to_owned(), "WVZ042".to_owned()]
        );
    }

    #[test]
    fn inject_geometry_sets_feature_geometry() {
        let mut payload = json!({ "geometry": null });
        inject_geometry(
            &mut payload,
            json!({
                "type": "Point",
                "coordinates": [-81.0, 37.5]
            }),
        );
        assert_eq!(payload["geometry"]["type"], "Point");
        assert_eq!(payload["geometry"]["coordinates"][0], -81.0);
    }

    #[test]
    fn geometry_centroid_averages_polygon_ring(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let geom = json!({
            "type": "Polygon",
            "coordinates": [[[-122.5, 37.7], [-122.3, 37.7], [-122.3, 37.9], [-122.5, 37.9], [-122.5, 37.7]]]
        });
        let (lon, lat) = geometry_centroid(&geom).ok_or("expected polygon centroid")?;
        assert!((lon - (-122.42)).abs() < 0.01);
        assert!((lat - 37.78).abs() < 0.01);
        Ok(())
    }
}
