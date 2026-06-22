//! # geos-api
//!
//! Axum HTTP API for Geos (`/api/v1`): authentication, tenant scoping, RBAC,
//! and event endpoints.

pub mod app;
pub mod auth;
pub mod error;
pub mod middleware;
pub mod routes;
pub mod state;
pub mod stream;

pub use app::build_router;
pub use state::AppState;
