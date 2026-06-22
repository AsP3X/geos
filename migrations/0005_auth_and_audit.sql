-- 0005 — Auth session tokens, MFA/password-reset scaffolding, API keys, audit log.
--
-- Supports JWT access + refresh rotation (api slice), compliance audit trail
-- (audit-log-coverage.mdc), and future MFA/API-key flows.

CREATE TABLE refresh_tokens (
    id          uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash  text        NOT NULL UNIQUE,
    expires_at  timestamptz NOT NULL,
    revoked_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX refresh_tokens_user_idx ON refresh_tokens (user_id);

CREATE TABLE password_reset_tokens (
    id          uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    user_id     uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash  text        NOT NULL UNIQUE,
    expires_at  timestamptz NOT NULL,
    used_at     timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE mfa_secrets (
    user_id     uuid        NOT NULL PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    secret_enc  text        NOT NULL,
    enabled_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE api_keys (
    id          uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    name        text        NOT NULL,
    key_prefix  text        NOT NULL,
    key_hash    text        NOT NULL UNIQUE,
    created_by  uuid        REFERENCES users (id) ON DELETE SET NULL,
    last_used_at timestamptz,
    revoked_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);

CREATE INDEX api_keys_tenant_idx ON api_keys (tenant_id);

CREATE TABLE audit_log (
    id             uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id      uuid        REFERENCES tenants (id) ON DELETE SET NULL,
    actor_user_id  uuid        REFERENCES users (id) ON DELETE SET NULL,
    action         text        NOT NULL,
    resource_type  text,
    resource_id    text,
    context        jsonb       NOT NULL DEFAULT '{}',
    ip_address     inet,
    user_agent     text,
    request_id     text,
    created_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_log_tenant_created_idx ON audit_log (tenant_id, created_at DESC);
CREATE INDEX audit_log_actor_created_idx  ON audit_log (actor_user_id, created_at DESC);
