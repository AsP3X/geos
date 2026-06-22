//! USGS GeoJSON feature -> canonical [`Event`] mapping.

use chrono::{TimeZone, Utc};
use geos_core::events::{Category, Event, EventStatus, GeoPoint, VerificationStatus};
use geos_core::impact;
use serde::Deserialize;
use uuid::Uuid;

use crate::connector::{ConnectorError, RawRecord, Result};

/// Map one USGS raw record into a canonical [`Event`].
pub fn normalize_usgs_record(record: &RawRecord, tenant_id: Uuid) -> Result<Event> {
    let feature: UsgsFeature = serde_json::from_value(record.payload.clone())
        .map_err(|e| ConnectorError::Parse(e.to_string()))?;

    let props = feature
        .properties
        .ok_or_else(|| ConnectorError::InvalidRecord("missing properties".into()))?;

    let coords = feature
        .geometry
        .and_then(|g| g.coordinates)
        .filter(|c| c.len() >= 2)
        .ok_or_else(|| ConnectorError::InvalidRecord("missing coordinates".into()))?;

    let lon = coords[0];
    let lat = coords[1];

    let occurred_at = props
        .time
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
        .ok_or_else(|| ConnectorError::InvalidRecord("invalid or missing time".into()))?;

    let detected_at = props
        .updated
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single());

    let magnitude = props.mag;
    let (impact_score, severity) = magnitude
        .map(impact::earthquake_magnitude)
        .unwrap_or((0, geos_core::events::Severity::Info));

    let title = props
        .title
        .or_else(|| magnitude.map(|m| format!("M{m:.1} earthquake")));

    let place_name = props.place;
    let url = props.url;
    let status = match props.status.as_deref() {
        Some("reviewed") | Some("review") => EventStatus::Active,
        Some("deleted") | Some("expired") => EventStatus::Archived,
        _ => EventStatus::Active,
    };

    let mut tags = vec!["earthquake".to_owned(), "usgs".to_owned()];
    if let Some(ref mag_type) = props.mag_type {
        tags.push(format!("mag_type:{mag_type}"));
    }
    if props.tsunami == Some(1) {
        tags.push("tsunami".to_owned());
    }

    let now = Utc::now();
    let confidence = if props.status.as_deref() == Some("reviewed") {
        0.9
    } else {
        0.7
    };

    Ok(Event {
        id: Uuid::new_v4(),
        tenant_id,
        source: record.source.clone(),
        source_event_id: record.source_event_id.clone(),
        category: Category::Earthquake,
        severity,
        impact_score,
        magnitude,
        title,
        summary: place_name.clone(),
        body: None,
        original_text: None,
        translated_text: None,
        language: Some("en".to_owned()),
        location: GeoPoint { lon, lat },
        affected_area: None,
        country: None,
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

#[derive(Debug, Deserialize)]
struct UsgsFeature {
    properties: Option<UsgsProperties>,
    geometry: Option<UsgsGeometry>,
}

#[derive(Debug, Deserialize)]
struct UsgsGeometry {
    coordinates: Option<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct UsgsProperties {
    mag: Option<f64>,
    place: Option<String>,
    time: Option<i64>,
    updated: Option<i64>,
    url: Option<String>,
    title: Option<String>,
    status: Option<String>,
    #[serde(rename = "magType")]
    mag_type: Option<String>,
    tsunami: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::{ConnectorError, RawRecord, USGS_SOURCE};
    use geos_core::tenancy::SYSTEM_TENANT_ID;
    use serde_json::Value;

    fn sample_record() -> crate::connector::Result<RawRecord> {
        let body: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/usgs_sample.geojson"))
                .map_err(|e| ConnectorError::Parse(e.to_string()))?;
        Ok(RawRecord {
            source: USGS_SOURCE.to_owned(),
            source_event_id: "us7000example".to_owned(),
            payload: body["features"][0].clone(),
        })
    }

    #[test]
    fn normalizes_fixture_to_canonical_event() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let record = sample_record()?;
        let event = normalize_usgs_record(&record, SYSTEM_TENANT_ID)?;

        assert_eq!(event.source, USGS_SOURCE);
        assert_eq!(event.source_event_id, "us7000example");
        assert_eq!(event.category, Category::Earthquake);
        assert_eq!(event.tenant_id, SYSTEM_TENANT_ID);
        assert!((event.location.lon - (-122.4194)).abs() < f64::EPSILON);
        assert!((event.location.lat - 37.7749).abs() < f64::EPSILON);
        assert_eq!(event.magnitude, Some(4.5));
        assert!(event.impact_score > 0);
        assert!(event.raw.is_object());
        Ok(())
    }

    #[test]
    fn idempotency_key_uses_source_and_source_event_id(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let record = sample_record()?;
        let a = normalize_usgs_record(&record, SYSTEM_TENANT_ID)?;
        let b = normalize_usgs_record(&record, SYSTEM_TENANT_ID)?;
        assert_eq!(a.source, b.source);
        assert_eq!(a.source_event_id, b.source_event_id);
        assert_ne!(a.id, b.id);
        Ok(())
    }
}
