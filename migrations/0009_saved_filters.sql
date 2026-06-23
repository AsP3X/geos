-- 0009 — Saved filter presets (per-user, private).
--
-- Named, reproducible event-filter presets owned by a single user within a
-- tenant (power-filter feature). RBAC-gated by saved_filters.read/manage
-- (migration 0002). The filters payload is the versioned EventFilters JSON
-- (validated server-side as SavedFilterPayload, not opaque jsonb).
--
-- ON DELETE CASCADE to tenants/users so tenant or user deletion (incl. GDPR
-- erasure, privacy-gdpr.mdc) removes presets automatically; the erasure path
-- also lists this table explicitly.

CREATE TABLE saved_filters (
    id          uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text        NOT NULL,
    filters     jsonb       NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    -- Per-user uniqueness powers overwrite-on-name-conflict upserts.
    UNIQUE (tenant_id, user_id, name)
);

CREATE INDEX saved_filters_tenant_user_idx ON saved_filters (tenant_id, user_id);
