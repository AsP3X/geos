//! Authentication endpoints (`/api/v1/auth/*`).

use axum::{extract::State, Extension, Json};
use serde::Deserialize;

use crate::auth;
use crate::middleware::request_id::RequestContext;
use crate::state::AppState;

/// `POST /api/v1/auth/register` body.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// User email.
    pub email: String,
    /// Plaintext password (hashed server-side).
    pub password: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// New tenant display name.
    pub tenant_name: String,
    /// Unique tenant slug.
    pub tenant_slug: String,
}

/// `POST /api/v1/auth/login` body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// User email.
    pub email: String,
    /// Plaintext password.
    pub password: String,
    /// Tenant slug when the user belongs to multiple tenants.
    pub tenant_slug: Option<String>,
}

/// `POST /api/v1/auth/refresh` body.
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    /// Opaque refresh token from a prior login/register/refresh.
    pub refresh_token: String,
    /// Optional tenant switch on refresh.
    pub tenant_slug: Option<String>,
}

/// Register a tenant and owner account.
pub async fn register(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<auth::AuthResponse>, crate::error::ApiError> {
    let response = auth::register(
        &state.pool,
        &state.config.jwt_secret,
        auth::RegisterInput {
            email: body.email.trim(),
            password: &body.password,
            display_name: body.display_name.as_deref(),
            tenant_name: body.tenant_name.trim(),
            tenant_slug: body.tenant_slug.trim(),
            request: ctx.meta,
        },
    )
    .await?;
    Ok(Json(response))
}

/// Login with email and password.
pub async fn login(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<auth::AuthResponse>, crate::error::ApiError> {
    let response = auth::login(
        &state.pool,
        &state.config.jwt_secret,
        body.email.trim(),
        &body.password,
        body.tenant_slug.as_deref().map(str::trim),
        ctx.meta,
    )
    .await?;
    Ok(Json(response))
}

/// Rotate refresh token and issue new access token.
pub async fn refresh(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<auth::AuthResponse>, crate::error::ApiError> {
    let response = auth::refresh(
        &state.pool,
        &state.config.jwt_secret,
        body.refresh_token.trim(),
        body.tenant_slug.as_deref().map(str::trim),
        ctx.meta,
    )
    .await?;
    Ok(Json(response))
}
