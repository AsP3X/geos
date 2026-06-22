# Geos

Geos is a multi-tenant SaaS for ingesting, normalizing, correlating, and
visualizing OSINT / public-source events (earthquakes, incidents, alerts,
weather, news, conflict) on an interactive 3D globe. The backend, workers, and
CLI are written in Rust; the frontend is React + Vite with a react-three-fiber
globe.

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
| Frontend | `frontend/` | React + Vite + TypeScript + Tailwind v4 + shadcn/ui + react-three-fiber |
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

# Or run the full stack in containers
docker compose up --build
```

| Service | Default URL |
| --- | --- |
| API | http://localhost:8080 |
| Frontend | http://localhost:4173 (preview) / 5173 (dev) |
| Meilisearch | http://localhost:7700 |
| Object storage (nebular-os) | http://localhost:9000 |
| Postgres | localhost:5432 |

## Dev scripts

| Script | Purpose |
| --- | --- |
| `scripts/dev.sh` | Start data services + API/workers/frontend for local dev |
| `scripts/db.sh` | Manage the dev Postgres container (`up`/`down`/`logs`/`psql`) |
| `scripts/db-backup.sh` / `scripts/db-restore.sh` | Back up / restore the database |
| `scripts/lint.sh` | Run fmt + clippy + eslint + tsc (mirrors hooks/CI) |
| `scripts/gen-types.sh` | Export Event JSON Schema → TS types *(added with the schema)* |
| `scripts/seed.sh` | Seed sample data *(added with connectors)* |
| `scripts/submodule-init.sh` | Initialize the nebular-os submodule |

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
