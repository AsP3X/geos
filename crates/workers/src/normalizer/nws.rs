//! NWS alert GeoJSON feature -> canonical [`Event`] mapping.

use chrono::{DateTime, Utc};
use geos_core::events::{Category, Event, EventStatus, GeoPoint, VerificationStatus};
use geos_core::impact;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::connector::{ConnectorError, RawRecord, Result};

/// Map one NWS raw record into a canonical [`Event`].
pub fn normalize_nws_record(record: &RawRecord, tenant_id: Uuid) -> Result<Event> {
    let feature: NwsFeature = serde_json::from_value(record.payload.clone())
        .map_err(|e| ConnectorError::Parse(e.to_string()))?;

    let props = feature
        .properties
        .ok_or_else(|| ConnectorError::InvalidRecord("missing properties".into()))?;

    let (lon, lat) = geometry_centroid(&feature.geometry)
        .ok_or_else(|| ConnectorError::InvalidRecord("missing usable geometry".into()))?;

    let occurred_at = parse_nws_time(props.onset.as_deref())
        .or_else(|| parse_nws_time(props.effective.as_deref()))
        .or_else(|| parse_nws_time(props.sent.as_deref()))
        .ok_or_else(|| ConnectorError::InvalidRecord("invalid or missing time".into()))?;

    let detected_at = parse_nws_time(props.sent.as_deref());

    let (impact_score, severity) =
        impact::nws_alert(props.severity.as_deref(), props.urgency.as_deref());

    let title = props.headline.clone().or_else(|| props.event.clone());

    let summary = props.event.clone().or(props.area_desc.clone());
    let place_name = props.area_desc.clone();

    let body = props.description.clone();
    let status = match props.status.as_deref() {
        Some("Expired") | Some("Cancel") => EventStatus::Archived,
        _ if props
            .expires
            .as_deref()
            .and_then(|s| parse_nws_time(Some(s)))
            .is_some_and(|exp| exp < Utc::now()) =>
        {
            EventStatus::Archived
        }
        _ => EventStatus::Active,
    };

    let category = map_category(props.event.as_deref(), props.category.as_deref());

    let mut tags = vec!["weather".to_owned(), "nws".to_owned()];
    if let Some(ref event) = props.event {
        tags.push(format!("event:{}", slug_tag(event)));
    }
    if let Some(ref certainty) = props.certainty {
        tags.push(format!("certainty:{}", certainty.to_lowercase()));
    }
    if let Some(ref urgency) = props.urgency {
        tags.push(format!("urgency:{}", urgency.to_lowercase()));
    }

    let confidence = match props.certainty.as_deref() {
        Some("Observed") => 0.95,
        Some("Likely") => 0.85,
        Some("Possible") => 0.65,
        Some("Unlikely") => 0.4,
        _ => 0.75,
    };

    let url = feature.id.clone();

    let now = Utc::now();

    Ok(Event {
        id: Uuid::new_v4(),
        tenant_id,
        source: record.source.clone(),
        source_event_id: record.source_event_id.clone(),
        category,
        severity,
        impact_score,
        magnitude: None,
        title,
        summary,
        body,
        original_text: props.instruction.clone(),
        translated_text: None,
        language: Some("en".to_owned()),
        location: GeoPoint { lon, lat },
        affected_area: None,
        country: Some("US".to_owned()),
        region: None,
        place_name,
        occurred_at,
        detected_at,
        ingested_at: now,
        status,
        verification_status: VerificationStatus::Verified,
        confidence,
        tags,
        url,
        raw: record.payload.clone(),
        embedding: None,
    })
}

