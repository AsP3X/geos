//! Role-based access control: the permission catalog and default role presets.
//!
//! [`Permission`] is the typed source of truth for permission keys; the same
//! keys are seeded into the `permissions` table by migration 0002. RBAC is
//! enforced *within* a tenant and is a separate gate from tenant isolation
//! (`tenant-isolation.mdc`). The presets here are materialized as per-tenant
//! rows when a tenant is created (done in the API/auth slice).

use serde::{Deserialize, Serialize};

/// A single, closed permission key. Serializes to its dotted string form
/// (e.g. `"events.read"`) so API payloads and the DB catalog stay aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Permission {
    /// View events.
    #[serde(rename = "events.read")]
    EventsRead,
    /// Create manual events.
    #[serde(rename = "events.create")]
    EventsCreate,
    /// Edit events.
    #[serde(rename = "events.update")]
    EventsUpdate,
    /// Delete events.
    #[serde(rename = "events.delete")]
    EventsDelete,
    /// Change event verification status.
    #[serde(rename = "events.verify")]
    EventsVerify,
    /// View event relations and graph.
    #[serde(rename = "relations.read")]
    RelationsRead,
    /// View entities.
    #[serde(rename = "entities.read")]
    EntitiesRead,
    /// Run search queries.
    #[serde(rename = "search.read")]
    SearchRead,
    /// View saved filters.
    #[serde(rename = "saved_filters.read")]
    SavedFiltersRead,
    /// Create and edit saved filters.
    #[serde(rename = "saved_filters.manage")]
    SavedFiltersManage,
    /// View areas of interest.
    #[serde(rename = "aoi.read")]
    AoiRead,
    /// Create and edit areas of interest.
    #[serde(rename = "aoi.manage")]
    AoiManage,
    /// Add annotations to events.
    #[serde(rename = "annotations.create")]
    AnnotationsCreate,
    /// Export data (GeoJSON/CSV).
    #[serde(rename = "exports.create")]
    ExportsCreate,
    /// View tenant members.
    #[serde(rename = "members.read")]
    MembersRead,
    /// Invite and manage tenant members.
    #[serde(rename = "members.manage")]
    MembersManage,
    /// View roles and permissions.
    #[serde(rename = "rbac.read")]
    RbacRead,
    /// Create and edit roles and grants.
    #[serde(rename = "rbac.manage")]
    RbacManage,
    /// View tenant settings.
    #[serde(rename = "tenant.settings.read")]
    TenantSettingsRead,
    /// Edit tenant settings and billing.
    #[serde(rename = "tenant.settings.manage")]
    TenantSettingsManage,
}

impl Permission {
    /// Every permission in the catalog. Must match the keys seeded by
    /// migration 0002 (`permissions` table).
    pub const ALL: [Permission; 20] = [
        Permission::EventsRead,
        Permission::EventsCreate,
        Permission::EventsUpdate,
        Permission::EventsDelete,
        Permission::EventsVerify,
        Permission::RelationsRead,
        Permission::EntitiesRead,
        Permission::SearchRead,
        Permission::SavedFiltersRead,
        Permission::SavedFiltersManage,
        Permission::AoiRead,
        Permission::AoiManage,
        Permission::AnnotationsCreate,
        Permission::ExportsCreate,
        Permission::MembersRead,
        Permission::MembersManage,
        Permission::RbacRead,
        Permission::RbacManage,
        Permission::TenantSettingsRead,
        Permission::TenantSettingsManage,
    ];

    /// The dotted string key for this permission (DB catalog key).
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::EventsRead => "events.read",
            Permission::EventsCreate => "events.create",
            Permission::EventsUpdate => "events.update",
            Permission::EventsDelete => "events.delete",
            Permission::EventsVerify => "events.verify",
            Permission::RelationsRead => "relations.read",
            Permission::EntitiesRead => "entities.read",
            Permission::SearchRead => "search.read",
            Permission::SavedFiltersRead => "saved_filters.read",
            Permission::SavedFiltersManage => "saved_filters.manage",
            Permission::AoiRead => "aoi.read",
            Permission::AoiManage => "aoi.manage",
            Permission::AnnotationsCreate => "annotations.create",
            Permission::ExportsCreate => "exports.create",
            Permission::MembersRead => "members.read",
            Permission::MembersManage => "members.manage",
            Permission::RbacRead => "rbac.read",
            Permission::RbacManage => "rbac.manage",
            Permission::TenantSettingsRead => "tenant.settings.read",
            Permission::TenantSettingsManage => "tenant.settings.manage",
        }
    }
}

/// The built-in role presets seeded for every new tenant. Custom roles can be
/// defined later on top of the same permission catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RolePreset {
    /// Full control including members, roles, and tenant settings/billing.
    Owner,
    /// Can read everything and create/edit/verify events, annotations, filters,
    /// AOIs, and exports — but not manage members, roles, or tenant settings.
    Analyst,
    /// Read-only access.
    Viewer,
}

impl RolePreset {
    /// All presets, in seeding order.
    pub const ALL: [RolePreset; 3] = [RolePreset::Owner, RolePreset::Analyst, RolePreset::Viewer];

    /// Stable role key stored in `roles.key`.
    pub fn key(&self) -> &'static str {
        match self {
            RolePreset::Owner => "owner",
            RolePreset::Analyst => "analyst",
            RolePreset::Viewer => "viewer",
        }
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            RolePreset::Owner => "Owner",
            RolePreset::Analyst => "Analyst",
            RolePreset::Viewer => "Viewer",
        }
    }

    /// The permissions granted to this preset.
    ///
    /// `Owner` holds every permission; `Viewer` holds only the read-only
    /// subset; `Analyst` sits in between (read + content authoring, no
    /// administration).
    pub fn permissions(&self) -> Vec<Permission> {
        match self {
            RolePreset::Owner => Permission::ALL.to_vec(),
            RolePreset::Viewer => VIEWER_PERMISSIONS.to_vec(),
            RolePreset::Analyst => ANALYST_PERMISSIONS.to_vec(),
        }
    }
}

// Human: Read-only subset shared by Viewer and (as a base) Analyst.
// Agent: CONST read perms; used by RolePreset::permissions.
const VIEWER_PERMISSIONS: [Permission; 7] = [
    Permission::EventsRead,
    Permission::RelationsRead,
    Permission::EntitiesRead,
    Permission::SearchRead,
    Permission::SavedFiltersRead,
    Permission::AoiRead,
    Permission::MembersRead,
];

// Human: Analyst = read-only subset plus content authoring (no member/role/
// tenant administration).
// Agent: CONST analyst perms; superset of VIEWER_PERMISSIONS minus admin perms.
const ANALYST_PERMISSIONS: [Permission; 14] = [
    Permission::EventsRead,
    Permission::EventsCreate,
    Permission::EventsUpdate,
    Permission::EventsVerify,
    Permission::RelationsRead,
    Permission::EntitiesRead,
    Permission::SearchRead,
    Permission::SavedFiltersRead,
    Permission::SavedFiltersManage,
    Permission::AoiRead,
    Permission::AoiManage,
    Permission::AnnotationsCreate,
    Permission::ExportsCreate,
    Permission::MembersRead,
];
