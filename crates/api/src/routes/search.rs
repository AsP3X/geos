//! Full-text search proxy (`/api/v1/search`).

use axum::{
    extract::{Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use geos_core::meili::{search_events, SearchFilters, SearchSort};
use geos_core::rbac::Permission;
use geos_core::AppError;
use serde::Deserialize;

use crate::error::ApiError;
use crate::middleware::auth::AuthContext;
use crate::routes::events::{parse_categories, parse_impact, parse_severities, parse_sources};
use crate::state::AppState;

/// Query parameters for `GET /api/v1/search`.
///
/// Mirrors the event filter where the Meili index supports it. Magnitude is not
/// indexed and is intentionally absent. `min_impact` aliases `impact_min`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search terms (Meilisearch query).
    pub q: String,
    /// Comma-separated categories.
    pub category: Option<String>,
    /// Comma-separated severity tiers.
    pub severity: Option<String>,
    /// Comma-separated source keys.
    pub source: Option<String>,
    /// Minimum impact score (0–100).
    pub impact_min: Option<i64>,
    /// Maximum impact score (0–100).
    pub impact_max: Option<i64>,
    /// Back-compat alias for `impact_min`.
    pub min_impact: Option<i64>,
    /// ISO-8601 lower bound on `occurred_at`.
    pub occurred_after: Option<DateTime<Utc>>,
    /// ISO-8601 upper bound on `occurred_at`.
    pub occurred_before: Option<DateTime<Utc>>,
    /// Sort order; `magnitude_desc` falls back to relevance (not indexed).
    pub sort: Option<String>,
    /// Page size (default 20, max 100).
    pub limit: Option<usize>,
    /// Pagination offset.
    pub offset: Option<usize>,
}

/// `GET /api/v1/search` — tenant-scoped keyword search via Meilisearch.
pub async fn search(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<geos_core::meili::SearchResults>, ApiError> {
    auth.require_permission(Permission::SearchRead)?;

    let q = query.q.trim();
    if q.is_empty() {
        return Err(AppError::bad_request("query parameter q is required").into());
    }

    let impact_min = parse_impact(query.impact_min.or(query.min_impact), "impact_min")?;
    let impact_max = parse_impact(query.impact_max, "impact_max")?;
    if let (Some(min), Some(max)) = (impact_min, impact_max) {
        if min > max {
            return Err(AppError::bad_request("impact_min must be <= impact_max").into());
        }
    }

    let filters = SearchFilters {
        categories: parse_categories(query.category.as_deref())?,
        severities: parse_severities(query.severity.as_deref())?,
        sources: parse_sources(query.source.as_deref()),
        impact_min,
        impact_max,
        occurred_after: query.occurred_after,
        occurred_before: query.occurred_before,
    };

    // Absent sort preserves keyword relevance ranking; magnitude is not indexed
    // and degrades to relevance.
    let sort = match query.sort.as_deref().map(str::trim) {
        None | Some("") => SearchSort::Relevance,
        Some("recent") => SearchSort::Recent,
        Some("impact_desc") => SearchSort::ImpactDesc,
        Some("magnitude_desc") => SearchSort::Relevance,
        Some(other) => {
            return Err(AppError::bad_request(format!("unknown sort: {other}")).into());
        }
    };

    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0);

    let results = search_events(
        state.meili.client(),
        auth.tenant_id,
        q,
        &filters,
        sort,
        limit,
        offset,
    )
    .await?;

    Ok(Json(results))
}