fn map_category(event: Option<&str>, category: Option<&str>) -> Category {
    let event_lower = event.unwrap_or("").to_lowercase();

    if event_lower.contains("earthquake") {
        return Category::Earthquake;
    }
    if event_lower.contains("wildfire")
        || event_lower.contains("fire weather")
        || event_lower.contains("red flag")
    {
        return Category::Wildfire;
    }

    match category {
        Some("Met") => Category::Weather,
        Some("Geo") => Category::Weather,
        Some("Fire") => Category::Wildfire,
        _ if event_lower.contains("storm")
            || event_lower.contains("flood")
            || event_lower.contains("wind")
            || event_lower.contains("heat")
            || event_lower.contains("cold")
            || event_lower.contains("winter")
            || event_lower.contains("tornado")
            || event_lower.contains("hurricane") =>
        {
            Category::Weather
        }
        _ => Category::Alert,
    }
}

fn slug_tag(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

fn parse_nws_time(value: Option<&str>) -> Option<DateTime<Utc>> {
    value.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    })
}

/// Centroid of Point, Polygon, or MultiPolygon geometry (first ring only).
fn geometry_centroid(geometry: &Option<NwsGeometry>) -> Option<(f64, f64)> {
    let geometry = geometry.as_ref()?;
    match geometry.geometry_type.as_deref()? {
        "Point" => point_from_value(geometry.coordinates.as_ref()?),
        "Polygon" => {
            let ring = geometry
                .coordinates
                .as_ref()?
                .as_array()?
                .first()?
                .as_array()?;
            ring_centroid(ring)
        }
        "MultiPolygon" => {
            let ring = geometry
                .coordinates
                .as_ref()?
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

#[derive(Debug, Deserialize)]
struct NwsFeature {
    id: Option<String>,
    properties: Option<NwsProperties>,
    geometry: Option<NwsGeometry>,
}

#[derive(Debug, Deserialize)]
struct NwsProperties {
    #[serde(rename = "areaDesc")]
    area_desc: Option<String>,
    sent: Option<String>,
    effective: Option<String>,
    onset: Option<String>,
    expires: Option<String>,
    status: Option<String>,
    category: Option<String>,
    severity: Option<String>,
    certainty: Option<String>,
    urgency: Option<String>,
    event: Option<String>,
    headline: Option<String>,
    description: Option<String>,
    instruction: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NwsGeometry {
    #[serde(rename = "type")]
    geometry_type: Option<String>,
    coordinates: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::{ConnectorError, RawRecord, NWS_SOURCE};
    use geos_core::events::Category;
    use geos_core::tenancy::SYSTEM_TENANT_ID;
    use serde_json::Value;

    fn sample_record() -> crate::connector::Result<RawRecord> {
        let body: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/nws_sample.geojson"))
                .map_err(|e| ConnectorError::Parse(e.to_string()))?;
        Ok(RawRecord {
            source: NWS_SOURCE.to_owned(),
            source_event_id: "urn:oid:2.49.0.1.840.0.1234567890.0.1234567890".to_owned(),
            payload: body["features"][0].clone(),
        })
    }

    #[test]
    fn normalizes_fixture_to_canonical_event() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let record = sample_record()?;
        let event = normalize_nws_record(&record, SYSTEM_TENANT_ID)?;

        assert_eq!(event.source, NWS_SOURCE);
        assert_eq!(event.category, Category::Weather);
        assert_eq!(event.tenant_id, SYSTEM_TENANT_ID);
        assert!((event.location.lon - (-122.42)).abs() < 0.01);
        assert!((event.location.lat - 37.78).abs() < 0.01);
        assert!(event.impact_score > 0);
        assert!(event.raw.is_object());
        Ok(())
    }

    #[test]
    fn idempotency_key_uses_source_and_source_event_id(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let record = sample_record()?;
        let a = normalize_nws_record(&record, SYSTEM_TENANT_ID)?;
        let b = normalize_nws_record(&record, SYSTEM_TENANT_ID)?;
        assert_eq!(a.source, b.source);
        assert_eq!(a.source_event_id, b.source_event_id);
        assert_ne!(a.id, b.id);
        Ok(())
    }
}
