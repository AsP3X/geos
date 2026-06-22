//! Authentication service logic shared by route handlers.

pub mod jwt;

use chrono::{Duration as ChronoDuration, Utc};
use geos_core::audit::{write, AuditEntry, RequestMeta};
use geos_core::db::PgPool;
use geos_core::db::{
    find_membership_for_user, find_user_by_email, find_valid_refresh_token, insert_refresh_token,
    list_memberships_for_user, permissions_for_role, register_tenant_and_owner,
    revoke_refresh_token, MembershipAuthRow,
};
use geos_core::error::{AppError, Result};
use geos_core::password::{hash_password, verify_password};
use serde_json::json;
use uuid::Uuid;

use self::jwt::{build_access_claims, hash_refresh_token, issue_access_token, mint_refresh_token};

/// Input for tenant + owner registration.
pub struct RegisterInput<'a> {
    /// User email.
    pub email: &'a str,
    /// Plaintext password.
    pub password: &'a str,
    /// Optional display name.
    pub display_name: Option<&'a str>,
    /// New tenant name.
    pub tenant_name: &'a str,
    /// New tenant slug.
    pub tenant_slug: &'a str,
    /// Request metadata for audit.
    pub request: RequestMeta,
}

/// Register a new tenant and owner user, returning token pair.
pub async fn register(
    pool: &PgPool,
    jwt_secret: &str,
    input: RegisterInput<'_>,
) -> Result<AuthResponse> {
    if input.password.len() < 12 {
        return Err(AppError::validation(
            "password too short",
            [(
                "password".to_owned(),
                "must be at least 12 characters".to_owned(),
            )]
            .into_iter()
            .collect(),
        ));
    }

    if find_user_by_email(pool, input.email).await?.is_some() {
        return Err(AppError::conflict("email already registered"));
    }

    let password_hash = hash_password(input.password)?;
    let (tenant_id, user_id, membership_id) = register_tenant_and_owner(
        pool,
        input.tenant_name,
        input.tenant_slug,
        input.email,
        &password_hash,
        input.display_name,
    )
    .await?;

    write(
        pool,
        AuditEntry {
            tenant_id: Some(tenant_id),
            actor_user_id: Some(user_id),
            action: "auth.register",
            resource_type: Some("tenant"),
            resource_id: Some(&tenant_id.to_string()),
            context: json!({ "email": input.email, "tenant_slug": input.tenant_slug }),
            request: input.request,
        },
    )
    .await?;

    issue_session(
        pool,
        jwt_secret,
        user_id,
        MembershipAuthRow {
            membership_id,
            tenant_id,
            tenant_slug: input.tenant_slug.to_owned(),
            role_key: "owner".to_owned(),
        },
    )
    .await
}

/// Successful authentication payload returned to clients.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthResponse {
    /// Bearer access JWT.
    pub access_token: String,
    /// Opaque refresh token (store securely client-side).
    pub refresh_token: String,
    /// Access token TTL in seconds.
    pub expires_in: u64,
    /// Always `Bearer`.
    pub token_type: &'static str,
    /// Authenticated user id.
    pub user_id: Uuid,
    /// Active tenant id.
    pub tenant_id: Uuid,
    /// Tenant slug.
    pub tenant_slug: String,
    /// Role key within the tenant.
    pub role: String,
}

/// Authenticate with email/password and optional tenant slug.
pub async fn login(
    pool: &PgPool,
    jwt_secret: &str,
    email: &str,
    password: &str,
    tenant_slug: Option<&str>,
    request: RequestMeta,
) -> Result<AuthResponse> {
    let Some(user) = find_user_by_email(pool, email).await? else {
        return Err(AppError::unauthorized("invalid email or password"));
    };

    if user.status != "active" {
        return Err(AppError::forbidden("account disabled"));
    }

    let Some(hash) = user.password_hash.as_deref() else {
        return Err(AppError::unauthorized("invalid email or password"));
    };

    if !verify_password(password, hash)? {
        return Err(AppError::unauthorized("invalid email or password"));
    }

    let membership = resolve_membership(pool, user.id, tenant_slug).await?;

    write(
        pool,
        AuditEntry {
            tenant_id: Some(membership.tenant_id),
            actor_user_id: Some(user.id),
            action: "auth.login",
            resource_type: Some("user"),
            resource_id: Some(&user.id.to_string()),
            context: json!({ "tenant_slug": membership.tenant_slug }),
            request,
        },
    )
    .await?;

    issue_session(pool, jwt_secret, user.id, membership).await
}

/// Rotate refresh token and issue a new access token.
pub async fn refresh(
    pool: &PgPool,
    jwt_secret: &str,
    refresh_token: &str,
    tenant_slug: Option<&str>,
    request: RequestMeta,
) -> Result<AuthResponse> {
    let token_hash = hash_refresh_token(refresh_token);
    let Some(row) = find_valid_refresh_token(pool, &token_hash).await? else {
        return Err(AppError::unauthorized("invalid refresh token"));
    };

    revoke_refresh_token(pool, row.id).await?;

    let membership = resolve_membership(pool, row.user_id, tenant_slug).await?;

    write(
        pool,
        AuditEntry {
            tenant_id: Some(membership.tenant_id),
            actor_user_id: Some(row.user_id),
            action: "auth.refresh",
            resource_type: Some("user"),
            resource_id: Some(&row.user_id.to_string()),
            context: json!({ "tenant_slug": membership.tenant_slug }),
            request,
        },
    )
    .await?;

    issue_session(pool, jwt_secret, row.user_id, membership).await
}

async fn resolve_membership(
    pool: &PgPool,
    user_id: Uuid,
    tenant_slug: Option<&str>,
) -> Result<MembershipAuthRow> {
    if let Some(slug) = tenant_slug {
        return find_membership_for_user(pool, user_id, slug)
            .await?
            .ok_or_else(|| AppError::forbidden("not a member of this tenant"));
    }

    let memberships = list_memberships_for_user(pool, user_id).await?;
    match memberships.len() {
        0 => Err(AppError::forbidden("no active tenant memberships")),
        1 => Ok(memberships
            .into_iter()
            .next()
            .ok_or_else(|| AppError::internal("membership list unexpectedly empty"))?),
        _ => Err(AppError::bad_request(
            "tenant_slug required when user belongs to multiple tenants",
        )),
    }
}

async fn issue_session(
    pool: &PgPool,
    jwt_secret: &str,
    user_id: Uuid,
    membership: MembershipAuthRow,
) -> Result<AuthResponse> {
    let permissions =
        permissions_for_role(pool, membership.tenant_id, &membership.role_key).await?;
    let claims = build_access_claims(
        user_id,
        membership.tenant_id,
        membership.membership_id,
        membership.role_key.clone(),
        permissions,
    );
    let access_token = issue_access_token(jwt_secret, &claims)?;
    let (refresh_token, refresh_hash) = mint_refresh_token();
    let expires_at = Utc::now() + ChronoDuration::seconds(jwt::REFRESH_TOKEN_TTL.as_secs() as i64);
    insert_refresh_token(pool, user_id, &refresh_hash, expires_at).await?;

    Ok(AuthResponse {
        access_token,
        refresh_token,
        expires_in: jwt::ACCESS_TOKEN_TTL.as_secs(),
        token_type: "Bearer",
        user_id,
        tenant_id: membership.tenant_id,
        tenant_slug: membership.tenant_slug,
        role: membership.role_key,
    })
}
