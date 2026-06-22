//! Idempotent upsert and tenant-scoped reads of canonical [`Event`] rows.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::QueryBuilder;
use uuid::Uuid;

use crate::events::{Category, Event, EventStatus, GeoPoint, Severity, VerificationStatus};
use crate::Result;

/// Geographic bounding box filter (WGS84 degrees).
#[derive(Debug, Clone, Copy)]
pub struct EventBBox {
    /// Western longitude.
    pub min_lon: f64,
    /// Southern latitude.
    pub min_lat: f64,
    /// Eastern longitude.
    pub max_lon: f64,
    /// Northern latitude.
    pub max_lat: f64,
}

/// Tenant-scoped list filters. `tenant_id` is always enforced.
#[derive(Debug, Clone)]
pub struct EventListFilter {
    /// Authenticated tenant — required on every query.
    pub tenant_id: Uuid,
    /// Optional viewport bounding box.
    pub bbox: Option<EventBBox>,
    /// Optional category filter.
    pub category: Option<Category>,
    /// Optional severity filter.
    pub severity: Option<Severity>,
    /// Include events at or after this time.
    pub occurred_after: Option<DateTime<Utc>>,
    /// Include events at or before this time.
    pub occurred_before: Option<DateTime<Utc>>,
    /// Include only events with `impact_score` at or above this threshold (0–100).
    pub min_impact: Option<u8>,
    /// Page size (clamped by caller).
    pub limit: i64,
    /// Pagination offset.
    pub offset: i64,
}

/// Insert or update an event keyed on `(tenant_id, source, source_event_id, occurred_at)`.
pub async fn upsert_event(pool: &PgPool, event: &Event) -> Result<()> {
    let embedding = event
        .embedding
        .as_ref()
        .map(|values| format_pgvector(values.as_slice()));

    sqlx::query(
        r#"
        INSERT INTO events (
            id, tenant_id, source, source_event_id, category, severity, impact_score,
            magnitude, title, summary, body, original_text, translated_text, language,
            location, country, region, place_name,
            occurred_at, detected_at, ingested_at,
            status, verification_status, confidence,
            tags, url, raw, embedding
        ) VALUES (
            $1, $2, $3, $4,
            $5::event_category, $6::event_severity, $7,
            $8, $9, $10, $11, $12, $13, $14,
            ST_SetSRID(ST_MakePoint($15, $16), 4326)::geography,
            $17, $18, $19,
            $20, $21, $22,
            $23::event_status, $24::verification_status, $25,
            $26, $27, $28::jsonb, $29::vector
        )
        ON CONFLICT (tenant_id, source, source_event_id, occurred_at) DO UPDATE SET
            category = EXCLUDED.category,
            severity = EXCLUDED.severity,
            impact_score = EXCLUDED.impact_score,
            magnitude = EXCLUDED.magnitude,
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            body = EXCLUDED.body,
            original_text = EXCLUDED.original_text,
            translated_text = EXCLUDED.translated_text,
            language = EXCLUDED.language,
            location = EXCLUDED.location,
            country = EXCLUDED.country,
            region = EXCLUDED.region,
            place_name = EXCLUDED.place_name,
            detected_at = EXCLUDED.detected_at,
            ingested_at = EXCLUDED.ingested_at,
            status = EXCLUDED.status,
            verification_status = EXCLUDED.verification_status,
            confidence = EXCLUDED.confidence,
            tags = EXCLUDED.tags,
            url = EXCLUDED.url,
            raw = EXCLUDED.raw,
            embedding = EXCLUDED.embedding
        "#,
    )
    .bind(event.id)
    .bind(event.tenant_id)
    .bind(&event.source)
    .bind(&event.source_event_id)
    .bind(pg_category(event.category))
    .bind(pg_severity(event.severity))
    .bind(i16::from(event.impact_score))
    .bind(event.magnitude)
    .bind(&event.title)
    .bind(&event.summary)
    .bind(&event.body)
    .bind(&event.original_text)
    .bind(&event.translated_text)
    .bind(&event.language)
    .bind(event.location.lon)
    .bind(event.location.lat)
    .bind(&event.country)
    .bind(&event.region)
    .bind(&event.place_name)
    .bind(event.occurred_at)
    .bind(event.detected_at)
    .bind(event.ingested_at)
    .bind(pg_status(event.status))
    .bind(pg_verification(event.verification_status))
    .bind(event.confidence)
    .bind(&event.tags)
    .bind(&event.url)
    .bind(&event.raw)
    .bind(embedding)
    .execute(pool)
    .await?;

    if let Err(err) = super::notify::notify_event_upsert(pool, event.tenant_id, event.id).await {
        tracing::warn!(
            event_id = %event.id,
            tenant_id = %event.tenant_id,
            error = %err,
            "failed to emit event upsert notification"
        );
    }

    Ok(())
}

