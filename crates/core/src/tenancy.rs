//! Tenancy and membership domain types — the Rust mirror of the tenants/users/
//! roles/memberships tables (migration 0002).
//!
//! These are the API-facing representations. Note [`User`] intentionally omits
//! `password_hash`: secrets never live on a serializable domain type and are
//! never sent to clients (`api-error-shape.mdc`). The SQLx row mappings and
//! secret-bearing auth records are added in the API/auth slice.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tenant that owns globally ingested public connector feeds (shared OSINT).
///
/// Not a customer organization; events from public connectors (USGS, weather,
/// …) are stored under this tenant until per-tenant private sources exist.
pub const SYSTEM_TENANT_ID: Uuid = uuid::uuid!("00000000-0000-4000-8000-000000000001");

/// Lifecycle status of a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantStatus {
    /// Active and usable.
    Active,
    /// Suspended (e.g. billing/abuse); access blocked but data retained.
    Suspended,
    /// Marked for deletion / offboarded.
    Deleted,
}

/// Lifecycle status of a user account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    /// Active and able to authenticate.
    Active,
    /// Disabled; cannot authenticate.
    Disabled,
}

/// Status of a user's membership within a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    /// Active member.
    Active,
    /// Invited but not yet accepted.
    Invited,
    /// Suspended within this tenant.
    Suspended,
}

/// A tenant (organization / workspace).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    /// Unique identifier.
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// URL-safe, case-insensitive unique slug.
    pub slug: String,
    /// Lifecycle status.
    pub status: TenantStatus,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last-update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// A global user account (may belong to multiple tenants via memberships).
///
/// Excludes credential material by design; see the module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique identifier.
    pub id: Uuid,
    /// Case-insensitive unique email.
    pub email: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// Account status.
    pub status: UserStatus,
    /// Whether the email has been verified.
    pub email_verified: bool,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last-update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// A per-tenant role. `key` matches a [`crate::rbac::RolePreset`] key for system
/// presets, or a custom value for user-defined roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Unique identifier.
    pub id: Uuid,
    /// Owning tenant.
    pub tenant_id: Uuid,
    /// Stable role key, unique within the tenant.
    pub key: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Whether this is a system preset (not deletable).
    pub is_system: bool,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// A user's membership (and role) within one tenant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    /// Unique identifier.
    pub id: Uuid,
    /// Tenant the membership belongs to.
    pub tenant_id: Uuid,
    /// The member user.
    pub user_id: Uuid,
    /// The role granted to the member within this tenant.
    pub role_id: Uuid,
    /// Membership status.
    pub status: MembershipStatus,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last-update timestamp.
    pub updated_at: DateTime<Utc>,
}
