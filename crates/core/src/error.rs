//! Canonical application error type and JSON error envelope.
//!
//! [`AppError`] is the single error type that flows through Geos. The HTTP API
//! maps it to the fixed JSON envelope `{ "error": { code, message, fields? } }`
//! (see `api-error-shape.mdc`). The `IntoResponse` implementation lives in
//! `crates/api` so this crate stays free of web-framework dependencies; here we
//! expose the building blocks (status code, client-safe message, envelope).

use std::collections::BTreeMap;

use serde::Serialize;

/// The single error type shared across all Geos crates.
///
/// Construct variants via the helper constructors (e.g. [`AppError::not_found`])
/// rather than building strings ad hoc, so codes and statuses stay consistent.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    /// Authentication is required or failed (HTTP 401).
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// The caller is authenticated but lacks permission (HTTP 403).
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// The requested resource does not exist or is not visible (HTTP 404).
    #[error("not found: {0}")]
    NotFound(String),

    /// The request was malformed or semantically invalid (HTTP 400).
    #[error("bad request: {0}")]
    BadRequest(String),

    /// Field-level validation failed (HTTP 422); `fields` maps field -> reason.
    #[error("validation failed: {message}")]
    Validation {
        /// Human-readable summary safe to show to clients.
        message: String,
        /// Per-field validation messages for client-side display.
        fields: BTreeMap<String, String>,
    },

    /// The request conflicts with current state (HTTP 409).
    #[error("conflict: {0}")]
    Conflict(String),

    /// The caller exceeded a rate limit or quota (HTTP 429).
    #[error("rate limited: {0}")]
    RateLimited(String),

    /// A required configuration value was missing or invalid (boot-time).
    #[error("configuration error: {0}")]
    Config(String),

    /// An unexpected internal failure (HTTP 500). The wrapped detail is for
    /// logs only and is never sent to clients.
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// 401 — authentication required or failed.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    /// 403 — authenticated but not permitted.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    /// 404 — resource missing or not visible to the caller.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// 400 — malformed or semantically invalid request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    /// 422 — field-level validation failure with per-field reasons.
    pub fn validation(message: impl Into<String>, fields: BTreeMap<String, String>) -> Self {
        Self::Validation {
            message: message.into(),
            fields,
        }
    }

    /// 409 — request conflicts with current state.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    /// 429 — rate limit or quota exceeded.
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::RateLimited(message.into())
    }

    /// 500 — unexpected internal failure; detail is logged, not returned.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    /// Stable machine-readable error code for the JSON envelope.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound(_) => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Validation { .. } => "validation",
            Self::Conflict(_) => "conflict",
            Self::RateLimited(_) => "rate_limited",
            Self::Config(_) => "config",
            Self::Internal(_) => "internal",
        }
    }

    /// HTTP status code to return for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Unauthorized(_) => 401,
            Self::Forbidden(_) => 403,
            Self::NotFound(_) => 404,
            Self::BadRequest(_) => 400,
            Self::Validation { .. } => 422,
            Self::Conflict(_) => 409,
            Self::RateLimited(_) => 429,
            // Config errors surface at boot, not over HTTP, but map to 500.
            Self::Config(_) | Self::Internal(_) => 500,
        }
    }

    // Human: Internal/config detail must never reach clients; we return a
    // generic string for 500s and the real text for client-safe variants.
    // Agent: RETURNS client-safe message; HIDES Internal/Config detail (logs only).
    fn client_message(&self) -> String {
        match self {
            Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::NotFound(m)
            | Self::BadRequest(m)
            | Self::Conflict(m)
            | Self::RateLimited(m) => m.clone(),
            Self::Validation { message, .. } => message.clone(),
            Self::Config(_) | Self::Internal(_) => "Internal server error".to_owned(),
        }
    }

    /// Build the client-facing JSON envelope for this error.
    ///
    /// The result serializes to `{ "error": { "code", "message", "fields"? } }`.
    pub fn envelope(&self) -> ErrorEnvelope {
        let fields = match self {
            Self::Validation { fields, .. } if !fields.is_empty() => Some(fields.clone()),
            _ => None,
        };
        ErrorEnvelope {
            error: ErrorBody {
                code: self.code(),
                message: self.client_message(),
                fields,
            },
        }
    }
}

/// Top-level error envelope: `{ "error": { ... } }`.
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    /// The single error object.
    pub error: ErrorBody,
}

/// Body of the error envelope.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Stable machine-readable code (e.g. `"not_found"`).
    pub code: &'static str,
    /// Client-safe human-readable message.
    pub message: String,
    /// Optional per-field validation messages; omitted when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<String, String>>,
}

/// Convenience alias for fallible operations returning [`AppError`].
pub type Result<T> = std::result::Result<T, AppError>;
