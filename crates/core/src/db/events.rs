//! Idempotent upsert and tenant-scoped reads of canonical [`Event`] rows.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::QueryBuilder;
use uuid::Uuid;

use crate::events::{Category, Event, EventStatus, GeoPoint, Severity, VerificationStatus};
use crate::tenancy::SYSTEM_TENANT_ID;
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

/// Result ordering for [`list_events`]. Nulls always sort last so events
/// missing the sort key do not crowd out scored/measured ones. Wire values
/// match the frontend: `recent`, `impact_desc`, `magnitude_desc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSort {
    /// Most recent first (`occurred_at DESC`). Default.
    #[default]
    Recent,
    /// Highest `impact_score` first (nulls last).
    ImpactDesc,
    /// Highest `magnitude` first (nulls last).
    MagnitudeDesc,
}

/// Tenant-scoped list filters. Queries return the caller's tenant rows plus
/// the shared [`SYSTEM_TENANT_ID`] public feeds (USGS, NWS, …); private
/// per-tenant data stays isolated (`tenant-isolation.mdc`).
///
/// Multi-value filters (`categories`, `severities`, `sources`) are conjunctive
/// with the rest of the filter but disjunctive within themselves (an empty vec
/// means "no constraint on this dimension").
#[derive(Debug, Clone)]
pub struct EventListFilter {
    /// Authenticated tenant — required on every query.
    pub tenant_id: Uuid,
    /// Optional viewport bounding box.
    pub bbox: Option<EventBBox>,
    /// Categories to include; empty = all categories.
    pub categories: Vec<Category>,
    /// Severities to include; empty = all severities.
    pub severities: Vec<Severity>,
    /// Source keys to include; empty = all sources.
    pub sources: Vec<String>,
    /// Include events at or after this time.
    pub occurred_after: Option<DateTime<Utc>>,
    /// Include events at or before this time.
    pub occurred_before: Option<DateTime<Utc>>,
    /// Include only events with `impact_score` at or above this threshold (0–100).
    pub impact_min: Option<u8>,
    /// Include only events with `impact_score` at or below this threshold (0–100).
    pub impact_max: Option<u8>,
    /// Include only events with `magnitude` at or above this value.
    /// An active bound also implies `magnitude IS NOT NULL`.
    pub min_magnitude: Option<f64>,
    /// Include only events with `magnitude` at or below this value.
    /// An active bound also implies `magnitude IS NOT NULL`.
    pub max_magnitude: Option<f64>,
    /// Result ordering.
    pub sort: EventSort,
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

/// Paginated event list plus the total number of rows matching the filter
/// (before `LIMIT`/`OFFSET`), so clients can show "X of Y" and paginate correctly.
#[derive(Debug, Clone)]
pub struct EventListResult {
    /// Events for the requested page.
    pub items: Vec<Event>,
    /// Total matching rows for the same filter (ignoring pagination).
    pub total: i64,
}

/// Lightweight map coordinate for globe heat/dots (no body, raw, or embedding).
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct EventMapPoint {
    /// Event id.
    pub id: Uuid,
    /// Canonical category (snake_case string).
    pub category: String,
    /// Severity tier (snake_case string).
    pub severity: String,
    /// Impact score 0–100.
    pub impact_score: i16,
    /// Optional source magnitude.
    pub magnitude: Option<f64>,
    /// WGS84 latitude.
    pub lat: f64,
    /// WGS84 longitude.
    pub lon: f64,
    /// When the event occurred.
    pub occurred_at: DateTime<Utc>,
}

/// Globe map payload: compact points plus the full matching count.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EventMapResult {
    /// Map points returned (may be capped by `limit`).
    pub points: Vec<EventMapPoint>,
    /// Total rows matching the filter (ignoring pagination). `0` when the count
    /// was skipped (`with_count = false`), e.g. on non-first globe batches.
    pub total: i64,
    /// Applied page size cap.
    pub limit: i64,
}

/// List events for one tenant with optional filters.
pub async fn list_events(pool: &PgPool, filter: &EventListFilter) -> Result<EventListResult> {
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
            confidence, tags, url, raw,
            COUNT(*) OVER() AS total_count
        FROM events
        WHERE tenant_id IN ("#,
    );
    // Caller's own tenant plus the shared public-feed (system) tenant.
    builder.push_bind(filter.tenant_id);
    builder.push(", ");
    builder.push_bind(SYSTEM_TENANT_ID);
    builder.push(")");

    push_event_filter_predicates(&mut builder, filter);
    push_event_order(&mut builder, filter);
    builder.push(" LIMIT ");
    builder.push_bind(filter.limit);
    builder.push(" OFFSET ");
    builder.push_bind(filter.offset);

    let rows = builder
        .build_query_as::<EventListRow>()
        .fetch_all(pool)
        .await?;
    let total = rows.first().map(|row| row.total_count).unwrap_or(0);
    let items = rows
        .into_iter()
        .map(EventListRow::into_event)
        .collect::<Result<Vec<_>>>()?;
    Ok(EventListResult { items, total })
}

