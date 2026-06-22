//! Process configuration loaded and validated from environment variables.
//!
//! [`Config::from_env`] reads the variables documented in `.env.example` and
//! fails fast with [`AppError::Config`] if a required value is missing, so
//! services crash loudly at boot rather than misbehaving later
//! (`rust/rust-quality.mdc`).

use crate::error::{AppError, Result};

/// Minimum length (characters) required for security-sensitive secrets.
const MIN_SECRET_LEN: usize = 32;

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
