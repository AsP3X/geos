//! Authenticated request context derived from JWT access tokens.

use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use geos_core::rbac::Permission;
use geos_core::AppError;
use uuid::Uuid;

use crate::auth::jwt::decode_access_token;
use crate::error::ApiError;
use crate::state::AppState;

/// Caller identity and tenant scope from a validated access JWT.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Authenticated user id (`sub` claim).
    pub user_id: Uuid,
    /// Active tenant for this session.
    pub tenant_id: Uuid,
    /// Membership row id.
    pub membership_id: Uuid,
    /// Role key within the tenant.
    pub role: String,
    /// Granted permission keys.
    pub permissions: Vec<String>,
}

impl AuthContext {
    /// Require a permission key; returns 403 when absent.
    pub fn require_permission(&self, permission: Permission) -> Result<(), AppError> {
        let key = permission.as_str();
        if self.permissions.iter().any(|value| value == key) {
            Ok(())
        } else {
            Err(AppError::forbidden("insufficient permissions"))
        }
    }
}

/// Reject unauthenticated requests and attach [`AuthContext`].
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let token = bearer_token(request.headers())
        .ok_or_else(|| AppError::unauthorized("missing or invalid Authorization header"))?;

    let claims = decode_access_token(&state.config.jwt_secret, token)?;

    request.extensions_mut().insert(AuthContext {
        user_id: claims.sub,
        tenant_id: claims.tenant_id,
        membership_id: claims.membership_id,
        role: claims.role,
        permissions: claims.permissions,
    });

    Ok(next.run(request).await)
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}
