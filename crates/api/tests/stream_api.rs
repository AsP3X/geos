//! WebSocket stream integration tests (require `DATABASE_URL` and Meilisearch).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use geos_api::routes::tiles::TileFlight;
use geos_api::stream::{run_event_listener, EventStreamHub};
use geos_api::{build_router, AppState};
use geos_core::config::Config;
use geos_core::db::{connect_pool, run_migrations, upsert_event, EventNotifyPayload};
use geos_core::events::{Category, Event, EventStatus, GeoPoint, Severity, VerificationStatus};
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

    let stream = EventStreamHub::default();
    let listener_url = config.database_url.clone();
    let listener_tx = stream.publisher();
    tokio::spawn(async move {
        run_event_listener(&listener_url, listener_tx).await;
    });

    Some(AppState {
        config,
        pool,
        meili,
        stream,
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
                        "tenant_name": "Stream Org",
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
        impact_score: 50,
        magnitude: Some(4.0),
        title: Some("Stream test event".to_owned()),
        summary: None,
        body: None,
        original_text: None,
        translated_text: None,
        language: None,
        location: GeoPoint {
            lon: -120.0,
            lat: 38.0,
        },
        affected_area: None,
        country: Some("US".to_owned()),
        region: None,
        place_name: Some("Streamville".to_owned()),
        occurred_at: now,
        detected_at: None,
        ingested_at: now,
        status: EventStatus::Active,
        verification_status: VerificationStatus::Unverified,
        confidence: 0.7,
        tags: vec!["stream".to_owned()],
        url: None,
        raw: serde_json::json!({}),
        embedding: None,
    }
}

#[tokio::test]
async fn stream_upgrade_requires_auth() {
    let Some(state) = setup().await else {
        return;
    };

    let app = build_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/stream")
                .header("connection", "Upgrade")
                .header("upgrade", "websocket")
                .header("sec-websocket-version", "13")
                // RFC 6455 example handshake key, not a secret.
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==") // gitleaks:allow
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn upsert_emits_postgres_notify_for_stream_hub() {
    let Some(state) = setup().await else {
        return;
    };

    let pool = state.pool.clone();
    let mut notify_rx = state.stream.subscribe();
    let app = build_router(state);

    let slug = format!("stream-notify-{}", Uuid::new_v4().simple());
    let auth = register_tenant(&app, &slug, &format!("stream-notify-{slug}@example.com")).await;
    let tenant_id: Uuid = auth["tenant_id"].as_str().unwrap().parse().unwrap();

    let event = sample_event(tenant_id, "notify-test-1");
    let event_id = event.id;
    upsert_event(&pool, &event).await.unwrap();

    let received = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let payload: EventNotifyPayload = notify_rx.recv().await.unwrap();
            if payload.event_id == event_id {
                return payload;
            }
        }
    })
    .await
    .expect("timed out waiting for event notification");

    assert_eq!(received.tenant_id, tenant_id);
}
