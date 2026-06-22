//! Event index mapping and tenant-scoped search.

use chrono::{DateTime, Utc};
use meilisearch_sdk::client::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::events::{Category, Event, Severity};
use crate::Result;

/// Meilisearch index uid for canonical events.
pub const EVENTS_INDEX: &str = "events";

/// Document stored in Meilisearch for keyword/fuzzy search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventDocument {
    /// Event id (primary key).
    pub id: String,
    /// Owning tenant — filterable for isolation.
    pub tenant_id: String,
    /// Source key.
    pub source: String,
    /// Optional title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Optional place name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_name: Option<String>,
    /// Searchable tags.
    pub tags: Vec<String>,
    /// Category string.
    pub category: String,
    /// Severity string.
    pub severity: String,
    /// Impact score 0–100.
    pub impact_score: u8,
    /// Occurred-at timestamp (sortable).
    pub occurred_at: DateTime<Utc>,
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lon: f64,
}

/// One search hit returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventSearchHit {
    /// Event id.
    pub id: Uuid,
    /// Tenant id.
    pub tenant_id: Uuid,
    /// Source key.
    pub source: String,
    /// Optional title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional summary snippet context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Optional place name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_name: Option<String>,
    /// Category.
    pub category: Category,
    /// Severity.
    pub severity: Severity,
    /// Impact score.
    pub impact_score: u8,
    /// When the event occurred.
    pub occurred_at: DateTime<Utc>,
    /// Location latitude.
    pub lat: f64,
    /// Location longitude.
    pub lon: f64,
}

/// Tenant-scoped search response.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchResults {
    /// Original query string.
    pub query: String,
    /// Matching documents.
    pub hits: Vec<EventSearchHit>,
    /// Requested page size.
    pub limit: usize,
    /// Requested offset.
    pub offset: usize,
    /// Total hits estimate from Meilisearch.
    pub estimated_total: usize,
}

/// Create or update the `events` index settings (idempotent).
pub async fn ensure_events_index(client: &Client) -> Result<()> {
    let index = client.index(EVENTS_INDEX);

    if client.get_index(EVENTS_INDEX).await.is_err() {
        client
            .create_index(EVENTS_INDEX, Some("id"))
            .await
            .map_err(map_meili_err)?;
    }

    index
        .set_filterable_attributes(["tenant_id", "category", "severity"])
        .await
        .map_err(map_meili_err)?;
    index
        .set_sortable_attributes(["occurred_at", "impact_score"])
        .await
        .map_err(map_meili_err)?;
    index
        .set_searchable_attributes([
            "title",
            "summary",
            "place_name",
            "tags",
            "source",
            "category",
        ])
        .await
        .map_err(map_meili_err)?;

    Ok(())
}

/// Upsert one event document into the search index.
pub async fn upsert_event_document(client: &Client, event: &Event) -> Result<()> {
    let document = EventDocument::from_event(event);
    client
        .index(EVENTS_INDEX)
        .add_or_replace(&[document], Some("id"))
        .await
        .map_err(map_meili_err)?;
    Ok(())
}

