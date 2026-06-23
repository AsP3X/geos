# Geos

Geos is a multi-tenant SaaS for ingesting, normalizing, correlating, and
visualizing OSINT / public-source events (earthquakes, incidents, alerts,
weather, news, conflict) on an interactive 3D globe. The backend, workers, and
CLI are written in Rust; the frontend is React + Vite with a CesiumJS globe fed
by a local, caching Sentinel-2 tile proxy.

> Source-available under the **Nebular OS Private Non-Commercial License v1.0**
> (Copyright © 2026 Niklas Vorberg). See [`LICENSE`](./LICENSE). Commercial use
> requires a separate written license.

## Architecture

| Component | Path | Stack |
| --- | --- | --- |
| Shared core | `crates/core` | Canonical `Event` model, config, errors, DB/PostGIS/pgvector, AI provider trait, storage client |
| HTTP + WS API | `crates/api` | Axum, JWT/argon2 auth, tenant scoping, RBAC |
| Ingestion workers | `crates/workers` | Connectors, normalizer, enrichment, correlation, schedulers |
| Interactive TUI | `crates/cli` | ratatui |
| Frontend | `frontend/` | React + Vite + TypeScript + Tailwind v4 + shadcn/ui + CesiumJS globe |
| Migrations | `migrations/` | sqlx (PostGIS + pgvector) |
| Object storage | `nebular-os/` | Git submodule — self-hosted S3-compatible storage (read-only vendor) |

The full design is captured in
[`.cursor/plans/geos_osint_globe_1c0d3b7d.plan.md`](./.cursor/plans/geos_osint_globe_1c0d3b7d.plan.md).

## Prerequisites

- Rust (stable, ≥ 1.85) with `cargo fmt` and `clippy`
- Node.js 22+ and `pnpm`
- Docker + Docker Compose
- Optional dev tooling: [`lefthook`](https://github.com/evilmartians/lefthook)
  and [`gitleaks`](https://github.com/gitleaks/gitleaks) for git hooks

## Getting started

```bash
# 1. Clone with the object-storage submodule
git clone --recurse-submodules <repo-url>
# or, in an existing clone:
./scripts/submodule-init.sh

# 2. Configure environment
cp .env.example .env        # then edit secrets (min 32 chars where noted)

# 3. Install git hooks (optional but recommended)
lefthook install

# 4. Install frontend dependencies
pnpm install

# 5. Build everything
cargo build --workspace
pnpm --filter geos-frontend build
```

## Running

```bash
# Data services (postgres, meilisearch, object-storage) + app services
./scripts/dev.sh

# Full stack in containers (local overlay — no external proxy-network required)
docker compose -f docker-compose.yml -f docker-compose.local.yml up --build

# Behind a reverse proxy: base compose attaches api/frontend to external
# proxy-network (create it first: docker network create proxy-network)
docker compose up --build
```

| Service | Default URL |
| --- | --- |
| API | http://localhost:8080 |
| Frontend | http://localhost:4173 (preview) / 5173 (dev) |
| Meilisearch | http://localhost:7700 |
| Object storage (nebular-os) | http://localhost:9000 |
| Postgres | localhost:5432 |

## Reverse proxy (nginx Proxy Manager)

The frontend reads the public API URL at **container runtime** via `GEOS_API_BASE_URL`
(injected into `/config.js` on startup — no image rebuild when this changes).

| NPM layout | `GEOS_API_BASE_URL` | Notes |
| --- | --- | --- |
| One host, `/api` → geos-api | _(empty)_ | Browser calls same-origin `/api/v1/...`. Either NPM proxies `/api` to `geos-api:8080`, or NPM points at the frontend container and its nginx proxies `/api` internally. |
| Separate API host | `https://api.your-domain.example` | Browser calls the API directly (REST, WebSocket, tile imagery). CORS is permissive today. |

After changing `.env`, restart the frontend container:

```bash
docker compose up -d frontend
```

## Dev scripts

| Script | Purpose |
| --- | --- |
| `scripts/dev.sh` | Start data services + API/workers/frontend for local dev |
| `scripts/db.sh` | Manage the dev Postgres container (`up`/`down`/`logs`/`psql`) |
| `scripts/db-backup.sh` / `scripts/db-restore.sh` | Back up / restore the database |
| `scripts/lint.sh` | Run fmt + clippy + eslint + tsc (mirrors hooks/CI) |
| `scripts/gen-types.sh` | Export Event JSON Schema → TS types *(added with the schema)* |
| `scripts/gen-contour-tiles.sh` | Generate the offline ETOPO contour basemap tiles (needs GDAL ≥ 3.5) |
| `scripts/seed.sh` | Seed sample data *(added with connectors)* |
| `scripts/submodule-init.sh` | Initialize the nebular-os submodule |

## Globe imagery (Cesium + caching tile proxy)

The globe uses **CesiumJS** with a local **Sentinel-2** tile pyramid. The API
exposes a caching tile proxy at `GET /api/v1/tiles/sentinel2/{z}/{x}/{y}.jpg`
(public, token-gated): a storage hit in nebular-os (bucket `geos-tiles`) is
served directly; on a miss the tile is fetched once from EOX Sentinel-2 cloudless
(CC-BY 4.0), served, and persisted, so the local dataset grows lazily as areas
are viewed. Max zoom is **z12** (~38 m/px), enforced server-side. The browser
gets a medium-lived imagery token from `GET /api/v1/tiles/session`.

Relevant env (see `.env.example`): `NOS_JWT_SECRET` (signs the service token the
API uses for the tile bucket), `TILES_BUCKET`, `TILES_MAX_ZOOM`,
`IMAGERY_UPSTREAM_URL`.

**Offline fallback:** run `scripts/gen-contour-tiles.sh` once (GDAL ≥ 3.5) to
build an elevation-contour basemap from the public-domain ETOPO 2022 DEM into
`frontend/public/contours/`. Cesium draws it beneath the Sentinel layer, so when
upstream imagery is unavailable the globe still renders with no network.

## Quality gates

Quality is mechanically enforced via [`lefthook.yml`](./lefthook.yml) (and
mirrored in CI): `cargo fmt`, `cargo clippy -D warnings`, ESLint, `tsc`, secret
scanning on commit, and the test suites on push. Project conventions live in
[`.cursor/rules/`](./.cursor/rules) — notably the canonical event schema, tenant
isolation, and the definition of done.

## Status

Project scaffold. The implementation plan's `scaffold` step is in place
(workspace, frontend, infra, rules, hooks). Subsequent steps — core schema,
connectors, API, frontend globe, correlation, CLI — follow on `feature/*`
branches per [`.cursor/rules/git-commits.mdc`](./.cursor/rules/git-commits.mdc).
