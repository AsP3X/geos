//! Tile proxy + tiles-session integration tests (require `DATABASE_URL`).
//!
//! These exercise the auth/bounds gates that do not need network access to the
//! object store or upstream imagery. The caching read-through logic itself is
//! unit-tested with stub `TileStore`/`TileUpstream` impls in `routes::tiles`.

#![allow(clippy::unwrap_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use geos_api::routes::tiles::TileFlight;
use geos_api::stream::EventStreamHub;
use geos_api::{build_router, AppState};
use geos_core::config::Config;
use geos_core::db::{connect_pool, run_migrations};
use geos_core::meili::{self, MeiliClient};
use geos_core::storage::StorageClient;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

/// Deliberately low imagery zoom ceiling for the test process: proves
/// `TILES_MAX_ZOOM` is honored end-to-end without caching deep tile pyramids.
const TEST_MAX_ZOOM: u32 = 6;

async fn setup() -> Option<AppState> {
    if std::env::var("DATABASE_URL").is_err() {
        return None;
    }
    if std::env::var("JWT_SECRET").is_err() {
        std::env::set_var("JWT_SECRET", "test-jwt-secret-for-integration-tests-only!!");
    }
    std::env::set_var("MEILI_URL", "http://localhost:7700");
    std::env::set_var("MEILI_MASTER_KEY", "masterKeyChangeMe32CharsMinimum!");
    std::env::set_var("STORAGE_URL", "http://localhost:9000");
    if std::env::var("NOS_JWT_SECRET").is_err() {
        std::env::set_var(
            "NOS_JWT_SECRET",
            "test-nos-jwt-secret-for-integration-tests!!",
        );
    }
    // Pin a low zoom ceiling so the test exercises the configurable
    // `TILES_MAX_ZOOM` path and keeps any cached-tile footprint minimal.
    std::env::set_var("TILES_MAX_ZOOM", TEST_MAX_ZOOM.to_string());

    let config = Config::from_env().ok()?;
    let pool = connect_pool(&config.database_url).await.ok()?;
    run_migrations(&pool).await.ok()?;
    let meili = MeiliClient::new(&config.meili_url, &config.meili_master_key).ok()?;
    meili::ensure_events_index(meili.client()).await.ok()?;
    let http = reqwest::Client::new();
    let storage =
        StorageClient::with_client(http.clone(), &config.storage_url, &config.nos_jwt_secret);
    Some(AppState {
        config,
        pool,
        meili,
        stream: EventStreamHub::default(),
        http,
        storage,
        tile_flight: TileFlight::default(),
    })
}

async fn read_json(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

async fn register_tenant(app: &Router, slug: &str, email: &str) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "email": email,
                        "password": "secure-password-12",
                        "tenant_name": "Tiles Org",
                        "tenant_slug": slug,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    read_json(response).await
}

#[tokio::test]
async fn tile_endpoint_requires_token_and_enforces_zoom() {
    let Some(state) = setup().await else {
        return;
    };
    let app = build_router(state);

    // No token at all -> 401 before any storage/upstream access.
    let no_token = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tiles/sentinel2/5/1/2.jpg")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_token.status(), StatusCode::UNAUTHORIZED);

    // The session endpoint itself is protected.
    let session_unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tiles/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(session_unauth.status(), StatusCode::UNAUTHORIZED);

    // Mint a tiles token via an authenticated session.
    let slug = format!("tiles-{}", Uuid::new_v4().simple());
    let auth = register_tenant(&app, &slug, &format!("{slug}@example.com")).await;
    let access_token = auth["access_token"].as_str().unwrap();

    let session = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tiles/session")
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(session.status(), StatusCode::OK);
    let session_json = read_json(session).await;
    let tiles_token = session_json["token"].as_str().unwrap().to_owned();
    assert!(session_json["expires_in"].as_u64().unwrap() > 0);
    // The session advertises the configured ceiling so the globe caps Cesium's
    // `maximumLevel` and the proxy never caches deeper than configured.
    assert_eq!(
        session_json["max_zoom"].as_u64().unwrap(),
        u64::from(TEST_MAX_ZOOM)
    );

    // A zoom one level above the configured ceiling is rejected before fetching,
    // proving the lowered `TILES_MAX_ZOOM` actually constrains tile requests.
    let too_deep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/tiles/sentinel2/{}/1/2.jpg?token={tiles_token}",
                    TEST_MAX_ZOOM + 1
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(too_deep.status(), StatusCode::NOT_FOUND);

    // A bogus token is rejected with 401.
    let bad_token = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tiles/sentinel2/5/1/2.jpg?token=not-a-real-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_token.status(), StatusCode::UNAUTHORIZED);
}
