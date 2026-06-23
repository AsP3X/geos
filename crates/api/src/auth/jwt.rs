//! JWT access tokens and refresh token helpers.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use geos_core::error::{AppError, Result};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Access token lifetime.
pub const ACCESS_TOKEN_TTL: Duration = Duration::from_secs(15 * 60);
/// Refresh token lifetime.
pub const REFRESH_TOKEN_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Tiles token lifetime: medium-lived so a long-running globe session keeps a
/// stable imagery URL decoupled from 15-minute access-token rotation.
pub const TILES_TOKEN_TTL: Duration = Duration::from_secs(12 * 60 * 60);

/// Claims embedded in a short-lived access JWT.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessClaims {
    /// Subject — user id.
    pub sub: Uuid,
    /// Active tenant for this session.
    pub tenant_id: Uuid,
    /// Membership row id.
    pub membership_id: Uuid,
    /// Role key within the tenant.
    pub role: String,
    /// Granted permission keys.
    pub permissions: Vec<String>,
    /// Token type discriminator.
    pub typ: String,
    /// Expiry (unix seconds).
    pub exp: u64,
    /// Issued-at (unix seconds).
    pub iat: u64,
}

/// Issue a signed access JWT.
pub fn issue_access_token(jwt_secret: &str, claims: &AccessClaims) -> Result<String> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|err| AppError::internal(format!("jwt encode failed: {err}")))
}

/// Validate and decode an access JWT.
pub fn decode_access_token(jwt_secret: &str, token: &str) -> Result<AccessClaims> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    let data = decode::<AccessClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::unauthorized("invalid or expired access token"))?;
    if data.claims.typ != "access" {
        return Err(AppError::unauthorized("invalid token type"));
    }
    Ok(data.claims)
}

/// Build access claims with standard expiry.
pub fn build_access_claims(
    user_id: Uuid,
    tenant_id: Uuid,
    membership_id: Uuid,
    role: String,
    permissions: Vec<String>,
) -> AccessClaims {
    let now = unix_now();
    AccessClaims {
        sub: user_id,
        tenant_id,
        membership_id,
        role,
        permissions,
        typ: "access".to_owned(),
        iat: now,
        exp: now + ACCESS_TOKEN_TTL.as_secs(),
    }
}

/// Claims embedded in a medium-lived tiles JWT used by the imagery proxy.
///
/// Distinct from [`AccessClaims`]: the only authorization it conveys is "this
/// principal may read imagery tiles", so it carries no permission list. Imagery
/// is global (not tenant data) per the tile-proxy design, so `tenant_id` is
/// retained only for auditing/rate-limiting context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TilesClaims {
    /// Subject — user id that requested the session token.
    pub sub: Uuid,
    /// Tenant active when the token was minted (context only).
    pub tenant_id: Uuid,
    /// Audience discriminator; always `"tiles"`.
    pub aud: String,
    /// Token type discriminator.
    pub typ: String,
    /// Expiry (unix seconds).
    pub exp: u64,
    /// Issued-at (unix seconds).
    pub iat: u64,
}

/// Issue a signed tiles JWT for the imagery proxy.
pub fn issue_tiles_token(jwt_secret: &str, user_id: Uuid, tenant_id: Uuid) -> Result<String> {
    let now = unix_now();
    let claims = TilesClaims {
        sub: user_id,
        tenant_id,
        aud: "tiles".to_owned(),
        typ: "tiles".to_owned(),
        iat: now,
        exp: now + TILES_TOKEN_TTL.as_secs(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|err| AppError::internal(format!("tiles jwt encode failed: {err}")))
}

/// Validate and decode a tiles JWT (used by the public tile endpoint).
pub fn decode_tiles_token(jwt_secret: &str, token: &str) -> Result<TilesClaims> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    // The `aud` field is validated manually below rather than via jsonwebtoken's
    // audience set, keeping the check explicit and framework-agnostic.
    validation.validate_aud = false;
    let data = decode::<TilesClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|_| AppError::unauthorized("invalid or expired tiles token"))?;
    if data.claims.typ != "tiles" || data.claims.aud != "tiles" {
        return Err(AppError::unauthorized("invalid token type"));
    }
    Ok(data.claims)
}

/// Generate a random refresh token and its SHA-256 hex hash for storage.
pub fn mint_refresh_token() -> (String, String) {
    let raw = Uuid::new_v4().to_string();
    let hash = hash_refresh_token(&raw);
    (raw, hash)
}

/// Hash a refresh token for lookup/storage.
pub fn hash_refresh_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_token_hash_is_deterministic() {
        assert_eq!(hash_refresh_token("abc"), hash_refresh_token("abc"));
        assert_ne!(hash_refresh_token("abc"), hash_refresh_token("def"));
    }
}
