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

/// Compute `(impact_score, severity)` for an NWS alert from severity and urgency.
pub fn nws_alert(severity: Option<&str>, urgency: Option<&str>) -> (u8, Severity) {
    let base = match severity {
        Some("Extreme") => 90,
        Some("Severe") => 75,
        Some("Moderate") => 50,
        Some("Minor") => 25,
        _ => 15,
    };

    let urgency_boost = match urgency {
        Some("Immediate") => 10,
        Some("Expected") => 5,
        _ => 0,
    };

    let score = (base + urgency_boost).min(100);
    let severity = match score {
        s if s >= 85 => Severity::Critical,
        s if s >= 65 => Severity::High,
        s if s >= 40 => Severity::Moderate,
        s if s >= 20 => Severity::Low,
        _ => Severity::Info,
    };
    (score, severity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nws_extreme_immediate_is_critical() {
        let (score, severity) = nws_alert(Some("Extreme"), Some("Immediate"));
        assert_eq!(score, 100);
        assert_eq!(severity, Severity::Critical);
    }

    #[test]
    fn nws_severe_fixture_maps_high() {
        let (score, severity) = nws_alert(Some("Severe"), Some("Expected"));
        assert_eq!(score, 80);
        assert_eq!(severity, Severity::High);
    }
}