/// Build a multi-row backfill upsert for one batch of events.
///
/// Rows are assembled by hand rather than with [`QueryBuilder::push_values`]:
/// the `Separated` builder used by `push_values` injects a comma before every
/// `push`, which corrupts the `::type` casts (producing `$1, ::event_category`
/// and a SQL syntax error). Bind values are encoded into the builder eagerly,
/// so per-row temporaries (e.g. the formatted embedding) need not outlive it.
fn build_backfill_upsert(batch: &[Event]) -> QueryBuilder<'_, sqlx::Postgres> {
    let mut builder = QueryBuilder::new(
        r#"
        INSERT INTO events (
            id, tenant_id, source, source_event_id, category, severity, impact_score,
            magnitude, title, summary, body, original_text, translated_text, language,
            location, country, region, place_name,
            occurred_at, detected_at, ingested_at,
            status, verification_status, confidence,
            tags, url, raw, embedding
        ) VALUES "#,
    );

    for (i, event) in batch.iter().enumerate() {
        if i > 0 {
            builder.push(", ");
        }
        let embedding = event
            .embedding
            .as_ref()
            .map(|values| format_pgvector(values.as_slice()));
        builder.push("(");
        builder.push_bind(event.id);
        builder.push(", ");
        builder.push_bind(event.tenant_id);
        builder.push(", ");
        builder.push_bind(&event.source);
        builder.push(", ");
        builder.push_bind(&event.source_event_id);
        builder.push(", ");
        builder.push_bind(pg_category(event.category));
        builder.push("::event_category, ");
        builder.push_bind(pg_severity(event.severity));
        builder.push("::event_severity, ");
        builder.push_bind(i16::from(event.impact_score));
        builder.push(", ");
        builder.push_bind(event.magnitude);
        builder.push(", ");
        builder.push_bind(&event.title);
        builder.push(", ");
        builder.push_bind(&event.summary);
        builder.push(", ");
        builder.push_bind(&event.body);
        builder.push(", ");
        builder.push_bind(&event.original_text);
        builder.push(", ");
        builder.push_bind(&event.translated_text);
        builder.push(", ");
        builder.push_bind(&event.language);
        builder.push(", ST_SetSRID(ST_MakePoint(");
        builder.push_bind(event.location.lon);
        builder.push(", ");
        builder.push_bind(event.location.lat);
        builder.push("), 4326)::geography, ");
        builder.push_bind(&event.country);
        builder.push(", ");
        builder.push_bind(&event.region);
        builder.push(", ");
        builder.push_bind(&event.place_name);
        builder.push(", ");
        builder.push_bind(event.occurred_at);
        builder.push(", ");
        builder.push_bind(event.detected_at);
        builder.push(", ");
        builder.push_bind(event.ingested_at);
        builder.push(", ");
        builder.push_bind(pg_status(event.status));
        builder.push("::event_status, ");
        builder.push_bind(pg_verification(event.verification_status));
        builder.push("::verification_status, ");
        builder.push_bind(event.confidence);
        builder.push(", ");
        builder.push_bind(&event.tags);
        builder.push(", ");
        builder.push_bind(&event.url);
        builder.push(", ");
        builder.push_bind(&event.raw);
        builder.push("::jsonb, ");
        builder.push_bind(embedding);
        builder.push("::vector)");
    }

    builder.push(
        r#"
        ON CONFLICT (tenant_id, source, source_event_id, occurred_at) DO UPDATE SET
            category = EXCLUDED.category,
            severity = EXCLUDED.severity,
            impact_score = EXCLUDED.impact_score,
            magnitude = EXCLUDED.magnitude,
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            body = EXCLUDED.body,
            original_text = EXCLUDED.original_text,
            translated_text = EXCLUDED.translated_text,
            language = EXCLUDED.language,
            location = EXCLUDED.location,
            country = EXCLUDED.country,
            region = EXCLUDED.region,
            place_name = EXCLUDED.place_name,
            detected_at = EXCLUDED.detected_at,
            ingested_at = EXCLUDED.ingested_at,
            status = EXCLUDED.status,
            verification_status = EXCLUDED.verification_status,
            confidence = EXCLUDED.confidence,
            tags = EXCLUDED.tags,
            url = EXCLUDED.url,
            raw = EXCLUDED.raw,
            embedding = EXCLUDED.embedding
        "#,
    );

    builder
}

