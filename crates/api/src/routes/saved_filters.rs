//! Per-user saved filter presets (`/api/v1/saved-filters`) and active filter
//! state (`/api/v1/filter-state`).
//!
//! Presets are RBAC-gated: `saved_filters.read` to list, `saved_filters.manage`
//! to create/update/delete. Active filter state is private per-user UI state and
//! requires only authentication (no permission key). Everything is tenant- and
//! user-scoped; objects owned by another tenant/user return 404 (no existence
//! leak, `tenant-isolation.mdc`). Mutations on presets are audited.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use geos_core::audit::{self, AuditEntry};
use geos_core::db::{
    create_saved_filter, delete_saved_filter, get_user_filter_state, list_saved_filters,
    update_saved_filter, upsert_user_filter_state, SavedFilter, SavedFilterPayload,
};
use geos_core::rbac::Permission;
use geos_core::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::middleware::auth::AuthContext;
use crate::middleware::request_id::RequestContext;
use crate::state::AppState;

/// Request body for creating/updating a preset.
#[derive(Debug, Deserialize)]
pub struct SavedFilterBody {
    /// User-visible preset name.
    pub name: String,
    /// The captured filter (validated server-side).
    pub filters: SavedFilterPayload,
}

/// List wrapper for presets.
#[derive(Debug, Serialize)]
pub struct SavedFilterListResponse {
    /// The caller's presets.
    pub items: Vec<SavedFilter>,
}

/// Active-filter-state read wrapper (`null` when nothing is persisted yet).
#[derive(Debug, Serialize)]
pub struct FilterStateResponse {
    /// The caller's stored active filter, if any.
    pub filters: Option<SavedFilterPayload>,
}

/// Request body for upserting active filter state.
#[derive(Debug, Deserialize)]
pub struct FilterStateBody {
    /// The active filter to persist.
    pub filters: SavedFilterPayload,
}

fn validate_name(name: &str) -> Result<&str, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("preset name is required"));
    }
    if trimmed.chars().count() > 80 {
        return Err(AppError::bad_request(
            "preset name must be 80 characters or fewer",
        ));
    }
    Ok(trimmed)
}

/// `GET /api/v1/saved-filters` — list the caller's presets.
pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<SavedFilterListResponse>, ApiError> {
    auth.require_permission(Permission::SavedFiltersRead)?;
    let items = list_saved_filters(&state.pool, auth.tenant_id, auth.user_id).await?;
    Ok(Json(SavedFilterListResponse { items }))
}

/// `POST /api/v1/saved-filters` — create (overwrite on name conflict).
pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(ctx): Extension<RequestContext>,
    Json(body): Json<SavedFilterBody>,
) -> Result<Json<SavedFilter>, ApiError> {
    auth.require_permission(Permission::SavedFiltersManage)?;
    let name = validate_name(&body.name)?;
    body.filters.validate()?;

    let saved = create_saved_filter(
        &state.pool,
        auth.tenant_id,
        auth.user_id,
        name,
        &body.filters,
    )
    .await?;

    audit_saved_filter(&state, &auth, &ctx, "saved_filter.create", saved.id).await;
    Ok(Json(saved))
}

/// `PUT /api/v1/saved-filters/{id}` — update an owned preset.
pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<Uuid>,
    Json(body): Json<SavedFilterBody>,
) -> Result<Json<SavedFilter>, ApiError> {
    auth.require_permission(Permission::SavedFiltersManage)?;
    let name = validate_name(&body.name)?;
    body.filters.validate()?;

    let saved = update_saved_filter(
        &state.pool,
        auth.tenant_id,
        auth.user_id,
        id,
        name,
        &body.filters,
    )
    .await?
    .ok_or_else(|| AppError::not_found("saved filter not found"))?;

    audit_saved_filter(&state, &auth, &ctx, "saved_filter.update", saved.id).await;
    Ok(Json(saved))
}

/// `DELETE /api/v1/saved-filters/{id}` — delete an owned preset.
pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    auth.require_permission(Permission::SavedFiltersManage)?;
    let removed = delete_saved_filter(&state.pool, auth.tenant_id, auth.user_id, id).await?;
    if !removed {
        return Err(AppError::not_found("saved filter not found").into());
    }
    audit_saved_filter(&state, &auth, &ctx, "saved_filter.delete", id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/filter-state` — read the caller's active filter (authenticated only).
pub async fn get_state(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<FilterStateResponse>, ApiError> {
    let filters = get_user_filter_state(&state.pool, auth.tenant_id, auth.user_id).await?;
    Ok(Json(FilterStateResponse { filters }))
}

/// `PUT /api/v1/filter-state` — upsert the caller's active filter (authenticated only).
pub async fn put_state(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<FilterStateBody>,
) -> Result<StatusCode, ApiError> {
    body.filters.validate()?;
    upsert_user_filter_state(&state.pool, auth.tenant_id, auth.user_id, &body.filters).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Write a best-effort audit row for a preset mutation. Failure to audit is
/// logged but does not fail the request the user already succeeded at.
async fn audit_saved_filter(
    state: &AppState,
    auth: &AuthContext,
    ctx: &RequestContext,
    action: &str,
    id: Uuid,
) {
    let id_string = id.to_string();
    let entry = AuditEntry {
        tenant_id: Some(auth.tenant_id),
        actor_user_id: Some(auth.user_id),
        action,
        resource_type: Some("saved_filter"),
        resource_id: Some(&id_string),
        context: serde_json::json!({}),
        request: ctx.meta.clone(),
    };
    if let Err(err) = audit::write(&state.pool, entry).await {
        tracing::warn!(error = %err, action, saved_filter_id = %id, "failed to write audit row");
    }
}
