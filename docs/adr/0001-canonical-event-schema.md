# 1. Canonical Event schema and storage

- Status: accepted
- Date: 2026-06-22

## Context

Geos ingests heterogeneous OSINT/public-source records and must store them in a
single normalized shape so the API, frontend, CLI, correlation, and search all
operate on one contract. The shape, its storage, and its cross-language
representation need to stay in sync and scale to high ingestion volume.

## Decision

- **Single source of truth in Rust.** The canonical model is
  `geos_core::events::Event` in `crates/core`. The JSON Schema
  (`crates/core/schema/event.schema.json`) and the frontend TypeScript types
  (`frontend/src/types/event.ts`) are generated from it via
  `scripts/gen-types.sh`. A `geos-core` unit test fails on schema drift.
- **Closed, versioned enums** for `category`, `severity`, `status`, and
  `verification_status`, mirrored as PostgreSQL enum types.
- **Geospatial types (SRID 4326):** `location` is `geography(Point,4326)` (metric
  `ST_DWithin`); `affected_area` is `geometry(Polygon,4326)`. GiST indexes on
  both.
- **Semantic vector:** `embedding vector(1024)` with a cosine HNSW index. The
  dimension is fixed by `geos_core::events::EMBEDDING_DIM = 1024` and must match
  the embedding model.
- **Monthly time partitioning** of `events` by `occurred_at`. Because Postgres
  requires the partition key in every unique constraint and the primary key, the
  PK is `(id, occurred_at)` and the connector idempotency key is
  `(tenant_id, source, source_event_id, occurred_at)`.
- **Raw retention:** the original payload is always kept in `raw` (jsonb);
  source-specific fields live there rather than forking the canonical type.
- **Tenant scoping:** every row carries `tenant_id`. The foreign key to a
  `tenants` table is deferred to a later migration (additive) when that table
  exists; scoping is enforced in queries regardless.

## Alternatives considered

- **Per-source tables / ad-hoc shapes:** rejected — fragments queries and breaks
  the single-contract goal.
- **Unpartitioned `events`:** rejected — a painful migration later under load;
  monthly partitions from day one keep map/time queries prune-friendly.
- **Storing lat/lon as floats** instead of PostGIS geography: rejected — loses
  spatial indexing and metric distance.

## Consequences

- Any change to the `Event` shape must update four artifacts together (Rust
  struct, JSON Schema, TS types, a new migration) per
  `canonical-event-schema.mdc`; CI/the drift test enforces this.
- A partition-maintenance routine must create future monthly partitions (added
  with the workers scheduler). The initial migration pre-creates 2024-01..2026-12.
- Changing `EMBEDDING_DIM` is a breaking change requiring data migration.
