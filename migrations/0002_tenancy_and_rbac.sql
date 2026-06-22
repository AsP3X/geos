-- 0002 — Multi-tenancy and RBAC foundation.
--
-- Adds tenants, global users, platform super-admins, per-tenant roles, a global
-- permission catalog, role/permission grants, and tenant memberships. Also wires
-- the deferred events.tenant_id foreign key now that tenants exists.
--
-- IMMUTABLE once applied (api-sqlx-migrations.mdc). Tenant scoping is enforced in
-- queries (tenant-isolation.mdc); super-admins are a separate, audited path and
-- are NOT a tenant role.

-- Case-insensitive text for emails (unique regardless of case).
CREATE EXTENSION IF NOT EXISTS citext;

CREATE TYPE tenant_status AS ENUM ('active', 'suspended', 'deleted');
CREATE TYPE user_status AS ENUM ('active', 'disabled');
CREATE TYPE membership_status AS ENUM ('active', 'invited', 'suspended');

-- Tenants (organizations / workspaces).
CREATE TABLE tenants (
    id          uuid          NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    name        text          NOT NULL,
    slug        citext        NOT NULL UNIQUE,
    status      tenant_status NOT NULL DEFAULT 'active',
    created_at  timestamptz   NOT NULL DEFAULT now(),
    updated_at  timestamptz   NOT NULL DEFAULT now()
);

-- Global user accounts. A user may belong to multiple tenants via memberships.
-- password_hash is nullable to leave room for future SSO-only accounts; argon2
-- hashing and auth flows are added in the API/auth slice.
CREATE TABLE users (
    id             uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    email          citext      NOT NULL UNIQUE,
    password_hash  text,
    display_name   text,
    status         user_status NOT NULL DEFAULT 'active',
    email_verified boolean     NOT NULL DEFAULT false,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

-- Platform operators. Deliberately separate from tenant RBAC; never reachable
-- through tenant roles (tenant-isolation.mdc).
CREATE TABLE super_admins (
    user_id    uuid        NOT NULL PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Global, closed permission catalog. Keys mirror geos_core::rbac::Permission;
-- adding a key requires updating both in the same change set.
CREATE TABLE permissions (
    key         text NOT NULL PRIMARY KEY,
    description text NOT NULL
);

-- Per-tenant roles. Default presets (Owner/Analyst/Viewer) are seeded by the
-- application on tenant creation, not here, because they are tenant-scoped rows.
CREATE TABLE roles (
    id          uuid        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id   uuid        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    key         text        NOT NULL,
    name        text        NOT NULL,
    description text,
    is_system   boolean     NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, key)
);

CREATE INDEX roles_tenant_idx ON roles (tenant_id);

-- Grants: which permissions a role holds.
CREATE TABLE role_permissions (
    role_id        uuid NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    permission_key text NOT NULL REFERENCES permissions (key) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_key)
);

-- Tenant membership: a user's role within one tenant. One membership per
-- (tenant, user).
CREATE TABLE memberships (
    id         uuid              NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id  uuid              NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id    uuid              NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role_id    uuid              NOT NULL REFERENCES roles (id),
    status     membership_status NOT NULL DEFAULT 'active',
    created_at timestamptz       NOT NULL DEFAULT now(),
    updated_at timestamptz       NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, user_id)
);

CREATE INDEX memberships_user_idx   ON memberships (user_id);
CREATE INDEX memberships_tenant_idx ON memberships (tenant_id);

-- Wire the events tenant FK deferred from migration 0001. RESTRICT so a tenant
-- cannot be dropped while it still owns events; GDPR erasure deletes events
-- explicitly via the audited deletion path (privacy-gdpr.mdc).
ALTER TABLE events
    ADD CONSTRAINT events_tenant_id_fkey
    FOREIGN KEY (tenant_id) REFERENCES tenants (id) ON DELETE RESTRICT;

-- Seed the global permission catalog. Keep in sync with geos_core::rbac::Permission.
INSERT INTO permissions (key, description) VALUES
    ('events.read',            'View events'),
    ('events.create',          'Create manual events'),
    ('events.update',          'Edit events'),
    ('events.delete',          'Delete events'),
    ('events.verify',          'Change event verification status'),
    ('relations.read',         'View event relations and graph'),
    ('entities.read',          'View entities'),
    ('search.read',            'Run search queries'),
    ('saved_filters.read',     'View saved filters'),
    ('saved_filters.manage',   'Create and edit saved filters'),
    ('aoi.read',               'View areas of interest'),
    ('aoi.manage',             'Create and edit areas of interest'),
    ('annotations.create',     'Add annotations to events'),
    ('exports.create',         'Export data (GeoJSON/CSV)'),
    ('members.read',           'View tenant members'),
    ('members.manage',         'Invite and manage tenant members'),
    ('rbac.read',              'View roles and permissions'),
    ('rbac.manage',            'Create and edit roles and grants'),
    ('tenant.settings.read',   'View tenant settings'),
    ('tenant.settings.manage', 'Edit tenant settings and billing');
