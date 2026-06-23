//! Imagery tile proxy and tiles-session token (`/api/v1/tiles/*`).
//!
//! The proxy is a lazy, caching read-through to the nebular-os object store
//! (bucket `geos-tiles`): a storage hit is served directly; a miss is fetched
//! once from the upstream EOX Sentinel-2 cloudless service (CC-BY), served, and
//! persisted so the local tile dataset grows organically. A per-key
//! single-flight gate collapses concurrent misses for the same tile so we never
//! stampede the upstream on first view.
//!
//! Imagery is global (not tenant data), so the tile route is on the public
//! router and validates a dedicated medium-lived "tiles token" (`?token=`)
//! in-handler — the same pattern used by `routes::stream` — decoupling Cesium's
//! long-lived imagery URL from 15-minute access-token rotation.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    Extension, Json,
};
use bytes::Bytes;
use geos_core::rbac::Permission;
use geos_core::storage::StorageClient;
use geos_core::AppError;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::auth::jwt::{decode_tiles_token, issue_tiles_token, TILES_TOKEN_TTL};
use crate::error::ApiError;
use crate::middleware::auth::AuthContext;
use crate::state::AppState;

/// Content type served for Sentinel-2 imagery tiles.
const TILE_CONTENT_TYPE: &str = "image/jpeg";

/// Long, immutable cache lifetime: a `{z}/{x}/{y}` Sentinel tile is effectively
/// content-addressed (one week, immutable) so browsers/CDNs cache aggressively.
const TILE_CACHE_CONTROL: &str = "public, max-age=604800, immutable";

// ── Single-flight gate ───────────────────────────────────────────────────────

/// Per-key async gate that collapses concurrent cache misses for the same tile.
///
/// On a miss the caller acquires the key's async mutex before fetching upstream;
/// other callers for the same key wait, then re-check storage (now populated) on
/// the double-check inside [`serve_tile`]. Worst case on a rare cleanup race is a
/// duplicate upstream fetch, never corruption (`put_object` is idempotent).
#[derive(Clone, Default)]
pub struct TileFlight {
    inner: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl TileFlight {
    // Human: Return (creating if needed) the shared async gate for this tile key.
    // Agent: LOCKS std map (poison-tolerant); RETURNS Arc<tokio::Mutex> registered under key.
    fn gate(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.entry(key.to_owned()).or_default().clone()
    }

    // Human: Drop the key's gate once no other waiter still holds a clone, so the
    // map does not grow unbounded across the lifetime of the server.
    // Agent: LOCKS std map; REMOVES key iff stored Arc strong_count <= 1.
    fn cleanup(&self, key: &str) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(gate) = map.get(key) {
            if Arc::strong_count(gate) <= 1 {
                map.remove(key);
            }
        }
    }
}

// ── Storage / upstream abstraction (stubbable in tests) ──────────────────────

/// Read-through tile cache backend.
pub trait TileStore: Send + Sync {
    /// Fetch a cached tile by storage key, or `None` if absent.
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<Bytes>, AppError>> + Send;
    /// Persist a tile under the given storage key.
    fn put(
        &self,
        key: &str,
        content_type: &str,
        bytes: Bytes,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}

/// Upstream imagery source consulted on a cache miss.
pub trait TileUpstream: Send + Sync {
    /// Fetch a `{z}/{x}/{y}` tile, or `None` if the upstream has no such tile.
    fn fetch(
        &self,
        z: u32,
        x: u32,
        y: u32,
    ) -> impl Future<Output = Result<Option<Bytes>, AppError>> + Send;
}

/// [`TileStore`] backed by the nebular-os object store, scoped to one bucket.
struct ObjectTileStore<'a> {
    client: &'a StorageClient,
    bucket: &'a str,
}

impl TileStore for ObjectTileStore<'_> {
    async fn get(&self, key: &str) -> Result<Option<Bytes>, AppError> {
        self.client.get_object(self.bucket, key).await
    }

    async fn put(&self, key: &str, content_type: &str, bytes: Bytes) -> Result<(), AppError> {
        self.client
            .put_object(self.bucket, key, content_type, bytes)
            .await
    }
}

/// [`TileUpstream`] that fetches JPEG tiles from a `{z}`/`{x}`/`{y}` URL template.
struct HttpTileUpstream<'a> {
    http: &'a reqwest::Client,
    template: &'a str,
}

