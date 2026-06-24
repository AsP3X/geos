//! Tenant-scoped event read endpoints.

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use geos_core::db::{
    get_event, list_event_map_points, list_event_sources, list_events, EventBBox, EventListFilter,
    EventMapResult, EventSort,
};
use geos_core::events::{Category, Event, Severity};
use geos_core::rbac::Permission;
use geos_core::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::middleware::auth::AuthContext;
use crate::state::AppState;

/// Query parameters for `GET /api/v1/events`.
///
/// Multi-value dimensions accept comma-separated lists (`category=earthquake,weather`);
/// a single value still parses (backward compatible). `min_impact` is retained as an
/// alias for `impact_min`.
#[derive(Debug, Deserialize)]
pub struct ListEventsQuery {
    /// Minimum longitude (bbox).
    pub min_lon: Option<f64>,
    /// Minimum latitude (bbox).
    pub min_lat: Option<f64>,
    /// Maximum longitude (bbox).
    pub max_lon: Option<f64>,
    /// Maximum latitude (bbox).
    pub max_lat: Option<f64>,
    /// Comma-separated categories (`earthquake`, `weather`, …).
    pub category: Option<String>,
    /// Comma-separated severity tiers.
    pub severity: Option<String>,
    /// Comma-separated source keys.
    pub source: Option<String>,
    /// ISO-8601 lower bound on `occurred_at`.
    pub occurred_after: Option<DateTime<Utc>>,
    /// ISO-8601 upper bound on `occurred_at`.
    pub occurred_before: Option<DateTime<Utc>>,
    /// Minimum impact score (0–100).
    pub impact_min: Option<i64>,
    /// Maximum impact score (0–100).
    pub impact_max: Option<i64>,
    /// Back-compat alias for `impact_min`; ignored when `impact_min` is present.
    pub min_impact: Option<i64>,
    /// Minimum magnitude (events without a magnitude are excluded).
    pub min_magnitude: Option<f64>,
    /// Maximum magnitude (events without a magnitude are excluded).
    pub max_magnitude: Option<f64>,
    /// Sort order: `recent` (default), `impact_desc`, `magnitude_desc`.
    pub sort: Option<String>,
    /// Page size (default 50, max 200).
    pub limit: Option<i64>,
    /// Pagination offset.
    pub offset: Option<i64>,
}

/// Paginated list response wrapper.
#[derive(Debug, Serialize)]
pub struct EventListResponse {
    /// Matching events for the authenticated tenant.
    pub items: Vec<Event>,
    /// Total rows matching the filter (ignoring pagination).
    pub total: i64,
    /// Applied page size.
    pub limit: i64,
    /// Applied offset.
    pub offset: i64,
}

/// Maximum points returned by `GET /api/v1/events/map` (globe visualization).
const MAP_MAX_LIMIT: i64 = 100_000;

/// Distinct-source response for `GET /api/v1/events/sources`.
#[derive(Debug, Serialize)]
pub struct EventSourcesResponse {
    /// Distinct source keys present in the caller's visible events.
    pub sources: Vec<String>,
}

/// `GET /api/v1/events` — list events for the authenticated tenant.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<EventListResponse>, ApiError> {
    auth.require_permission(Permission::EventsRead)?;

    let bbox = parse_bbox(query.min_lon, query.min_lat, query.max_lon, query.max_lat)?;
    let impact_min = parse_impact(query.impact_min.or(query.min_impact), "impact_min")?;
    let impact_max = parse_impact(query.impact_max, "impact_max")?;
    if let (Some(min), Some(max)) = (impact_min, impact_max) {
        if min > max {
            return Err(AppError::bad_request("impact_min must be <= impact_max").into());
        }
    }
    let (min_magnitude, max_magnitude) =
        parse_magnitude_range(query.min_magnitude, query.max_magnitude)?;

    let categories = parse_categories(query.category.as_deref())?;
    let severities = parse_severities(query.severity.as_deref())?;
    let sources = parse_sources(query.source.as_deref());
    let sort = parse_sort(query.sort.as_deref())?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let page = list_events(
        &state.pool,
        &EventListFilter {
            tenant_id: auth.tenant_id,
            bbox,
            categories,
            severities,
            sources,
            occurred_after: query.occurred_after,
            occurred_before: query.occurred_before,
            impact_min,
            impact_max,
            min_magnitude,
            max_magnitude,
            sort,
            limit,
            offset,
        },
    )
    .await?;

    Ok(Json(EventListResponse {
        items: page.items,
        total: page.total,
        limit,
        offset,
    }))
}

