//! Search API integration tests (require `DATABASE_URL` and Meilisearch).

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
use geos_core::meili::{self, upsert_event_document, MeiliClient};
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

    let config = Config::from_env().ok()?;
    let pool = connect_pool(&config.database_url).await.ok()?;
    run_migrations(&pool).await.ok()?;
    let meili = MeiliClient::new(&config.meili_url, &config.meili_master_key).ok()?;
    meili::ensure_events_index(meili.client()).await.ok()?;
    Some(AppState {
        config,
        pool,
        meili,
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
                        "tenant_name": "Search Org",
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

fn searchable_event(tenant_id: Uuid, source_event_id: &str, title: &str) -> Event {
    let now = Utc::now();
    Event {
        id: Uuid::new_v4(),
        tenant_id,
        source: "test".to_owned(),
        source_event_id: source_event_id.to_owned(),
        category: Category::Earthquake,
        severity: Severity::Moderate,
        impact_score: 55,
        magnitude: Some(4.2),
        title: Some(title.to_owned()),
        summary: Some("UniqueSearchMarker summary text".to_owned()),
        body: None,
        original_text: None,
        translated_text: None,
        language: None,
        location: GeoPoint {
            lon: -118.0,
            lat: 34.0,
        },
        affected_area: None,
        country: Some("US".to_owned()),
        region: None,
        place_name: Some("Searchville".to_owned()),
        occurred_at: now,
        detected_at: None,
        ingested_at: now,
        status: EventStatus::Active,
        verification_status: VerificationStatus::Unverified,
        confidence: 0.8,
        tags: vec!["search-test".to_owned()],
        url: None,
        raw: serde_json::json!({}),
        embedding: None,
    }
}

#[tokio::test]
async fn search_is_tenant_scoped_and_requires_auth() {
    let Some(state) = setup().await else {
        return;
    };

    let pool = state.pool.clone();
    let meili = state.meili.clone();
    let app = build_router(state);

    let slug_a = format!("search-a-{}", Uuid::new_v4().simple());
    let slug_b = format!("search-b-{}", Uuid::new_v4().simple());
    let auth_a = register_tenant(&app, &slug_a, &format!("search-a-{slug_a}@example.com")).await;
    let auth_b = register_tenant(&app, &slug_b, &format!("search-b-{slug_b}@example.com")).await;

    let tenant_a: Uuid = auth_a["tenant_id"].as_str().unwrap().parse().unwrap();
    let tenant_b: Uuid = auth_b["tenant_id"].as_str().unwrap().parse().unwrap();
    let token_a = auth_a["access_token"].as_str().unwrap();
    let token_b = auth_b["access_token"].as_str().unwrap();

    let event_a = searchable_event(tenant_a, "search-a-1", "UniqueSearchMarker Alpha");
    upsert_event(&pool, &event_a).await.unwrap();
    upsert_event_document(meili.client(), &event_a)
        .await
        .unwrap();

    let event_b = searchable_event(tenant_b, "search-b-1", "UniqueSearchMarker Beta");
    upsert_event(&pool, &event_b).await.unwrap();
    upsert_event_document(meili.client(), &event_b)
        .await
        .unwrap();

    // Meilisearch indexing is async; wait briefly for documents to become searchable.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let unauth = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/search?q=UniqueSearchMarker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let tenant_a_hits = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/search?q=UniqueSearchMarker")
                .header("authorization", format!("Bearer {token_a}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tenant_a_hits.status(), StatusCode::OK);
    let hits_a = read_json(tenant_a_hits).await;
    let items_a = hits_a["hits"].as_array().unwrap();
    assert_eq!(items_a.len(), 1);
    assert_eq!(items_a[0]["id"].as_str().unwrap(), event_a.id.to_string());

    let tenant_b_hits = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/search?q=UniqueSearchMarker")
                .header("authorization", format!("Bearer {token_b}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tenant_b_hits.status(), StatusCode::OK);
    let hits_b = read_json(tenant_b_hits).await;
    let items_b = hits_b["hits"].as_array().unwrap();
    assert_eq!(items_b.len(), 1);
    assert_eq!(items_b[0]["id"].as_str().unwrap(), event_b.id.to_string());
}
