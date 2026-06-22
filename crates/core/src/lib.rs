//! # geos-core
//!
//! Shared foundation crate for the Geos platform. This crate is the single
//! source of truth for the canonical [`Event`] domain model (see
//! `canonical-event-schema.mdc`) and will host configuration, the [`AppError`]
//! type, the SQLx database layer, PostGIS/pgvector helpers, the `AiProvider`
//! trait, and the nebular-os storage client.
//!
//! At this scaffold stage only the crate skeleton and error type exist; modules
//! are added by the `core-schema` work in the implementation plan.

/// Crate version string, sourced from `Cargo.toml` at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Top-level error type shared across Geos crates.
///
/// Concrete variants (database, validation, auth, storage, AI provider, …) are
/// introduced alongside the modules that produce them. Keeping a single error
/// enum here lets the API map every failure to the canonical `AppError` JSON
/// envelope (`api-error-shape.mdc`).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    /// A configuration value was missing or invalid.
    #[error("configuration error: {0}")]
    Config(String),
}

/// Convenience alias for fallible operations returning [`AppError`].
pub type Result<T> = std::result::Result<T, AppError>;
