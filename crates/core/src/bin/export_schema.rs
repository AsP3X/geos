//! Print the canonical Event JSON Schema to stdout.
//!
//! Invoked by `scripts/gen-types.sh` to (re)generate
//! `crates/core/schema/event.schema.json`, from which the frontend TypeScript
//! types are derived.

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("{}", geos_core::event_schema_json()?);
    Ok(())
}
