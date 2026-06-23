//! Event index mapping and tenant-scoped search.

use chrono::{DateTime, Utc};
use meilisearch_sdk::client::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::events::{Category, Event, Severity};
use crate::tenancy::SYSTEM_TENANT_ID;
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
    /// Occurred-at timestamp (sortable, human-readable; for display).
    pub occurred_at: DateTime<Utc>,
    /// Occurred-at as epoch seconds. Meili range filters require a numeric
    /// attribute (the RFC3339 string above cannot be range-filtered).
    pub occurred_at_unix: i64,
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

    // Human: Both geos-api and geos-workers bootstrap this index at startup.
    // Meilisearch index creation is asynchronous, so two near-simultaneous
    // creates can race; the loser's task fails with "index already exists".
    // We await the create task and treat an index that ends up existing as
    // success, so the race never propagates as an application error.
    // Agent: TOLERATES concurrent create race; ERRORS only if index truly absent.
    if client.get_index(EVENTS_INDEX).await.is_err() {
        if let Ok(task) = client.create_index(EVENTS_INDEX, Some("id")).await {
            let _ = task.wait_for_completion(client, None, None).await;
        }
        if client.get_index(EVENTS_INDEX).await.is_err() {
            return Err(crate::error::AppError::internal(
                "failed to create meilisearch events index",
            ));
        }
    }

    index
        .set_filterable_attributes([
            "tenant_id",
            "category",
            "severity",
            "impact_score",
            "source",
            "occurred_at_unix",
        ])
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

/// Optional attribute filters applied on top of tenant scoping during search.
///
/// Multi-value dimensions are disjunctive within themselves (Meili `IN [...]`)
/// and conjunctive with the other clauses. Magnitude is intentionally absent —
/// it is not indexed and so cannot constrain search.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Restrict to these categories; empty = all.
    pub categories: Vec<Category>,
    /// Restrict to these severities; empty = all.
    pub severities: Vec<Severity>,
    /// Restrict to these source keys; empty = all.
    pub sources: Vec<String>,
    /// Minimum impact score (0–100).
    pub impact_min: Option<u8>,
    /// Maximum impact score (0–100).
    pub impact_max: Option<u8>,
    /// Lower bound on `occurred_at` (inclusive).
    pub occurred_after: Option<DateTime<Utc>>,
    /// Upper bound on `occurred_at` (inclusive).
    pub occurred_before: Option<DateTime<Utc>>,
}

impl SearchFilters {
    /// Build the Meilisearch filter expression. Scoped to the caller's tenant
    /// plus the shared public-feed (system) tenant; private per-tenant data
    /// stays isolated (`tenant-isolation.mdc`).
    fn to_expression(&self, tenant_id: Uuid) -> String {
        let mut clauses = vec![format!(
            "(tenant_id = \"{tenant_id}\" OR tenant_id = \"{SYSTEM_TENANT_ID}\")"
        )];
        if !self.categories.is_empty() {
            clauses.push(meili_in(
                "category",
                self.categories.iter().map(|c| category_str(*c)),
            ));
        }
        if !self.severities.is_empty() {
            clauses.push(meili_in(
                "severity",
                self.severities.iter().map(|s| severity_str(*s)),
            ));
        }
        if !self.sources.is_empty() {
            clauses.push(meili_in("source", self.sources.iter().map(String::as_str)));
        }
        if let Some(impact_min) = self.impact_min {
            clauses.push(format!("impact_score >= {impact_min}"));
        }
        if let Some(impact_max) = self.impact_max {
            clauses.push(format!("impact_score <= {impact_max}"));
        }
        if let Some(after) = self.occurred_after {
            clauses.push(format!("occurred_at_unix >= {}", after.timestamp()));
        }
        if let Some(before) = self.occurred_before {
            clauses.push(format!("occurred_at_unix <= {}", before.timestamp()));
        }
        clauses.join(" AND ")
    }
}

/// Render a Meili `attr IN ["a", "b"]` clause with quoted, escaped values.
fn meili_in<'a>(attr: &str, values: impl Iterator<Item = &'a str>) -> String {
    let rendered = values
        .map(|v| format!("\"{}\"", v.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{attr} IN [{rendered}]")
}

/// Sort order for search results. Magnitude is not indexed, so it is not an
/// option here (the API maps a magnitude sort to relevance/recency).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchSort {
    /// Default Meili relevance ranking (keyword match quality).
    #[default]
    Relevance,
    /// Most recent first.
    Recent,
    /// Highest impact first.
    ImpactDesc,
}

impl SearchSort {
    fn rules(self) -> &'static [&'static str] {
        match self {
            SearchSort::Relevance => &[],
            SearchSort::Recent => &["occurred_at:desc"],
            SearchSort::ImpactDesc => &["impact_score:desc"],
        }
    }
}

/// Run a tenant-scoped full-text search with optional attribute filters.
pub async fn search_events(
    client: &Client,
    tenant_id: Uuid,
    query: &str,
    filters: &SearchFilters,
    sort: SearchSort,
    limit: usize,
    offset: usize,
) -> Result<SearchResults> {
    let filter = filters.to_expression(tenant_id);
    let index = client.index(EVENTS_INDEX);
    let mut search = index.search();
    search
        .with_query(query)
        .with_filter(&filter)
        .with_limit(limit)
        .with_offset(offset);
    let sort_rules = sort.rules();
    if !sort_rules.is_empty() {
        search.with_sort(sort_rules);
    }
    let results = search
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
            occurred_at_unix: event.occurred_at.timestamp(),
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
    #![allow(clippy::unwrap_used, clippy::expect_used)]

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
        assert_eq!(doc.occurred_at_unix, now.timestamp());
    }

    #[test]
    fn filter_expression_uses_in_and_numeric_time() {
        let tenant_id = Uuid::new_v4();
        let after = chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("ts");
        let filters = SearchFilters {
            categories: vec![Category::Earthquake, Category::Weather],
            severities: vec![],
            sources: vec!["usgs".to_owned()],
            impact_min: Some(20),
            impact_max: Some(90),
            occurred_after: Some(after),
            occurred_before: None,
        };
        let expr = filters.to_expression(tenant_id);
        assert!(expr.contains("tenant_id ="), "{expr}");
        assert!(
            expr.contains("category IN [\"earthquake\", \"weather\"]"),
            "{expr}"
        );
        assert!(expr.contains("source IN [\"usgs\"]"), "{expr}");
        assert!(expr.contains("impact_score >= 20"), "{expr}");
        assert!(expr.contains("impact_score <= 90"), "{expr}");
        assert!(expr.contains("occurred_at_unix >= 1700000000"), "{expr}");
    }
}
