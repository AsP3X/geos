-- 0006 — Connector historical backfill: progress tracking + on-demand partitions.
--
-- Adds resumable backfill bookkeeping to connectors_state so the workers can walk
-- a source's history in chunks, advance a cursor, and report how much has been
-- pulled vs. how much remains (connector-contract.mdc). Also registers the NWS
-- weather source (previously only USGS was seeded) and introduces a partition
-- maintenance routine so backfilling far into the past does not fail inserts.
--
-- The events table is RANGE-partitioned monthly by occurred_at and migration 0001
-- only pre-created 2024-01..2026-12. Historical backfill (e.g. USGS back to 1900)
-- requires the matching monthly partitions to exist first; ensure_events_partitions
-- creates them on demand. Migration 0001 anticipated this "maintenance routine
-- added with the workers scheduler".

-- ── Backfill progress columns ────────────────────────────────────────────────
-- last_backfill_cursor + backfill_complete already exist (migration 0003).
-- Cursor semantics: oldest occurred_at the backfill has reached (walks newest ->
-- oldest); backfill is complete once the cursor reaches backfill_window_start.
ALTER TABLE connectors_state
    ADD COLUMN backfill_window_start    timestamptz,
    ADD COLUMN backfill_window_end      timestamptz,
    ADD COLUMN backfill_started_at      timestamptz,
    ADD COLUMN backfill_completed_at    timestamptz,
    ADD COLUMN backfill_events_ingested bigint NOT NULL DEFAULT 0
        CHECK (backfill_events_ingested >= 0);

-- ── Register the NWS weather source + system-tenant connector state ──────────
INSERT INTO sources (key, name, description, reliability_score, license_notes)
VALUES (
    'nws',
    'NWS Weather Alerts',
    'US National Weather Service active weather alerts (CAP/GeoJSON).',
    0.90,
    'NWS/NOAA data are public domain; attribution per NWS API policy. Public API exposes only active and last-7-day alerts.'
)
ON CONFLICT (key) DO NOTHING;

INSERT INTO connectors_state (tenant_id, source_key)
VALUES ('00000000-0000-4000-8000-000000000001', 'nws')
ON CONFLICT (tenant_id, source_key) DO NOTHING;

-- ── On-demand monthly partition maintenance ──────────────────────────────────
-- Creates every monthly partition covering [range_start, range_end] if missing.
-- Idempotent (CREATE TABLE IF NOT EXISTS); returns the number of months touched.
CREATE OR REPLACE FUNCTION ensure_events_partitions(
    range_start timestamptz,
    range_end   timestamptz
) RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    m         date := date_trunc('month', LEAST(range_start, range_end))::date;
    stop      date := date_trunc('month', GREATEST(range_start, range_end))::date;
    part_name text;
    touched   integer := 0;
BEGIN
    WHILE m <= stop LOOP
        part_name := format('events_%s', to_char(m, 'YYYY_MM'));
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF events FOR VALUES FROM (%L) TO (%L);',
            part_name, m, (m + interval '1 month')::date
        );
        touched := touched + 1;
        m := (m + interval '1 month')::date;
    END LOOP;
    RETURN touched;
END;
$$;
