-- 0001 — Extensions + canonical events table (monthly time-partitioned).
--
-- This migration establishes the canonical `events` storage that mirrors
-- geos_core::events::Event (canonical-event-schema.mdc). It is IMMUTABLE once
-- applied; later schema changes go in new numbered migrations
-- (api-sqlx-migrations.mdc).
--
-- Conventions (geospatial-postgis.mdc):
--   * SRID 4326 everywhere; location is geography(Point,4326) so ST_DWithin is
--     in metres; affected_area is geometry(Polygon,4326).
--   * GiST indexes on spatial columns; HNSW (cosine) index on the embedding.
--   * Table is RANGE-partitioned monthly by occurred_at; the partition key must
--     appear in the primary key and every unique constraint, which is why both
--     include occurred_at.

-- Required extensions — do not assume they pre-exist.
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS vector;

-- Closed, versioned enums mirroring the Rust types. Adding a value requires a
-- new migration plus updates to the Rust enum, JSON schema, TS types, and UI.
CREATE TYPE event_category AS ENUM (
    'earthquake', 'incident', 'alert', 'weather', 'news', 'conflict', 'wildfire', 'other'
);
CREATE TYPE event_severity AS ENUM ('info', 'low', 'moderate', 'high', 'critical');
CREATE TYPE event_status AS ENUM ('active', 'resolved', 'archived');
CREATE TYPE verification_status AS ENUM ('verified', 'unverified', 'rumor', 'disputed');

-- Canonical events table. tenant_id is intentionally NOT yet a foreign key:
-- the tenants table is introduced in a later migration, at which point the FK
-- is added (additive change). Every query remains tenant-scoped regardless
-- (tenant-isolation.mdc).
CREATE TABLE events (
    id                  uuid              NOT NULL DEFAULT gen_random_uuid(),
    tenant_id           uuid              NOT NULL,
    source              text              NOT NULL,
    source_event_id     text              NOT NULL,
    category            event_category    NOT NULL,
    severity            event_severity    NOT NULL,
    impact_score        smallint          NOT NULL DEFAULT 0
                            CHECK (impact_score BETWEEN 0 AND 100),
    magnitude           double precision,
    title               text,
    summary             text,
    body                text,
    original_text       text,
    translated_text     text,
    language            text,
    location            geography(Point, 4326) NOT NULL,
    affected_area       geometry(Polygon, 4326),
    country             text,
    region              text,
    place_name          text,
    occurred_at         timestamptz       NOT NULL,
    detected_at         timestamptz,
    ingested_at         timestamptz       NOT NULL DEFAULT now(),
    status              event_status      NOT NULL DEFAULT 'active',
    verification_status verification_status NOT NULL DEFAULT 'unverified',
    confidence          real              NOT NULL DEFAULT 0
                            CHECK (confidence BETWEEN 0 AND 1),
    tags                text[]            NOT NULL DEFAULT '{}',
    url                 text,
    raw                 jsonb             NOT NULL DEFAULT '{}'::jsonb,
    -- Dimension must match geos_core::events::EMBEDDING_DIM.
    embedding           vector(1024),
    PRIMARY KEY (id, occurred_at),
    -- Idempotency key for connector upserts (connector-contract.mdc); includes
    -- occurred_at because the partition key must be part of unique constraints.
    UNIQUE (tenant_id, source, source_event_id, occurred_at)
) PARTITION BY RANGE (occurred_at);

-- Indexes are declared on the partitioned parent so existing and future
-- partitions inherit them automatically.
CREATE INDEX events_location_gist     ON events USING gist (location);
CREATE INDEX events_affected_area_gist ON events USING gist (affected_area);
CREATE INDEX events_occurred_at_idx    ON events (occurred_at);
CREATE INDEX events_tenant_time_idx    ON events (tenant_id, occurred_at DESC);
CREATE INDEX events_tenant_category_idx ON events (tenant_id, category);
CREATE INDEX events_tenant_severity_idx ON events (tenant_id, severity);
CREATE INDEX events_tags_gin           ON events USING gin (tags);
-- Cosine HNSW index for semantic similarity / related-events search.
CREATE INDEX events_embedding_hnsw     ON events USING hnsw (embedding vector_cosine_ops);

-- Pre-create monthly partitions across a working range. A maintenance routine
-- (added with the workers scheduler) extends this window over time; queries
-- should constrain occurred_at to enable partition pruning.
DO $$
DECLARE
    m          date := date '2024-01-01';
    end_month  date := date '2027-01-01';
    part_name  text;
BEGIN
    WHILE m < end_month LOOP
        part_name := format('events_%s', to_char(m, 'YYYY_MM'));
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF events FOR VALUES FROM (%L) TO (%L);',
            part_name, m, (m + interval '1 month')::date
        );
        m := (m + interval '1 month')::date;
    END LOOP;
END $$;
