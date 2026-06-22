//! # geos-core
//!
//! Shared foundation crate for the Geos platform. This crate is the single
//! source of truth for the canonical [`events::Event`] domain model (see
//! `canonical-event-schema.mdc`) and hosts the shared [`error::AppError`] type
//! and runtime [`config::Config`].
//!
//! Future modules (SQLx database layer, PostGIS/pgvector helpers, the
//! `AiProvider` trait, the nebular-os storage client, impact scoring) are added
//! by later steps in the implementation plan.

pub mod config;
pub mod db;
pub mod error;
pub mod events;
pub mod impact;
pub mod rbac;
pub mod tenancy;

pub use error::{AppError, Result};

/// Crate version string, sourced from `Cargo.toml` at build time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serialize the canonical [`events::Event`] JSON Schema as pretty JSON.
///
/// This is the authoritative schema exported for the frontend and external
/// consumers. `scripts/gen-types.sh` writes it to
/// `crates/core/schema/event.schema.json` and regenerates the matching
/// TypeScript types; a drift test guards that the checked-in file stays in sync.
///
/// # Errors
/// Returns a [`serde_json::Error`] only if schema serialization fails, which
/// indicates a bug in the schema derivation rather than user input.
pub fn event_schema_json() -> std::result::Result<String, serde_json::Error> {
    let schema = schemars::schema_for!(events::Event);
    serde_json::to_string_pretty(&schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Human: Fails if the checked-in Event JSON Schema no longer matches what
    // the Rust types produce, enforcing the canonical-schema invariant locally
    // (the same check runs in CI).
    // Agent: READS crates/core/schema/event.schema.json; COMPARES to event_schema_json(); FAILS on drift.
    #[test]
    fn checked_in_event_schema_is_in_sync() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let generated = event_schema_json()?;
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/schema/event.schema.json");
        let checked_in = std::fs::read_to_string(path)?;
        assert_eq!(
            generated.trim(),
            checked_in.trim(),
            "Event JSON Schema drift detected — regenerate with scripts/gen-types.sh"
        );
        Ok(())
    }

    // Human: Guards that each Permission serializes to the same dotted key that
    // as_str() returns, so the API payload form and DB catalog key never diverge.
    // Agent: ASSERTS serde rename == as_str for all Permission::ALL.
    #[test]
    fn permission_serialization_matches_as_str() -> std::result::Result<(), serde_json::Error> {
        for permission in rbac::Permission::ALL {
            let json = serde_json::to_string(&permission)?;
            assert_eq!(json, format!("\"{}\"", permission.as_str()));
        }
        Ok(())
    }

    // Human: Sanity-checks the preset hierarchy: Viewer ⊂ Analyst ⊂ Owner, and
    // Owner holds the full catalog. Catches accidental preset drift.
    // Agent: ASSERTS Owner==ALL; Viewer⊆Analyst⊆Owner permission sets.
    #[test]
    fn role_presets_are_nested_and_owner_is_full() {
        let owner = rbac::RolePreset::Owner.permissions();
        let analyst = rbac::RolePreset::Analyst.permissions();
        let viewer = rbac::RolePreset::Viewer.permissions();

        assert_eq!(owner.len(), rbac::Permission::ALL.len());
        assert!(viewer.iter().all(|p| analyst.contains(p)));
        assert!(analyst.iter().all(|p| owner.contains(p)));
    }

    #[test]
    fn earthquake_impact_scales_with_magnitude() {
        let (low, _) = impact::earthquake_magnitude(3.0);
        let (high, sev) = impact::earthquake_magnitude(7.5);
        assert!(high > low);
        assert_eq!(sev, events::Severity::Critical);
    }
}
