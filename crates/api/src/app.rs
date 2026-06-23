//! Axum router assembly.

use axum::{
    middleware,
    routing::{get, post, put},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::middleware::auth::require_auth;
use crate::middleware::request_id::assign_request_context;
use crate::routes::{auth, connectors, events, health, saved_filters, search, stream, tiles};
use crate::state::AppState;

/// Build the full HTTP router with middleware and `/api/v1` routes.
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/stream", get(stream::stream))
        // Public router: imagery is global, not tenant data; the handler
        // validates a signed tiles token from `?token=` itself.
        .route(
            "/api/v1/tiles/sentinel2/{z}/{x}/{y}",
            get(tiles::serve_sentinel2),
        );

    let protected = Router::new()
        .route("/api/v1/events", get(events::list))
        .route("/api/v1/events/map", get(events::map))
        .route("/api/v1/events/sources", get(events::sources))
        .route("/api/v1/events/{id}", get(events::get_by_id))
        .route("/api/v1/connectors", get(connectors::list))
        .route("/api/v1/search", get(search::search))
        .route(
            "/api/v1/saved-filters",
            get(saved_filters::list).post(saved_filters::create),
        )
        .route(
            "/api/v1/saved-filters/{id}",
            put(saved_filters::update).delete(saved_filters::delete),
        )
        .route(
            "/api/v1/filter-state",
            get(saved_filters::get_state).put(saved_filters::put_state),
        )
        .route("/api/v1/tiles/session", get(tiles::session))
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
