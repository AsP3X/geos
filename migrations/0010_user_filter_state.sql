-- 0010 — Per-user active filter state (server-authoritative).
--
-- The active event filter persists per user on the server (source of truth);
-- the frontend caches it in localStorage only for instant first paint and
-- reconciles against this row on load. One row per (tenant_id, user_id).
--
-- ON DELETE CASCADE to tenants/users mirrors saved_filters (0009): tenant or
-- user deletion (incl. GDPR erasure, privacy-gdpr.mdc) removes the row.

CREATE TABLE user_filter_state (
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    filters     jsonb       NOT NULL,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id)
);