/// `GET /api/v1/events/map` — compact coordinates for globe layers (high limit).
pub async fn map(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<EventMapResult>, ApiError> {
    auth.require_permission(Permission::EventsRead)?;

    let bbox = parse_bbox(query.min_lon, query.min_lat, query.max_lon, query.max_lat)?;
    let impact_min = parse_impact(query.impact_min.or(query.min_impact), "impact_min")?;
    let impact_max = parse_impact(query.impact_max, "impact_max")?;
    if let (Some(min), Some(max)) = (impact_min, impact_max) {
        if min > max {
            return Err(AppError::bad_request("impact_min must be <= impact_max").into());
        }
    }
    let (min_magnitude, max_magnitude) =
        parse_magnitude_range(query.min_magnitude, query.max_magnitude)?;

    let categories = parse_categories(query.category.as_deref())?;
    let severities = parse_severities(query.severity.as_deref())?;
    let sources = parse_sources(query.source.as_deref());
    let sort = parse_sort(query.sort.as_deref())?;

    let limit = query.limit.unwrap_or(MAP_MAX_LIMIT).clamp(1, MAP_MAX_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    // The globe streams points in pages but only needs the total once. Compute it
    // on the first page and skip the redundant COUNT on subsequent batches.
    let with_count = offset == 0;

    let result = list_event_map_points(
        &state.pool,
        &EventListFilter {
            tenant_id: auth.tenant_id,
            bbox,
            categories,
            severities,
            sources,
            occurred_after: query.occurred_after,
            occurred_before: query.occurred_before,
            impact_min,
            impact_max,
            min_magnitude,
            max_magnitude,
            sort,
            limit,
            offset,
        },
        with_count,
    )
    .await?;

    Ok(Json(result))
}

/// `GET /api/v1/events/sources` — distinct source keys in the tenant's data.
pub async fn sources(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<EventSourcesResponse>, ApiError> {
    auth.require_permission(Permission::EventsRead)?;
    let sources = list_event_sources(&state.pool, auth.tenant_id).await?;
    Ok(Json(EventSourcesResponse { sources }))
}

/// `GET /api/v1/events/{id}` — fetch one event within the authenticated tenant.
pub async fn get_by_id(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(event_id): Path<Uuid>,
) -> Result<Json<Event>, ApiError> {
    auth.require_permission(Permission::EventsRead)?;

    let event = get_event(&state.pool, auth.tenant_id, event_id)
        .await?
        .ok_or_else(|| AppError::not_found("event not found"))?;

    Ok(Json(event))
}

/// Parse and bound an impact score (0–100), naming the offending field on error.
pub(crate) fn parse_impact(value: Option<i64>, field: &str) -> Result<Option<u8>, AppError> {
    match value {
        None => Ok(None),
        Some(score) if (0..=100).contains(&score) => Ok(Some(score as u8)),
        Some(_) => Err(AppError::bad_request(format!(
            "{field} must be between 0 and 100"
        ))),
    }
}

/// Validate a magnitude range (min <= max when both present).
pub(crate) fn parse_magnitude_range(
    min: Option<f64>,
    max: Option<f64>,
) -> Result<(Option<f64>, Option<f64>), AppError> {
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            return Err(AppError::bad_request(
                "min_magnitude must be <= max_magnitude",
            ));
        }
    }
    Ok((min, max))
}

/// Parse a comma-separated category list (empty/absent = no constraint).
pub(crate) fn parse_categories(raw: Option<&str>) -> Result<Vec<Category>, AppError> {
    parse_enum_csv(raw, "category")
}

/// Parse a comma-separated severity list (empty/absent = no constraint).
pub(crate) fn parse_severities(raw: Option<&str>) -> Result<Vec<Severity>, AppError> {
    parse_enum_csv(raw, "severity")
}

/// Parse a comma-separated source list (empty/absent = no constraint).
pub(crate) fn parse_sources(raw: Option<&str>) -> Vec<String> {
    raw.map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

/// Parse the sort order (defaults to `recent`).
pub(crate) fn parse_sort(raw: Option<&str>) -> Result<EventSort, AppError> {
    match raw.map(str::trim) {
        None | Some("") | Some("recent") => Ok(EventSort::Recent),
        Some("impact_desc") => Ok(EventSort::ImpactDesc),
        Some("magnitude_desc") => Ok(EventSort::MagnitudeDesc),
        Some(other) => Err(AppError::bad_request(format!("unknown sort: {other}"))),
    }
}

/// Deserialize each comma-separated token into a serde enum `T` (snake_case).
fn parse_enum_csv<T: serde::de::DeserializeOwned>(
    raw: Option<&str>,
    field: &str,
) -> Result<Vec<T>, AppError> {
    let Some(raw) = raw else {
        return Ok(vec![]);
    };
    raw.split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            serde_json::from_value::<T>(serde_json::Value::String(token.to_owned()))
                .map_err(|_| AppError::bad_request(format!("unknown {field}: {token}")))
        })
        .collect()
}

fn parse_bbox(
    min_lon: Option<f64>,
    min_lat: Option<f64>,
    max_lon: Option<f64>,
    max_lat: Option<f64>,
) -> Result<Option<EventBBox>, AppError> {
    match (min_lon, min_lat, max_lon, max_lat) {
        (None, None, None, None) => Ok(None),
        (Some(min_lon), Some(min_lat), Some(max_lon), Some(max_lat)) => {
            if min_lon > max_lon || min_lat > max_lat {
                return Err(AppError::bad_request("invalid bbox: min must be <= max"));
            }
            Ok(Some(EventBBox {
                min_lon,
                min_lat,
                max_lon,
                max_lat,
            }))
        }
        _ => Err(AppError::bad_request(
            "bbox requires min_lon, min_lat, max_lon, and max_lat together",
        )),
    }
}