/// Batch upsert for backfill: one round-trip per batch, no live-stream NOTIFY.
///
/// Returns the number of rows written (all events in the batch on success).
pub async fn upsert_events_backfill(pool: &PgPool, events: &[Event]) -> Result<usize> {
    if events.is_empty() {
        return Ok(0);
    }

    const BATCH_SIZE: usize = 50;
    let mut total = 0usize;

    for batch in events.chunks(BATCH_SIZE) {
        build_backfill_upsert(batch).build().execute(pool).await?;
        total += batch.len();
    }

    Ok(total)
}

/// List events for one tenant with optional filters.
pub async fn list_events(pool: &PgPool, filter: &EventListFilter) -> Result<Vec<Event>> {
    let mut builder = sqlx::QueryBuilder::new(
        r#"
        SELECT
            id, tenant_id, source, source_event_id,
            category::text AS category, severity::text AS severity, impact_score,
            magnitude, title, summary, body, original_text, translated_text, language,
            ST_X(location::geometry) AS lon, ST_Y(location::geometry) AS lat,
            country, region, place_name,
            occurred_at, detected_at, ingested_at,
            status::text AS status, verification_status::text AS verification_status,
            confidence, tags, url, raw
        FROM events
        WHERE tenant_id = "#,
    );
    builder.push_bind(filter.tenant_id);

    if let Some(bbox) = filter.bbox {
        builder.push(" AND ST_Intersects(location::geometry, ST_MakeEnvelope(");
        builder.push_bind(bbox.min_lon);
        builder.push(", ");
        builder.push_bind(bbox.min_lat);
        builder.push(", ");
        builder.push_bind(bbox.max_lon);
        builder.push(", ");
        builder.push_bind(bbox.max_lat);
        builder.push(", 4326))");
    }

    if let Some(category) = filter.category {
        builder.push(" AND category = ");
        builder.push_bind(pg_category(category));
        builder.push("::event_category");
    }

    if let Some(severity) = filter.severity {
        builder.push(" AND severity = ");
        builder.push_bind(pg_severity(severity));
        builder.push("::event_severity");
    }

    if let Some(after) = filter.occurred_after {
        builder.push(" AND occurred_at >= ");
        builder.push_bind(after);
    }

    if let Some(before) = filter.occurred_before {
        builder.push(" AND occurred_at <= ");
        builder.push_bind(before);
    }

    if let Some(min_impact) = filter.min_impact {
        builder.push(" AND impact_score >= ");
        builder.push_bind(i16::from(min_impact));
    }

    builder.push(" ORDER BY occurred_at DESC LIMIT ");
    builder.push_bind(filter.limit);
    builder.push(" OFFSET ");
    builder.push_bind(filter.offset);

    let rows = builder.build_query_as::<EventRow>().fetch_all(pool).await?;
    rows.into_iter()
        .map(EventRow::into_event)
        .collect::<Result<Vec<_>>>()
}

/// Fetch one event by id within a tenant, or `None` if missing / wrong tenant.
pub async fn get_event(pool: &PgPool, tenant_id: Uuid, event_id: Uuid) -> Result<Option<Event>> {
    let row = sqlx::query_as::<_, EventRow>(
        r#"
        SELECT
            id, tenant_id, source, source_event_id,
            category::text AS category, severity::text AS severity, impact_score,
            magnitude, title, summary, body, original_text, translated_text, language,
            ST_X(location::geometry) AS lon, ST_Y(location::geometry) AS lat,
            country, region, place_name,
            occurred_at, detected_at, ingested_at,
            status::text AS status, verification_status::text AS verification_status,
            confidence, tags, url, raw
        FROM events
        WHERE tenant_id = $1 AND id = $2
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.into_event()).transpose()
}

#[derive(sqlx::FromRow)]
struct EventRow {
    id: Uuid,
    tenant_id: Uuid,
    source: String,
    source_event_id: String,
    category: String,
    severity: String,
    impact_score: i16,
    magnitude: Option<f64>,
    title: Option<String>,
    summary: Option<String>,
    body: Option<String>,
    original_text: Option<String>,
    translated_text: Option<String>,
    language: Option<String>,
    lon: f64,
    lat: f64,
    country: Option<String>,
    region: Option<String>,
    place_name: Option<String>,
    occurred_at: DateTime<Utc>,
    detected_at: Option<DateTime<Utc>>,
    ingested_at: DateTime<Utc>,
    status: String,
    verification_status: String,
    confidence: f32,
    tags: Vec<String>,
    url: Option<String>,
    raw: serde_json::Value,
}

