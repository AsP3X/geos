# 2. CesiumJS globe and caching Sentinel-2 tile proxy

- Status: accepted
- Date: 2026-06-23

## Context

The original globe rendered a single fixed `5400x2700` Blue Marble texture over
a react-three-fiber (r3f) sphere (~7 km/px at the equator), so it blurred on
zoom, and the hand-rolled LOD/clustering/horizon-culling code lagged while
zooming and rotating. We want Google-Earth-style sharp, smooth zoom using free,
fully-local imagery, while preserving every existing globe feature (dots,
clustering, selection + detail hydration, cluster-to-sidebar, heat overlay,
vector overlays, layer toggles, status overlay, live WebSocket updates,
deep-zoom camera, clear-on-empty-click).

## Decision

- **Rendering engine: full switch to CesiumJS.** Cesium's native quadtree LOD
  provides sharp, smooth zoom and built-in atmosphere/skybox/stars, removing the
  custom r3f LOD/culling code. The r3f globe files were removed after parity.
  Integrated via `vite-plugin-cesium` (copies Workers/Assets/Widgets and sets
  `CESIUM_BASE_URL`); imagery via `UrlTemplateImageryProvider`. Cesium Ion is
  disabled (`Ion.defaultAccessToken = ""`); no Ion network calls.
- **Imagery: local Sentinel-2 tile pyramid via a caching proxy.** The new
  `geos-api` tile proxy (`GET /api/v1/tiles/sentinel2/{z}/{x}/{y}.jpg`) is a lazy
  read-through to nebular-os object storage (bucket `geos-tiles`): a storage hit
  is served directly; a miss is fetched once from EOX Sentinel-2 cloudless
  (CC-BY 4.0), served, and persisted, so the local dataset grows organically. A
  per-key single-flight gate prevents upstream stampedes on first view. Max zoom
  is **z12** (~38 m/px), enforced server-side and as Cesium `maximumLevel`.
- **Storage client in `crates/core`.** A new `StorageClient` wraps `reqwest` and
  talks to nebular-os `GET/HEAD/PUT /{bucket}/{*key}`, authenticating with a
  short-lived Geos-minted NOS JWT (HS256, claims `{sub,email,role,exp,iat}`,
  signed with `NOS_JWT_SECRET`). Per `nebular-os-vendor.mdc`, all integration is
  in Geos only.
- **Tile auth: a dedicated medium-lived "tiles token".** `GET /api/v1/tiles/session`
  (protected, `events.read`) mints a ~12h JWT (`typ`/`aud` = `tiles`). The public
  tile route validates `?token=` in-handler (like `routes::stream`), decoupling
  Cesium's long-lived imagery URL from 15-minute access-token rotation. Imagery
  is global (not tenant data), so tiles are not tenant-scoped.
- **Offline fallback: local elevation-contour basemap.** `scripts/gen-contour-tiles.sh`
  builds shaded-relief + contour XYZ JPEG tiles (~z8) from the public-domain
  ETOPO 2022 DEM with GDAL, written to `frontend/public/contours/`. Cesium adds
  the contour layer **beneath** Sentinel, so failed/absent imagery tiles reveal
  the local basemap with no network.
- **Attribution.** The EOX credit ("Sentinel-2 cloudless by EOX IT Services
  GmbH") is attached to the imagery provider and shown in Cesium's credit
  container, satisfying CC-BY.

## Alternatives considered

- **Keep r3f and raise texture resolution:** rejected — single-texture globes
  cannot reach z12 sharpness, and the custom LOD code was the source of the lag.
- **Bulk pre-download of the whole pyramid:** rejected — multi-TB and discouraged
  by EOX licensing. The proxy is strictly lazy (only viewed tiles are cached).
- **Direct browser → EOX requests:** rejected — no caching, no attribution
  control, exposes the upstream, and cannot fall back offline.
- **3D terrain mesh:** out of scope for v1 (ellipsoid surface only); a future
  additive option.

## Consequences

- New env/config: `NOS_JWT_SECRET` (now required by the API), `TILES_BUCKET`
  (default `geos-tiles`), `TILES_MAX_ZOOM` (default 12), `IMAGERY_UPSTREAM_URL`
  (EOX template). Documented in `.env.example` and `docker-compose.yml`.
- `AppState` gained a shared `reqwest::Client`, a `StorageClient`, and a tile
  single-flight gate.
- The frontend drops `three`/`@react-three/*`/`earcut` and adds
  `cesium`/`resium`/`vite-plugin-cesium`. The Cesium runtime is loaded from
  copied static assets (`dist/cesium/`), lazily, so the main bundle stays small.
- Generated contour tiles and the DEM cache are git-ignored; the offline
  fallback requires a one-time `scripts/gen-contour-tiles.sh` run (GDAL ≥ 3.5).
- The `GlobeViewport` prop contract is unchanged, so `CommandCenterPage` only
  swaps the lazy import.
