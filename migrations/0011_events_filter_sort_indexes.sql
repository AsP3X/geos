-- 0011 — Indexes supporting power-filter sorts and the distinct-sources endpoint.
--
-- Every events query is tenant-scoped (tenant_id IN (caller, system)), so the
-- sort indexes are tenant-prefixed; otherwise the planner ignores a bare
-- (impact_score)/(magnitude) index under the tenant predicate. Declared on the
-- partitioned parent so existing and future monthly partitions inherit them
-- (geospatial-postgis.mdc, api-sqlx-migrations.mdc).

-- Impact-desc sort (sort = impact_desc).
CREATE INDEX events_tenant_impact_idx ON events (tenant_id, impact_score DESC);

-- Magnitude-desc sort (sort = magnitude_desc). Partial because magnitude sorts
-- and any active magnitude range exclude NULL magnitudes.
CREATE INDEX events_tenant_magnitude_idx ON events (tenant_id, magnitude DESC)
    WHERE magnitude IS NOT NULL;

-- Backs GET /api/v1/events/sources (SELECT DISTINCT source ... per tenant).
CREATE INDEX events_tenant_source_idx ON events (tenant_id, source);