impl EventRow {
    fn into_event(self) -> Result<Event> {
        Ok(Event {
            id: self.id,
            tenant_id: self.tenant_id,
            source: self.source,
            source_event_id: self.source_event_id,
            category: parse_category(&self.category)?,
            severity: parse_severity(&self.severity)?,
            impact_score: u8::try_from(self.impact_score)
                .map_err(|_| crate::error::AppError::internal("invalid impact_score"))?,
            magnitude: self.magnitude,
            title: self.title,
            summary: self.summary,
            body: self.body,
            original_text: self.original_text,
            translated_text: self.translated_text,
            language: self.language,
            location: GeoPoint {
                lon: self.lon,
                lat: self.lat,
            },
            affected_area: None,
            country: self.country,
            region: self.region,
            place_name: self.place_name,
            occurred_at: self.occurred_at,
            detected_at: self.detected_at,
            ingested_at: self.ingested_at,
            status: parse_status(&self.status)?,
            verification_status: parse_verification(&self.verification_status)?,
            confidence: self.confidence,
            tags: self.tags,
            url: self.url,
            raw: self.raw,
            embedding: None,
        })
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
            "unknown category: {value}"
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
            "unknown severity: {value}"
        ))),
    }
}

fn parse_status(value: &str) -> Result<EventStatus> {
    match value {
        "active" => Ok(EventStatus::Active),
        "resolved" => Ok(EventStatus::Resolved),
        "archived" => Ok(EventStatus::Archived),
        _ => Err(crate::error::AppError::internal(format!(
            "unknown status: {value}"
        ))),
    }
}

fn parse_verification(value: &str) -> Result<VerificationStatus> {
    match value {
        "verified" => Ok(VerificationStatus::Verified),
        "unverified" => Ok(VerificationStatus::Unverified),
        "rumor" => Ok(VerificationStatus::Rumor),
        "disputed" => Ok(VerificationStatus::Disputed),
        _ => Err(crate::error::AppError::internal(format!(
            "unknown verification_status: {value}"
        ))),
    }
}

fn pg_category(value: Category) -> &'static str {
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

fn pg_severity(value: Severity) -> &'static str {
    match value {
        Severity::Info => "info",
        Severity::Low => "low",
        Severity::Moderate => "moderate",
        Severity::High => "high",
        Severity::Critical => "critical",
    }
}

fn pg_status(value: EventStatus) -> &'static str {
    match value {
        EventStatus::Active => "active",
        EventStatus::Resolved => "resolved",
        EventStatus::Archived => "archived",
    }
}

fn pg_verification(value: VerificationStatus) -> &'static str {
    match value {
        VerificationStatus::Verified => "verified",
        VerificationStatus::Unverified => "unverified",
        VerificationStatus::Rumor => "rumor",
        VerificationStatus::Disputed => "disputed",
    }
}

fn format_pgvector(values: &[f32]) -> String {
    let body = values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> Event {
        let now = Utc::now();
        Event {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            source: "usgs".to_owned(),
            source_event_id: "abc123".to_owned(),
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
            raw: serde_json::json!({"k": "v"}),
            embedding: None,
        }
    }

    #[test]
    fn pg_enum_strings_match_migration() {
        assert_eq!(pg_category(Category::Earthquake), "earthquake");
        assert_eq!(pg_severity(Severity::Critical), "critical");
        assert_eq!(pg_status(EventStatus::Active), "active");
        assert_eq!(
            pg_verification(VerificationStatus::Unverified),
            "unverified"
        );
    }

    // Regression: the batch upsert previously used `QueryBuilder::push_values`,
    // whose `Separated` builder inserts a comma before every `push`, turning the
    // `::type` casts into `$n, ::event_category` and causing a Postgres syntax
    // error ("syntax error at or near \"::\""). Backfill silently wrote 0 rows.
    #[test]
    fn backfill_upsert_sql_casts_are_not_comma_separated() {
        let batch = vec![sample_event(), sample_event()];
        let builder = build_backfill_upsert(&batch);
        let sql = builder.sql().to_owned();

        // The corrupting pattern must never appear.
        assert!(
            !sql.contains(", ::"),
            "casts must attach to their bind, not be separate columns: {sql}"
        );
        // Casts attach directly to the preceding placeholder.
        assert!(
            sql.contains("::event_category, "),
            "missing category cast: {sql}"
        );
        assert!(
            sql.contains("::event_severity, "),
            "missing severity cast: {sql}"
        );
        assert!(
            sql.contains(")::geography, "),
            "missing geography cast: {sql}"
        );
        assert!(
            sql.contains("::event_status, "),
            "missing status cast: {sql}"
        );
        assert!(
            sql.contains("::verification_status, "),
            "missing verification cast: {sql}"
        );
        assert!(sql.contains("::jsonb, "), "missing jsonb cast: {sql}");
        // Two rows -> two value tuples, each terminated by the vector cast.
        assert_eq!(
            sql.matches("::vector)").count(),
            2,
            "expected two value rows: {sql}"
        );
        assert!(sql.contains("ON CONFLICT (tenant_id, source, source_event_id, occurred_at)"));
    }
}