/// Count events matching the filter (same tenant scope and predicates as [`list_events`]).
pub async fn count_events(pool: &PgPool, filter: &EventListFilter) -> Result<i64> {
    let mut builder =
        sqlx::QueryBuilder::new("SELECT COUNT(*)::bigint FROM events WHERE tenant_id IN (");
    builder.push_bind(filter.tenant_id);
    builder.push(", ");
    builder.push_bind(SYSTEM_TENANT_ID);
    builder.push(")");
    push_event_filter_predicates(&mut builder, filter);
    builder
        .build_query_scalar::<i64>()
        .fetch_one(pool)
        .await
        .map_err(Into::into)
}

/// Compact map coordinates for the globe (high limit, minimal columns).
///
/// `with_count` controls whether the full matching `total` is computed. The
/// globe loads points in paginated batches but only needs the total once (first
/// batch), so callers pass `false` on subsequent batches to skip the extra
/// `COUNT(*)` scan; `total` is then `0`.
pub async fn list_event_map_points(
    pool: &PgPool,
    filter: &EventListFilter,
    with_count: bool,
) -> Result<EventMapResult> {
    let total = if with_count {
        count_events(pool, filter).await?
    } else {
        0
    };
    let mut builder = sqlx::QueryBuilder::new(
        r#"
        SELECT
            id,
            category::text AS category,
            severity::text AS severity,
            impact_score,
            magnitude,
            ST_Y(location::geometry) AS lat,
            ST_X(location::geometry) AS lon,
            occurred_at
        FROM events
        WHERE tenant_id IN ("#,
    );
    builder.push_bind(filter.tenant_id);
    builder.push(", ");
    builder.push_bind(SYSTEM_TENANT_ID);
    builder.push(")");

    push_event_filter_predicates(&mut builder, filter);
    push_event_order(&mut builder, filter);
    builder.push(" LIMIT ");
    builder.push_bind(filter.limit);
    builder.push(" OFFSET ");
    builder.push_bind(filter.offset);

    let points = builder
        .build_query_as::<EventMapPoint>()
        .fetch_all(pool)
        .await?;
    Ok(EventMapResult {
        points,
        total,
        limit: filter.limit,
    })
}

/// Append bbox/dimension/range predicates to a builder whose
/// `WHERE tenant_id IN (...)` clause is already in place.
fn push_event_filter_predicates(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    filter: &EventListFilter,
) {
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

    if !filter.categories.is_empty() {
        // category = ANY($n::event_category[]) — disjunctive within the dimension.
        let values: Vec<&'static str> = filter.categories.iter().map(|c| pg_category(*c)).collect();
        builder.push(" AND category = ANY(");
        builder.push_bind(values);
        builder.push("::event_category[])");
    }

    if !filter.severities.is_empty() {
        let values: Vec<&'static str> = filter.severities.iter().map(|s| pg_severity(*s)).collect();
        builder.push(" AND severity = ANY(");
        builder.push_bind(values);
        builder.push("::event_severity[])");
    }

    if !filter.sources.is_empty() {
        builder.push(" AND source = ANY(");
        builder.push_bind(filter.sources.clone());
        builder.push(")");
    }

    if let Some(after) = filter.occurred_after {
        builder.push(" AND occurred_at >= ");
        builder.push_bind(after);
    }

    if let Some(before) = filter.occurred_before {
        builder.push(" AND occurred_at <= ");
        builder.push_bind(before);
    }

    if let Some(impact_min) = filter.impact_min {
        builder.push(" AND impact_score >= ");
        builder.push_bind(i16::from(impact_min));
    }

    if let Some(impact_max) = filter.impact_max {
        builder.push(" AND impact_score <= ");
        builder.push_bind(i16::from(impact_max));
    }

    // An active magnitude bound implies the event has a magnitude at all.
    if filter.min_magnitude.is_some() || filter.max_magnitude.is_some() {
        builder.push(" AND magnitude IS NOT NULL");
    }
    if let Some(min_magnitude) = filter.min_magnitude {
        builder.push(" AND magnitude >= ");
        builder.push_bind(min_magnitude);
    }
    if let Some(max_magnitude) = filter.max_magnitude {
        builder.push(" AND magnitude <= ");
        builder.push_bind(max_magnitude);
    }
}

/// Append `ORDER BY` for list/map queries (deterministic tie-break on `occurred_at`).
fn push_event_order(
    builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    filter: &EventListFilter,
) {
    // occurred_at stays in the ORDER BY (even for other sorts) to keep results
    // deterministic and partition-pruning friendly (geospatial-postgis.mdc).
    match filter.sort {
        EventSort::Recent => builder.push(" ORDER BY occurred_at DESC"),
        EventSort::ImpactDesc => {
            builder.push(" ORDER BY impact_score DESC NULLS LAST, occurred_at DESC")
        }
        EventSort::MagnitudeDesc => {
            builder.push(" ORDER BY magnitude DESC NULLS LAST, occurred_at DESC")
        }
    };
}

/// Distinct `source` values present in the caller's visible events (own tenant
/// plus the shared public-feed tenant). Drives the dynamic source filter so it
/// always reflects real data (including `manual` and unregistered sources the
/// global `sources` catalog would miss). Backed by `events_tenant_source_idx`.
pub async fn list_event_sources(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<String>> {
    let sources = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT source
        FROM events
        WHERE tenant_id IN ($1, $2)
        ORDER BY source
        "#,
    )
    .bind(tenant_id)
    .bind(SYSTEM_TENANT_ID)
    .fetch_all(pool)
    .await?;
    Ok(sources)
}

/// Fetch one event by id, visible to the caller's tenant or the shared public
/// (system) tenant; returns `None` for missing or other tenants' private rows.
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
        WHERE tenant_id IN ($1, $3) AND id = $2
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(event_id)
    .bind(SYSTEM_TENANT_ID)
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.into_event()).transpose()
}

