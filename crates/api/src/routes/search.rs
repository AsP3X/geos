//! Full-text search proxy (`/api/v1/search`).

use axum::{
    extract::{Query, State},
    Extension, Json,
};
use geos_core::events::{Category, Severity};
use geos_core::meili::{search_events, SearchFilters};
use geos_core::rbac::Permission;
use geos_core::AppError;
use serde::Deserialize;

use crate::error::ApiError;
use crate::middleware::auth::AuthContext;
use crate::state::AppState;

/// Query parameters for `GET /api/v1/search`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search terms (Meilisearch query).
    pub q: String,
    /// Optional category filter.
    pub category: Option<Category>,
    /// Optional severity filter.
    pub severity: Option<Severity>,
    /// Minimum impact score (0–100).
    pub min_impact: Option<i64>,
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

    let min_impact = match query.min_impact {
        None => None,
        Some(score) if (0..=100).contains(&score) => Some(score as u8),
        Some(_) => {
            return Err(AppError::bad_request("min_impact must be between 0 and 100").into());
        }
    };

    let filters = SearchFilters {
        category: query.category,
        severity: query.severity,
        min_impact,
    };

    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0);

    let results = search_events(
        state.meili.client(),
        auth.tenant_id,
        q,
        &filters,
        limit,
        offset,
    )
    .await?;

    Ok(Json(results))
}
