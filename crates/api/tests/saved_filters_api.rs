//! Saved-filters, filter-state, and sources API integration tests (require `DATABASE_URL`).

#![allow(clippy::unwrap_used)]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use geos_api::stream::EventStreamHub;
use geos_api::{build_router, AppState};
use geos_core::config::Config;
use geos_core::db::{connect_pool, run_migrations, upsert_event};
use geos_core::events::{Category, Event, EventStatus, GeoPoint, Severity, VerificationStatus};
use geos_core::meili::{self, MeiliClient};
use http_body_util::BodyExt;
use serde_json::{json, Value};
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
        stream: EventStreamHub::default(),
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
                    json!({
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

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap()
}

fn sample_filters() -> Value {
    json!({
        "version": 1,
        "categories": ["earthquake"],
        "severities": [],
        "sources": [],
        "impactMin": 10,
        "impactMax": 90,
        "magnitudeMin": null,
        "magnitudeMax": null,
        "timeRange": "30d",
        "from": null,
        "to": null,
        "sort": "recent"
    })
}

#[tokio::test]
async fn auth_response_includes_permissions() {
    let Some(state) = setup().await else {
        return;
    };
    let app = build_router(state);
    let slug = format!("perm-{}", Uuid::new_v4().simple());
    let auth = register_tenant(&app, &slug, &format!("{slug}@example.com")).await;

    let perms = auth["permissions"].as_array().unwrap();
    let keys: Vec<&str> = perms.iter().filter_map(Value::as_str).collect();
    // Owner preset holds management permissions.
    assert!(keys.contains(&"saved_filters.manage"));
    assert!(keys.contains(&"saved_filters.read"));
}

#[tokio::test]
async fn saved_filters_crud_and_isolation() {
    let Some(state) = setup().await else {
        return;
    };
    let app = build_router(state);

    let slug_a = format!("sf-a-{}", Uuid::new_v4().simple());
    let slug_b = format!("sf-b-{}", Uuid::new_v4().simple());
    let auth_a = register_tenant(&app, &slug_a, &format!("{slug_a}@example.com")).await;
    let auth_b = register_tenant(&app, &slug_b, &format!("{slug_b}@example.com")).await;
    let token_a = auth_a["access_token"].as_str().unwrap();
    let token_b = auth_b["access_token"].as_str().unwrap();

    // Create.
    let created = send(
        &app,
        "POST",
        "/api/v1/saved-filters",
        token_a,
        Some(json!({ "name": "My preset", "filters": sample_filters() })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let created_json = read_json(created).await;
    let preset_id = created_json["id"].as_str().unwrap().to_owned();
    assert_eq!(created_json["filters"]["impactMin"].as_i64(), Some(10));

    // List shows it for A.
    let list_a = send(&app, "GET", "/api/v1/saved-filters", token_a, None).await;
    assert_eq!(list_a.status(), StatusCode::OK);
    let list_json = read_json(list_a).await;
    assert_eq!(list_json["items"].as_array().map(Vec::len), Some(1));

    // B cannot see A's preset.
    let list_b = send(&app, "GET", "/api/v1/saved-filters", token_b, None).await;
    let list_b_json = read_json(list_b).await;
    assert_eq!(list_b_json["items"].as_array().map(Vec::len), Some(0));

    // B cannot update or delete A's preset (404, no existence leak).
    let cross_update = send(
        &app,
        "PUT",
        &format!("/api/v1/saved-filters/{preset_id}"),
        token_b,
        Some(json!({ "name": "hijack", "filters": sample_filters() })),
    )
    .await;
    assert_eq!(cross_update.status(), StatusCode::NOT_FOUND);

    let cross_delete = send(
        &app,
        "DELETE",
        &format!("/api/v1/saved-filters/{preset_id}"),
        token_b,
        None,
    )
    .await;
    assert_eq!(cross_delete.status(), StatusCode::NOT_FOUND);

    // A updates its own preset.
    let updated = send(
        &app,
        "PUT",
        &format!("/api/v1/saved-filters/{preset_id}"),
        token_a,
        Some(json!({ "name": "Renamed", "filters": sample_filters() })),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated_json = read_json(updated).await;
    assert_eq!(updated_json["name"].as_str(), Some("Renamed"));

    // Invalid payload rejected (impactMin > impactMax).
    let bad = send(
        &app,
        "POST",
        "/api/v1/saved-filters",
        token_a,
        Some(json!({
            "name": "bad",
            "filters": { "impactMin": 80, "impactMax": 20 }
        })),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    // A deletes its preset.
    let deleted = send(
        &app,
        "DELETE",
        &format!("/api/v1/saved-filters/{preset_id}"),
        token_a,
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let list_after = send(&app, "GET", "/api/v1/saved-filters", token_a, None).await;
    let after_json = read_json(list_after).await;
    assert_eq!(after_json["items"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn filter_state_is_per_user_and_isolated() {
    let Some(state) = setup().await else {
        return;
    };
    let app = build_router(state);

    let slug_a = format!("fs-a-{}", Uuid::new_v4().simple());
    let slug_b = format!("fs-b-{}", Uuid::new_v4().simple());
    let auth_a = register_tenant(&app, &slug_a, &format!("{slug_a}@example.com")).await;
    let auth_b = register_tenant(&app, &slug_b, &format!("{slug_b}@example.com")).await;
    let token_a = auth_a["access_token"].as_str().unwrap();
    let token_b = auth_b["access_token"].as_str().unwrap();

    // Initially empty.
    let initial = send(&app, "GET", "/api/v1/filter-state", token_a, None).await;
    assert_eq!(initial.status(), StatusCode::OK);
    let initial_json = read_json(initial).await;
    assert!(initial_json["filters"].is_null());

    // Upsert for A.
    let put = send(
        &app,
        "PUT",
        "/api/v1/filter-state",
        token_a,
        Some(json!({ "filters": sample_filters() })),
    )
    .await;
    assert_eq!(put.status(), StatusCode::NO_CONTENT);

    // A reads it back.
    let read_a = send(&app, "GET", "/api/v1/filter-state", token_a, None).await;
    let read_a_json = read_json(read_a).await;
    assert_eq!(read_a_json["filters"]["impactMax"].as_i64(), Some(90));

    // B still has nothing (isolation).
    let read_b = send(&app, "GET", "/api/v1/filter-state", token_b, None).await;
    let read_b_json = read_json(read_b).await;
    assert!(read_b_json["filters"].is_null());
}

#[tokio::test]
async fn sources_endpoint_reflects_tenant_data() {
    let Some(state) = setup().await else {
        return;
    };
    let pool = state.pool.clone();
    let app = build_router(state);

    let slug = format!("src-{}", Uuid::new_v4().simple());
    let auth = register_tenant(&app, &slug, &format!("{slug}@example.com")).await;
    let token = auth["access_token"].as_str().unwrap();
    let tenant_id: Uuid = auth["tenant_id"].as_str().unwrap().parse().unwrap();

    let event = Event {
        id: Uuid::new_v4(),
        tenant_id,
        source: "test-src".to_owned(),
        source_event_id: "sources-1".to_owned(),
        category: Category::Earthquake,
        severity: Severity::Moderate,
        impact_score: 50,
        magnitude: Some(4.0),
        title: Some("Quake".to_owned()),
        summary: None,
        body: None,
        original_text: None,
        translated_text: None,
        language: None,
        location: GeoPoint {
            lon: -25.0,
            lat: -45.0,
        },
        affected_area: None,
        country: None,
        region: None,
        place_name: None,
        occurred_at: Utc::now(),
        detected_at: None,
        ingested_at: Utc::now(),
        status: EventStatus::Active,
        verification_status: VerificationStatus::Unverified,
        confidence: 0.9,
        tags: vec![],
        url: None,
        raw: json!({}),
        embedding: None,
    };
    upsert_event(&pool, &event).await.unwrap();

    let response = send(&app, "GET", "/api/v1/events/sources", token, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    let sources: Vec<&str> = json["sources"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(sources.contains(&"test-src"));
}
