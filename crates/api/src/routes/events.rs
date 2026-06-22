//! Tenant-scoped event read endpoints.

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use geos_core::db::{get_event, list_events, EventBBox, EventListFilter};
use geos_core::events::{Category, Event, Severity};
use geos_core::rbac::Permission;
use geos_core::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::middleware::auth::AuthContext;
use crate::state::AppState;

/// Query parameters for `GET /api/v1/events`.
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
    /// Filter by category (`earthquake`, `incident`, …).
    pub category: Option<Category>,
    /// Filter by severity tier.
    pub severity: Option<Severity>,
    /// ISO-8601 lower bound on `occurred_at`.
    pub occurred_after: Option<DateTime<Utc>>,
    /// ISO-8601 upper bound on `occurred_at`.
    pub occurred_before: Option<DateTime<Utc>>,
    /// Minimum impact score (0–100); events below are excluded.
    pub min_impact: Option<i64>,
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
    /// Applied page size.
    pub limit: i64,
    /// Applied offset.
    pub offset: i64,
}

/// `GET /api/v1/events` — list events for the authenticated tenant.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<EventListResponse>, ApiError> {
    auth.require_permission(Permission::EventsRead)?;

    let bbox = parse_bbox(query.min_lon, query.min_lat, query.max_lon, query.max_lat)?;
    let min_impact = parse_min_impact(query.min_impact)?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let items = list_events(
        &state.pool,
        &EventListFilter {
            tenant_id: auth.tenant_id,
            bbox,
            category: query.category,
            severity: query.severity,
            occurred_after: query.occurred_after,
            occurred_before: query.occurred_before,
            min_impact,
            limit,
            offset,
        },
    )
    .await?;

    Ok(Json(EventListResponse {
        items,
        limit,
        offset,
    }))
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

fn parse_min_impact(value: Option<i64>) -> Result<Option<u8>, AppError> {
    match value {
        None => Ok(None),
        Some(score) if (0..=100).contains(&score) => Ok(Some(score as u8)),
        Some(_) => Err(AppError::bad_request(
            "min_impact must be between 0 and 100",
        )),
    }
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
