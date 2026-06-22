//! Axum router assembly.

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::middleware::auth::require_auth;
use crate::middleware::request_id::assign_request_context;
use crate::routes::{auth, events, health, search, stream};
use crate::state::AppState;

/// Build the full HTTP router with middleware and `/api/v1` routes.
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/stream", get(stream::stream));

    let protected = Router::new()
        .route("/api/v1/events", get(events::list))
        .route("/api/v1/events/{id}", get(events::get_by_id))
        .route("/api/v1/search", get(search::search))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(middleware::from_fn(assign_request_context))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
