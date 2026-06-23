//! nebular-os object-storage client.
//!
//! [`StorageClient`] wraps a [`reqwest::Client`] and talks to the nebular-os
//! `GET`/`HEAD`/`PUT /{bucket}/{*key}` routes (`nebular-os/src/server.rs`),
//! authenticating with a short-lived Geos-minted NOS JWT (HS256, claims
//! `{sub,email,role,exp,iat}` per `nebular-os/src/auth.rs`). Per
//! `nebular-os-vendor.mdc`, this integration lives entirely in Geos; the
//! storage service source is never edited here.
//!
//! It is used by the API tile proxy as a caching backend for imagery tiles
//! (bucket `geos-tiles`), but is a general-purpose object client.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;

use crate::error::{AppError, Result};

/// Lifetime of a minted NOS service token. Short-lived because the client mints
/// a fresh token per request batch; nebular-os validates `exp`.
const NOS_TOKEN_TTL: Duration = Duration::from_secs(5 * 60);

/// Claims for the nebular-os JWT. Must match `Claims` in `nebular-os/src/auth.rs`
/// (`sub`, `email`, `role`, `exp`, `iat`); `role = "admin"` grants read+write.
#[derive(Debug, Serialize)]
struct NosClaims {
    sub: String,
    email: String,
    role: String,
    exp: i64,
    iat: i64,
}

/// HTTP client for the nebular-os object store (cheap to clone).
#[derive(Clone)]
pub struct StorageClient {
    http: reqwest::Client,
    base_url: String,
    nos_jwt_secret: String,
}

impl StorageClient {
    /// Build a storage client targeting `base_url`, signing tokens with
    /// `nos_jwt_secret`.
    ///
    /// # Errors
    /// Returns [`AppError::Config`] if the underlying HTTP client cannot be
    /// constructed.
    pub fn new(base_url: &str, nos_jwt_secret: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|err| AppError::Config(format!("storage http client: {err}")))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            nos_jwt_secret: nos_jwt_secret.to_owned(),
        })
    }

    /// Reuse an existing [`reqwest::Client`] (shares the connection pool).
    pub fn with_client(http: reqwest::Client, base_url: &str, nos_jwt_secret: &str) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            nos_jwt_secret: nos_jwt_secret.to_owned(),
        }
    }

    // Human: Builds the full object URL for `{bucket}/{key}`; keys may contain
    // slashes (nebular-os matches `/{bucket}/{*key}`) so the key is appended raw.
    // Agent: RETURNS "{base}/{bucket}/{key}".
    fn object_url(&self, bucket: &str, key: &str) -> String {
        format!("{}/{}/{}", self.base_url, bucket, key)
    }

    // Human: Mints a fresh short-lived NOS JWT for one request; admin role so
    // both reads and writes are authorized by nebular-os role gating.
    // Agent: SIGNS HS256 with nos_jwt_secret; claims {sub,email,role=admin,exp,iat}.
    fn mint_token(&self) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0) as i64;
        let claims = NosClaims {
            sub: "geos-api".to_owned(),
            email: "geos-api@geos.internal".to_owned(),
            role: "admin".to_owned(),
            iat: now,
            exp: now + NOS_TOKEN_TTL.as_secs() as i64,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.nos_jwt_secret.as_bytes()),
        )
        .map_err(|err| AppError::internal(format!("nos jwt encode failed: {err}")))
    }

    /// Fetch an object's bytes, or `None` if it does not exist (HTTP 404).
    ///
    /// # Errors
    /// Returns [`AppError::Internal`] on transport failures or non-404 error
    /// statuses.
    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Option<Bytes>> {
        let token = self.mint_token()?;
        let response = self
            .http
            .get(self.object_url(bucket, key))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("storage get failed: {err}")))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(AppError::internal(format!(
                "storage get returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|err| AppError::internal(format!("storage get body failed: {err}")))?;
        Ok(Some(bytes))
    }

    /// Return whether an object exists (HTTP `HEAD`).
    ///
    /// # Errors
    /// Returns [`AppError::Internal`] on transport failures or non-404 error
    /// statuses.
    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<bool> {
        let token = self.mint_token()?;
        let response = self
            .http
            .head(self.object_url(bucket, key))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("storage head failed: {err}")))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if !response.status().is_success() {
            return Err(AppError::internal(format!(
                "storage head returned {}",
                response.status()
            )));
        }
        Ok(true)
    }

    /// Store an object with the given content type.
    ///
    /// # Errors
    /// Returns [`AppError::Internal`] on transport failures or non-success
    /// statuses.
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        bytes: Bytes,
    ) -> Result<()> {
        let token = self.mint_token()?;
        let response = self
            .http
            .put(self.object_url(bucket, key))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("storage put failed: {err}")))?;

        if !response.status().is_success() {
            return Err(AppError::internal(format!(
                "storage put returned {}",
                response.status()
            )));
        }
        Ok(())
    }
}
