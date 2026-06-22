-- 0008 — Incremental backfill total-count progress cursor.
--
-- Some sources (USGS FDSNWS) cannot count a very large range in one request, so
-- the total denominator is built incrementally by counting bounded sub-windows
-- newest -> oldest and summing into `backfill_total_estimate`.
--
-- `backfill_count_cursor` is the oldest time the counting pass has reached:
--   * NULL                      -> counting not started
--   * > backfill_window_start   -> counting in progress (total is partial)
--   * <= backfill_window_start  -> counting complete (total is final)
-- Consumers only treat `backfill_total_estimate` as final once the cursor has
-- reached the window start; until then progress stays time-/events-based.

ALTER TABLE connectors_state
    ADD COLUMN backfill_count_cursor timestamptz;
