//! Liveness/readiness probes.

use axum::Json;
use serde::Serialize;

/// Health check JSON body.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Service status.
    pub status: &'static str,
    /// Crate version.
    pub version: &'static str,
}

/// `GET /health` — process is running.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: geos_core::VERSION,
    })
}
