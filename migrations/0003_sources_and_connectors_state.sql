-- 0003 — Ingestion metadata: sources catalog, connector cursors, system tenant seed.
--
-- Supports the worker pipeline (connector-contract.mdc): track source attribution,
-- reliability, and per-tenant connector state for live polls and backfill cursors.
-- Seeds the system tenant (SYSTEM_TENANT_ID) used for shared public feeds.

-- System tenant for globally ingested public connector data (see geos_core::tenancy).
INSERT INTO tenants (id, name, slug, status)
VALUES (
    '00000000-0000-4000-8000-000000000001',
    'System',
    'system',
    'active'
)
ON CONFLICT (id) DO NOTHING;

CREATE TABLE sources (
    key               text    NOT NULL PRIMARY KEY,
    name              text    NOT NULL,
    description       text,
    reliability_score real    NOT NULL DEFAULT 0.8
                          CHECK (reliability_score >= 0 AND reliability_score <= 1),
    license_notes     text,
    enabled           boolean NOT NULL DEFAULT true,
    created_at        timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE connectors_state (
    tenant_id            uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    source_key           text        NOT NULL REFERENCES sources (key),
    last_live_run_at     timestamptz,
    last_backfill_cursor timestamptz,
    backfill_complete    boolean     NOT NULL DEFAULT false,
    updated_at           timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, source_key)
);

-- Seed the USGS earthquake source (first connector).
INSERT INTO sources (key, name, description, reliability_score, license_notes)
VALUES (
    'usgs',
    'USGS Earthquakes',
    'United States Geological Survey earthquake feeds (GeoJSON).',
    0.95,
    'USGS data are public domain; attribution required per USGS guidance.'
);

-- Register USGS connector state for the system tenant.
INSERT INTO connectors_state (tenant_id, source_key)
VALUES ('00000000-0000-4000-8000-000000000001', 'usgs')
ON CONFLICT (tenant_id, source_key) DO NOTHING;
