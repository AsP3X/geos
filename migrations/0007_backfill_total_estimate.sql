-- 0007 — Backfill total-count estimate for accurate progress.
--
-- Backfill progress was previously time-based (cursor position across the target
-- window). For sources that can report how many events match a window up front
-- (USGS FDSNWS `count`), we store that total so the UI can show true count-based
-- progress: events_ingested / backfill_total_estimate. NULL means "unknown",
-- in which case consumers fall back to the time-based estimate.

ALTER TABLE connectors_state
    ADD COLUMN backfill_total_estimate bigint
        CHECK (backfill_total_estimate IS NULL OR backfill_total_estimate >= 0);