impl TileUpstream for HttpTileUpstream<'_> {
    async fn fetch(&self, z: u32, x: u32, y: u32) -> Result<Option<Bytes>, AppError> {
        let url = self
            .template
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string());
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("imagery upstream failed: {err}")))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(AppError::internal(format!(
                "imagery upstream returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|err| AppError::internal(format!("imagery upstream body: {err}")))?;
        Ok(Some(bytes))
    }
}

// ── Core proxy logic (generic for testability) ───────────────────────────────

/// Build the canonical storage key for a Sentinel-2 tile.
fn sentinel2_key(z: u32, x: u32, y: u32) -> String {
    format!("sentinel2/{z}/{x}/{y}.jpg")
}

/// Serve a tile: storage hit -> bytes; miss -> single-flight upstream fetch,
/// persist, then bytes. `None` means neither cache nor upstream has the tile.
///
/// Tiles with `z > max_zoom` are refused **before any storage or upstream
/// access**, so a deeper zoom level is never downloaded from upstream nor
/// persisted to the cache. This is what bounds how much tile storage a
/// deployment accrues: lowering `max_zoom` strictly shrinks the cacheable set.
///
/// # Errors
/// Propagates storage/upstream [`AppError`]s.
pub async fn serve_tile<S, U>(
    store: &S,
    upstream: &U,
    flight: &TileFlight,
    z: u32,
    x: u32,
    y: u32,
    max_zoom: u32,
) -> Result<Option<Bytes>, AppError>
where
    S: TileStore,
    U: TileUpstream,
{
    // Hard ceiling: never read, fetch, or persist beyond the configured zoom.
    if z > max_zoom {
        return Ok(None);
    }

    let key = sentinel2_key(z, x, y);

    // Fast path: serve straight from cache without taking the single-flight gate.
    if let Some(bytes) = store.get(&key).await? {
        return Ok(Some(bytes));
    }

    let gate = flight.gate(&key);
    let outcome = async {
        let _permit = gate.lock().await;
        // Double-check: a concurrent miss for this key may have populated it
        // while we waited on the gate.
        if let Some(bytes) = store.get(&key).await? {
            return Ok(Some(bytes));
        }
        match upstream.fetch(z, x, y).await? {
            Some(bytes) => {
                store.put(&key, TILE_CONTENT_TYPE, bytes.clone()).await?;
                Ok(Some(bytes))
            }
            None => Ok(None),
        }
    }
    .await;

    flight.cleanup(&key);
    outcome
}

// ── HTTP handlers ────────────────────────────────────────────────────────────

/// Response body for `GET /api/v1/tiles/session`.
#[derive(Debug, Serialize)]
pub struct TilesSessionResponse {
    /// Signed medium-lived tiles token to embed in the imagery URL.
    pub token: String,
    /// Lifetime of the token in seconds (frontend refreshes before expiry).
    pub expires_in: u64,
    /// Server-enforced maximum imagery zoom level (`tiles_max_zoom`). The globe
    /// caps Cesium's `maximumLevel` to this so it never requests — and the proxy
    /// never caches — tiles deeper than the configured ceiling. Lowering this in
    /// config reduces how much tile storage a deployment accrues.
    pub max_zoom: u32,
}

/// `GET /api/v1/tiles/session` (protected) — mint a tiles token for the globe.
pub async fn session(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<TilesSessionResponse>, ApiError> {
    // Viewing imagery is part of viewing the event map, gated by EventsRead.
    auth.require_permission(Permission::EventsRead)?;
    let token = issue_tiles_token(&state.config.jwt_secret, auth.user_id, auth.tenant_id)?;
    Ok(Json(TilesSessionResponse {
        token,
        expires_in: TILES_TOKEN_TTL.as_secs(),
        max_zoom: state.config.tiles_max_zoom,
    }))
}

/// Query string for the public tile endpoint.
#[derive(Debug, Deserialize)]
pub struct TileTokenQuery {
    /// Signed tiles token (validated in-handler).
    pub token: Option<String>,
}

/// `GET /api/v1/tiles/sentinel2/{z}/{x}/{y}.jpg` (public; token-gated).
///
/// The final path segment includes the `.jpg` suffix (matchit treats `{y}` as
/// the whole segment), so it is stripped before parsing the tile row.
pub async fn serve_sentinel2(
    State(state): State<AppState>,
    Path((z, x, y_segment)): Path<(u32, u32, String)>,
    Query(query): Query<TileTokenQuery>,
) -> Response {
    match serve_sentinel2_inner(&state, z, x, &y_segment, query.token.as_deref()).await {
        Ok(response) => response,
        Err(err) => ApiError(err).into_response(),
    }
}

async fn serve_sentinel2_inner(
    state: &AppState,
    z: u32,
    x: u32,
    y_segment: &str,
    token: Option<&str>,
) -> Result<Response, AppError> {
    let token = token.ok_or_else(|| AppError::unauthorized("missing tiles token"))?;
    decode_tiles_token(&state.config.jwt_secret, token)?;

    // Enforce the server-side zoom ceiling regardless of what the client requests.
    if z > state.config.tiles_max_zoom {
        return Err(AppError::not_found("tile zoom out of range"));
    }

    let y: u32 = y_segment
        .strip_suffix(".jpg")
        .unwrap_or(y_segment)
        .parse()
        .map_err(|_| AppError::bad_request("invalid tile coordinate"))?;

    let store = ObjectTileStore {
        client: &state.storage,
        bucket: &state.config.tiles_bucket,
    };
    let upstream = HttpTileUpstream {
        http: &state.http,
        template: &state.config.imagery_upstream_url,
    };

    match serve_tile(
        &store,
        &upstream,
        &state.tile_flight,
        z,
        x,
        y,
        state.config.tiles_max_zoom,
    )
    .await?
    {
        Some(bytes) => tile_response(bytes),
        None => {
            warn!(z, x, y, "tile not available in cache or upstream");
            Err(AppError::not_found("tile not available"))
        }
    }
}

// Human: Builds the raw image/jpeg response with an aggressive cache header.
// Agent: RETURNS 200 image/jpeg body=bytes; SETS Cache-Control immutable 1w.
fn tile_response(bytes: Bytes) -> Result<Response, AppError> {
    Response::builder()
        .header(header::CONTENT_TYPE, TILE_CONTENT_TYPE)
        .header(header::CACHE_CONTROL, TILE_CACHE_CONTROL)
        .body(Body::from(bytes))
        .map_err(|err| AppError::internal(format!("tile response build: {err}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// In-memory tile store recording gets/puts, for network-free proxy tests.
    #[derive(Default)]
    struct MemStore {
        objects: StdMutex<HashMap<String, Bytes>>,
        get_calls: StdMutex<u32>,
        put_calls: StdMutex<u32>,
    }

    impl TileStore for MemStore {
        async fn get(&self, key: &str) -> Result<Option<Bytes>, AppError> {
            *self.get_calls.lock().unwrap_or_else(|p| p.into_inner()) += 1;
            Ok(self
                .objects
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(key)
                .cloned())
        }

        async fn put(&self, key: &str, _ct: &str, bytes: Bytes) -> Result<(), AppError> {
            *self.put_calls.lock().unwrap_or_else(|p| p.into_inner()) += 1;
            self.objects
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(key.to_owned(), bytes);
            Ok(())
        }
    }

    /// Upstream that returns fixed bytes and counts fetches.
    struct CountingUpstream {
        bytes: Bytes,
        fetches: StdMutex<u32>,
    }

    impl TileUpstream for CountingUpstream {
        async fn fetch(&self, _z: u32, _x: u32, _y: u32) -> Result<Option<Bytes>, AppError> {
            *self.fetches.lock().unwrap_or_else(|p| p.into_inner()) += 1;
            Ok(Some(self.bytes.clone()))
        }
    }

    #[tokio::test]
    async fn miss_fetches_upstream_and_persists() {
        let store = MemStore::default();
        let upstream = CountingUpstream {
            bytes: Bytes::from_static(b"jpegbytes"),
            fetches: StdMutex::new(0),
        };
        let flight = TileFlight::default();

        let first = serve_tile(&store, &upstream, &flight, 5, 1, 2, 12)
            .await
            .unwrap();
        assert_eq!(first.as_deref(), Some(&b"jpegbytes"[..]));
        assert_eq!(*store.put_calls.lock().unwrap(), 1);

        // Second request is a cache hit: no extra upstream fetch or put.
        let second = serve_tile(&store, &upstream, &flight, 5, 1, 2, 12)
            .await
            .unwrap();
        assert_eq!(second.as_deref(), Some(&b"jpegbytes"[..]));
        assert_eq!(*upstream.fetches.lock().unwrap(), 1);
        assert_eq!(*store.put_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn over_max_zoom_never_reads_fetches_or_persists() {
        let store = MemStore::default();
        let upstream = CountingUpstream {
            bytes: Bytes::from_static(b"jpegbytes"),
            fetches: StdMutex::new(0),
        };
        let flight = TileFlight::default();

        // z (8) above the configured ceiling (6): no download, no caching.
        let result = serve_tile(&store, &upstream, &flight, 8, 1, 2, 6)
            .await
            .unwrap();
        assert!(result.is_none());
        assert_eq!(*store.get_calls.lock().unwrap(), 0, "no cache read");
        assert_eq!(*upstream.fetches.lock().unwrap(), 0, "no upstream download");
        assert_eq!(*store.put_calls.lock().unwrap(), 0, "no tile persisted");

        // A tile exactly at the ceiling is still served (and cached) normally.
        let at_ceiling = serve_tile(&store, &upstream, &flight, 6, 1, 2, 6)
            .await
            .unwrap();
        assert_eq!(at_ceiling.as_deref(), Some(&b"jpegbytes"[..]));
        assert_eq!(*store.put_calls.lock().unwrap(), 1);
    }

    #[test]
    fn tile_key_layout_is_stable() {
        assert_eq!(sentinel2_key(12, 100, 200), "sentinel2/12/100/200.jpg");
    }
}
