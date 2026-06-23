//! Process configuration loaded and validated from environment variables.
//!
//! [`Config::from_env`] reads the variables documented in `.env.example` and
//! fails fast with [`AppError::Config`] if a required value is missing, so
//! services crash loudly at boot rather than misbehaving later
//! (`rust/rust-quality.mdc`).

use crate::error::{AppError, Result};

/// Minimum length (characters) required for security-sensitive secrets.
const MIN_SECRET_LEN: usize = 32;

/// Default object-storage bucket for cached imagery tiles.
const DEFAULT_TILES_BUCKET: &str = "geos-tiles";

/// Default maximum imagery zoom level (~38 m/px at z12 for Sentinel-2).
const DEFAULT_TILES_MAX_ZOOM: u32 = 12;

/// Default upstream imagery template: EOX Sentinel-2 cloudless (CC-BY 4.0).
///
/// Placeholders `{z}`/`{x}`/`{y}` are substituted by the tile proxy; the EOX
/// WMTS REST layout orders the path segments `{z}/{y}/{x}`.
const DEFAULT_IMAGERY_UPSTREAM_URL: &str =
    "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2024_3857/default/GoogleMapsCompatible/{z}/{y}/{x}.jpg";

/// Validated runtime configuration shared by the API and workers.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string (PostGIS + pgvector enabled).
    pub database_url: String,
    /// Base URL of the Meilisearch instance.
    pub meili_url: String,
    /// Meilisearch master key.
    pub meili_master_key: String,
    /// Base URL of the nebular-os object-storage service.
    pub storage_url: String,
    /// JWT signing secret for Geos auth (min 32 chars).
    pub jwt_secret: String,
    /// JWT signing secret for minting nebular-os service tokens (min 32 chars).
    pub nos_jwt_secret: String,
    /// Object-storage bucket holding cached imagery tiles.
    pub tiles_bucket: String,
    /// Upstream imagery URL template with `{z}`/`{x}`/`{y}` placeholders.
    pub imagery_upstream_url: String,
    /// Maximum imagery zoom level served by the tile proxy.
    pub tiles_max_zoom: u32,
    /// Socket address for the HTTP API (e.g. `0.0.0.0:8080`).
    pub bind_addr: String,
}

impl Config {
    /// Load and validate configuration from the process environment.
    ///
    /// # Errors
    /// Returns [`AppError::Config`] if a required variable is unset/empty or if
    /// a secret is shorter than [`MIN_SECRET_LEN`].
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: require_var("DATABASE_URL")?,
            meili_url: require_var("MEILI_URL")?,
            meili_master_key: require_secret("MEILI_MASTER_KEY")?,
            storage_url: require_var("STORAGE_URL")?,
            jwt_secret: require_secret("JWT_SECRET")?,
            nos_jwt_secret: require_secret("NOS_JWT_SECRET")?,
            tiles_bucket: optional_var("TILES_BUCKET", DEFAULT_TILES_BUCKET),
            imagery_upstream_url: optional_var(
                "IMAGERY_UPSTREAM_URL",
                DEFAULT_IMAGERY_UPSTREAM_URL,
            ),
            tiles_max_zoom: optional_zoom("TILES_MAX_ZOOM", DEFAULT_TILES_MAX_ZOOM)?,
            bind_addr: std::env::var("GEOS_BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".to_owned()),
        })
    }
}

// Human: A required, non-empty environment variable; trims surrounding
// whitespace so accidental padding does not pass validation silently.
// Agent: READS env var `name`; RETURNS AppError::Config if missing/empty.
fn require_var(name: &str) -> Result<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(AppError::Config(format!(
            "missing required environment variable: {name}"
        ))),
    }
}

// Human: Like `require_var` but additionally enforces a minimum secret length
// to reject obviously weak signing keys at boot.
// Agent: READS env var `name`; REQUIRES len >= MIN_SECRET_LEN; RETURNS AppError::Config otherwise.
fn require_secret(name: &str) -> Result<String> {
    let value = require_var(name)?;
    if value.len() < MIN_SECRET_LEN {
        return Err(AppError::Config(format!(
            "{name} must be at least {MIN_SECRET_LEN} characters"
        )));
    }
    Ok(value)
}

// Human: An optional env var with a fallback default; trims surrounding
// whitespace and treats empty/whitespace-only values as unset.
// Agent: READS env var `name`; RETURNS trimmed value or `default` when missing/empty.
fn optional_var(name: &str, default: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
        _ => default.to_owned(),
    }
}

// Human: Parses an optional zoom-level env var, rejecting non-numeric values so
// a typo fails fast at boot rather than silently disabling deep zoom.
// Agent: READS env var `name`; PARSES u32; RETURNS AppError::Config on invalid; defaults to `default`.
fn optional_zoom(name: &str, default: u32) -> Result<u32> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u32>()
            .map_err(|_| AppError::Config(format!("{name} must be a non-negative integer"))),
        _ => Ok(default),
    }
}
