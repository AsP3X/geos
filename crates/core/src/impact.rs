//! Deterministic, versioned impact scoring for canonical events.
//!
//! Full weight configuration and recomputation on enrichment land in a later
//! slice; earthquake scoring is the first implementation so connectors can
//! populate `impact_score` and `severity` at normalization time.

use crate::events::Severity;

/// Compute `(impact_score, severity)` for a USGS-style earthquake magnitude.
///
/// Weights are intentionally simple for v1; tune via ADR when enrichment
/// recomputes scores from casualties, area, and source confidence.
pub fn earthquake_magnitude(magnitude: f64) -> (u8, Severity) {
    let mag = magnitude.max(0.0);
    let score = ((mag / 10.0) * 100.0).clamp(0.0, 100.0).round() as u8;
    let severity = match mag {
        m if m >= 7.0 => Severity::Critical,
        m if m >= 6.0 => Severity::High,
        m if m >= 5.0 => Severity::Moderate,
        m if m >= 4.0 => Severity::Low,
        _ => Severity::Info,
    };
    (score, severity)
}