#[derive(sqlx::FromRow)]
struct EventListRow {
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
    total_count: i64,
}

impl EventListRow {
    fn into_event(self) -> Result<Event> {
        EventRow {
            id: self.id,
            tenant_id: self.tenant_id,
            source: self.source,
            source_event_id: self.source_event_id,
            category: self.category,
            severity: self.severity,
            impact_score: self.impact_score,
            magnitude: self.magnitude,
            title: self.title,
            summary: self.summary,
            body: self.body,
            original_text: self.original_text,
            translated_text: self.translated_text,
            language: self.language,
            lon: self.lon,
            lat: self.lat,
            country: self.country,
            region: self.region,
            place_name: self.place_name,
            occurred_at: self.occurred_at,
            detected_at: self.detected_at,
            ingested_at: self.ingested_at,
            status: self.status,
            verification_status: self.verification_status,
            confidence: self.confidence,
            tags: self.tags,
            url: self.url,
            raw: self.raw,
        }
        .into_event()
    }
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

    fn base_filter() -> EventListFilter {
        EventListFilter {
            tenant_id: Uuid::new_v4(),
            bbox: None,
            categories: vec![],
            severities: vec![],
            sources: vec![],
            occurred_after: None,
            occurred_before: None,
            impact_min: None,
            impact_max: None,
            min_magnitude: None,
            max_magnitude: None,
            sort: EventSort::Recent,
            limit: 50,
            offset: 0,
        }
    }

    // Build just the SQL string (no DB) to assert the query shape per filter.
    fn list_sql(filter: &EventListFilter) -> String {
        let mut builder =
            sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 1 FROM events WHERE tenant_id IN (");
        builder.push_bind(filter.tenant_id);
        builder.push(", ");
        builder.push_bind(SYSTEM_TENANT_ID);
        builder.push(")");
        push_event_filter_predicates(&mut builder, filter);
        push_event_order(&mut builder, filter);
        builder.push(" LIMIT ");
        builder.push_bind(filter.limit);
        builder.push(" OFFSET ");
        builder.push_bind(filter.offset);
        builder.sql().to_owned()
    }

    #[test]
    fn empty_multi_filters_add_no_predicate() {
        let sql = list_sql(&base_filter());
        assert!(
            !sql.contains("ANY("),
            "empty vecs must not emit ANY(): {sql}"
        );
        assert!(sql.contains("ORDER BY occurred_at DESC"));
    }

    #[test]
    fn multi_value_filters_use_any() {
        let mut f = base_filter();
        f.categories = vec![Category::Earthquake, Category::Weather];
        f.severities = vec![Severity::High];
        f.sources = vec!["usgs".to_owned()];
        let sql = list_sql(&f);
        assert!(sql.contains("category = ANY("), "{sql}");
        assert!(sql.contains("severity = ANY("), "{sql}");
        assert!(sql.contains("source = ANY("), "{sql}");
    }

    #[test]
    fn magnitude_bound_implies_not_null() {
        let mut f = base_filter();
        f.min_magnitude = Some(4.0);
        let sql = list_sql(&f);
        assert!(sql.contains("magnitude IS NOT NULL"), "{sql}");
        assert!(sql.contains("magnitude >= "), "{sql}");
    }

    #[test]
    fn sort_modes_use_nulls_last() {
        let mut f = base_filter();
        f.sort = EventSort::ImpactDesc;
        assert!(list_sql(&f).contains("impact_score DESC NULLS LAST"));
        f.sort = EventSort::MagnitudeDesc;
        assert!(list_sql(&f).contains("magnitude DESC NULLS LAST"));
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