/// Run a tenant-scoped full-text search.
pub async fn search_events(
    client: &Client,
    tenant_id: Uuid,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<SearchResults> {
    let filter = format!("tenant_id = \"{tenant_id}\"");
    let results = client
        .index(EVENTS_INDEX)
        .search()
        .with_query(query)
        .with_filter(&filter)
        .with_limit(limit)
        .with_offset(offset)
        .execute::<EventDocument>()
        .await
        .map_err(map_meili_err)?;

    let hits = results
        .hits
        .into_iter()
        .filter_map(|hit| document_to_search_hit(hit.result).ok())
        .collect();

    Ok(SearchResults {
        query: query.to_owned(),
        hits,
        limit,
        offset,
        estimated_total: results.estimated_total_hits.unwrap_or(0) as usize,
    })
}

impl EventDocument {
    fn from_event(event: &Event) -> Self {
        Self {
            id: event.id.to_string(),
            tenant_id: event.tenant_id.to_string(),
            source: event.source.clone(),
            title: event.title.clone(),
            summary: event.summary.clone(),
            place_name: event.place_name.clone(),
            tags: event.tags.clone(),
            category: category_str(event.category).to_owned(),
            severity: severity_str(event.severity).to_owned(),
            impact_score: event.impact_score,
            occurred_at: event.occurred_at,
            lat: event.location.lat,
            lon: event.location.lon,
        }
    }
}

fn document_to_search_hit(document: EventDocument) -> Result<EventSearchHit> {
    Ok(EventSearchHit {
        id: document
            .id
            .parse()
            .map_err(|_| crate::error::AppError::internal("invalid event id in search index"))?,
        tenant_id: document
            .tenant_id
            .parse()
            .map_err(|_| crate::error::AppError::internal("invalid tenant id in search index"))?,
        source: document.source,
        title: document.title,
        summary: document.summary,
        place_name: document.place_name,
        category: parse_category(&document.category)?,
        severity: parse_severity(&document.severity)?,
        impact_score: document.impact_score,
        occurred_at: document.occurred_at,
        lat: document.lat,
        lon: document.lon,
    })
}

fn category_str(value: Category) -> &'static str {
    match value {
        Category::Earthquake => "earthquake",
        Category::Incident => "incident",
        Category::Alert => "alert",
        Category::Weather => "weather",
        Category::News => "news",
        Category::Conflict => "conflict",
        Category::Wildfire => "wildfire",
        Category::Other => "other",
    }
}

fn severity_str(value: Severity) -> &'static str {
    match value {
        Severity::Info => "info",
        Severity::Low => "low",
        Severity::Moderate => "moderate",
        Severity::High => "high",
        Severity::Critical => "critical",
    }
}

fn parse_category(value: &str) -> Result<Category> {
    match value {
        "earthquake" => Ok(Category::Earthquake),
        "incident" => Ok(Category::Incident),
        "alert" => Ok(Category::Alert),
        "weather" => Ok(Category::Weather),
        "news" => Ok(Category::News),
        "conflict" => Ok(Category::Conflict),
        "wildfire" => Ok(Category::Wildfire),
        "other" => Ok(Category::Other),
        _ => Err(crate::error::AppError::internal(format!(
            "unknown category in index: {value}"
        ))),
    }
}

fn parse_severity(value: &str) -> Result<Severity> {
    match value {
        "info" => Ok(Severity::Info),
        "low" => Ok(Severity::Low),
        "moderate" => Ok(Severity::Moderate),
        "high" => Ok(Severity::High),
        "critical" => Ok(Severity::Critical),
        _ => Err(crate::error::AppError::internal(format!(
            "unknown severity in index: {value}"
        ))),
    }
}

fn map_meili_err(err: meilisearch_sdk::errors::Error) -> crate::error::AppError {
    crate::error::AppError::internal(format!("meilisearch error: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventStatus, GeoPoint, VerificationStatus};

    #[test]
    fn event_document_maps_core_fields() {
        let now = Utc::now();
        let tenant_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();
        let event = Event {
            id: event_id,
            tenant_id,
            source: "usgs".to_owned(),
            source_event_id: "abc".to_owned(),
            category: Category::Earthquake,
            severity: Severity::High,
            impact_score: 80,
            magnitude: Some(5.1),
            title: Some("Quake".to_owned()),
            summary: None,
            body: None,
            original_text: None,
            translated_text: None,
            language: None,
            location: GeoPoint { lon: 1.0, lat: 2.0 },
            affected_area: None,
            country: None,
            region: None,
            place_name: Some("Somewhere".to_owned()),
            occurred_at: now,
            detected_at: None,
            ingested_at: now,
            status: EventStatus::Active,
            verification_status: VerificationStatus::Unverified,
            confidence: 0.5,
            tags: vec!["test".to_owned()],
            url: None,
            raw: serde_json::json!({}),
            embedding: None,
        };

        let doc = EventDocument::from_event(&event);
        assert_eq!(doc.id, event_id.to_string());
        assert_eq!(doc.tenant_id, tenant_id.to_string());
        assert_eq!(doc.category, "earthquake");
    }
}
