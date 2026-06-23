//! Auth HTTP integration tests (require `DATABASE_URL` and `JWT_SECRET`).

#![allow(clippy::unwrap_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
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

#[tokio::test]
async fn register_login_refresh_flow() {
    let Some(state) = setup().await else {
        return;
    };

    let app = build_router(state);
    let slug = format!("test-{}", Uuid::new_v4().simple());
    let email = format!("user-{slug}@example.com");

    let register_body = serde_json::json!({
        "email": email,
        "password": "secure-password-12",
        "display_name": "Test User",
        "tenant_name": "Test Org",
        "tenant_slug": slug,
    });

    let register_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(register_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(register_response.status(), StatusCode::OK);
    let register_json = read_json(register_response).await;
    let refresh_token = register_json["refresh_token"].as_str().unwrap();

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "email": email,
                        "password": "secure-password-12",
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login_response.status(), StatusCode::OK);

    let refresh_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "refresh_token": refresh_token }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(refresh_response.status(), StatusCode::OK);
}
