//! Events API integration tests (require `DATABASE_URL`).

#![allow(clippy::unwrap_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use geos_api::{build_router, AppState};
use geos_core::config::Config;
use geos_core::db::{connect_pool, run_migrations, upsert_event};
use geos_core::events::{Category, Event, EventStatus, GeoPoint, Severity, VerificationStatus};
use geos_core::tenancy::SYSTEM_TENANT_ID;
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
    std::env::set_var(
        "MEILI_MASTER_KEY",
        "test-meili-master-key-for-integration!!",
    );
    std::env::set_var("STORAGE_URL", "http://localhost:9000");

    let config = Config::from_env().ok()?;
    let pool = connect_pool(&config.database_url).await.ok()?;
    run_migrations(&pool).await.ok()?;
    Some(AppState { config, pool })
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
                        "tenant_name": "Test Org",
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

fn sample_event(tenant_id: Uuid, source_event_id: &str) -> Event {
    let now = Utc::now();
    Event {
        id: Uuid::new_v4(),
        tenant_id,
        source: "test".to_owned(),
        source_event_id: source_event_id.to_owned(),
        category: Category::Earthquake,
        severity: Severity::Moderate,
        impact_score: 42,
        magnitude: Some(4.5),
        title: Some("Test quake".to_owned()),
        summary: None,
        body: None,
        original_text: None,
        translated_text: None,
        language: None,
        location: GeoPoint {
            lon: -122.0,
            lat: 37.0,
        },
        affected_area: None,
        country: Some("US".to_owned()),
        region: None,
        place_name: Some("Testville".to_owned()),
        occurred_at: now,
        detected_at: None,
        ingested_at: now,
        status: EventStatus::Active,
        verification_status: VerificationStatus::Unverified,
        confidence: 0.9,
        tags: vec!["test".to_owned()],
        url: None,
        raw: serde_json::json!({}),
        embedding: None,
    }
}

#[tokio::test]
async fn events_require_auth_and_respect_tenant_isolation() {
    let Some(state) = setup().await else {
        return;
    };

    let pool = state.pool.clone();
    let app = build_router(state);

    let slug_a = format!("tenant-a-{}", Uuid::new_v4().simple());
    let slug_b = format!("tenant-b-{}", Uuid::new_v4().simple());
    let auth_a = register_tenant(&app, &slug_a, &format!("a-{slug_a}@example.com")).await;
    let auth_b = register_tenant(&app, &slug_b, &format!("b-{slug_b}@example.com")).await;

    let tenant_a: Uuid = auth_a["tenant_id"].as_str().unwrap().parse().unwrap();
    let token_a = auth_a["access_token"].as_str().unwrap();
    let token_b = auth_b["access_token"].as_str().unwrap();

    let event = sample_event(tenant_a, "iso-test-1");
    let event_id = event.id;
    upsert_event(&pool, &event).await.unwrap();

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let list_a = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/events")
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_a.status(), StatusCode::OK);
    let list_json = read_json(list_a).await;
    assert_eq!(list_json["items"].as_array().unwrap().len(), 1);

    let cross_tenant = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/events/{event_id}"))
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_tenant.status(), StatusCode::NOT_FOUND);

    let system_event = sample_event(SYSTEM_TENANT_ID, "system-only");
    let system_event_id = system_event.id;
    upsert_event(&pool, &system_event).await.unwrap();

    let tenant_a_system = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/events/{system_event_id}"))
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tenant_a_system.status(), StatusCode::NOT_FOUND);
}
