//! Tenant creation with RBAC preset seeding.

use sqlx::PgPool;
use uuid::Uuid;

use crate::rbac::RolePreset;
use crate::Result;

/// Create a tenant, seed Owner/Analyst/Viewer roles, and return the Owner role id.
pub async fn create_tenant_with_roles(
    pool: &PgPool,
    name: &str,
    slug: &str,
) -> Result<(Uuid, Uuid)> {
    let mut tx = pool.begin().await?;
    let tenant_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO tenants (name, slug)
        VALUES ($1, $2)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(slug)
    .fetch_one(&mut *tx)
    .await?;

    let mut owner_role_id = None;
    for preset in RolePreset::ALL {
        let role_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO roles (tenant_id, key, name, is_system)
            VALUES ($1, $2, $3, true)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(preset.key())
        .bind(preset.display_name())
        .fetch_one(&mut *tx)
        .await?;

        for permission in preset.permissions() {
            sqlx::query(
                r#"
                INSERT INTO role_permissions (role_id, permission_key)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(role_id)
            .bind(permission.as_str())
            .execute(&mut *tx)
            .await?;
        }

        if preset == RolePreset::Owner {
            owner_role_id = Some(role_id);
        }
    }

    tx.commit().await?;
    let owner_role_id =
        owner_role_id.ok_or_else(|| crate::error::AppError::internal("owner role not seeded"))?;
    Ok((tenant_id, owner_role_id))
}

/// Add an active membership linking a user to a tenant role.
pub async fn insert_membership(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    role_id: Uuid,
) -> Result<Uuid> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO memberships (tenant_id, user_id, role_id, status)
        VALUES ($1, $2, $3, 'active')
        RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Resolve tenant id by slug.
pub async fn find_tenant_id_by_slug(pool: &PgPool, slug: &str) -> Result<Option<Uuid>> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM tenants WHERE slug = $1 AND status = 'active'"#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// Membership + role for login when the user belongs to a tenant.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MembershipAuthRow {
    /// Membership primary key.
    pub membership_id: Uuid,
    /// Tenant primary key.
    pub tenant_id: Uuid,
    /// Tenant slug.
    pub tenant_slug: String,
    /// Role key (e.g. `owner`).
    pub role_key: String,
}

/// Fetch membership for `(user_id, tenant_slug)`.
pub async fn find_membership_for_user(
    pool: &PgPool,
    user_id: Uuid,
    tenant_slug: &str,
) -> Result<Option<MembershipAuthRow>> {
    let row = sqlx::query_as::<_, MembershipAuthRow>(
        r#"
        SELECT
            m.id AS membership_id,
            t.id AS tenant_id,
            t.slug::text AS tenant_slug,
            r.key AS role_key
        FROM memberships m
        JOIN tenants t ON t.id = m.tenant_id
        JOIN roles r ON r.id = m.role_id
        WHERE m.user_id = $1
          AND t.slug = $2
          AND m.status = 'active'
          AND t.status = 'active'
        "#,
    )
    .bind(user_id)
    .bind(tenant_slug)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List active memberships when the client did not specify a tenant slug.
pub async fn list_memberships_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<MembershipAuthRow>> {
    let rows = sqlx::query_as::<_, MembershipAuthRow>(
        r#"
        SELECT
            m.id AS membership_id,
            t.id AS tenant_id,
            t.slug::text AS tenant_slug,
            r.key AS role_key
        FROM memberships m
        JOIN tenants t ON t.id = m.tenant_id
        JOIN roles r ON r.id = m.role_id
        WHERE m.user_id = $1
          AND m.status = 'active'
          AND t.status = 'active'
        ORDER BY t.slug
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Permission keys granted to a role within a tenant.
pub async fn permissions_for_role(
    pool: &PgPool,
    tenant_id: Uuid,
    role_key: &str,
) -> Result<Vec<String>> {
    let keys = sqlx::query_scalar::<_, String>(
        r#"
        SELECT rp.permission_key
        FROM role_permissions rp
        JOIN roles r ON r.id = rp.role_id
        WHERE r.tenant_id = $1 AND r.key = $2
        ORDER BY rp.permission_key
        "#,
    )
    .bind(tenant_id)
    .bind(role_key)
    .fetch_all(pool)
    .await?;
    Ok(keys)
}

/// Atomically create tenant (with RBAC presets), owner user, and membership.
pub async fn register_tenant_and_owner(
    pool: &PgPool,
    tenant_name: &str,
    tenant_slug: &str,
    email: &str,
    password_hash: &str,
    display_name: Option<&str>,
) -> Result<(Uuid, Uuid, Uuid)> {
    let mut tx = pool.begin().await?;

    let tenant_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO tenants (name, slug)
        VALUES ($1, $2)
        RETURNING id
        "#,
    )
    .bind(tenant_name)
    .bind(tenant_slug)
    .fetch_one(&mut *tx)
    .await?;

    let mut owner_role_id = None;
    for preset in RolePreset::ALL {
        let role_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO roles (tenant_id, key, name, is_system)
            VALUES ($1, $2, $3, true)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(preset.key())
        .bind(preset.display_name())
        .fetch_one(&mut *tx)
        .await?;

        for permission in preset.permissions() {
            sqlx::query(
                r#"
                INSERT INTO role_permissions (role_id, permission_key)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(role_id)
            .bind(permission.as_str())
            .execute(&mut *tx)
            .await?;
        }

        if preset == RolePreset::Owner {
            owner_role_id = Some(role_id);
        }
    }

    let user_id =
        crate::db::users::insert_user(&mut tx, email, password_hash, display_name).await?;
    let owner_role_id =
        owner_role_id.ok_or_else(|| crate::error::AppError::internal("owner role not seeded"))?;

    let membership_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO memberships (tenant_id, user_id, role_id, status)
        VALUES ($1, $2, $3, 'active')
        RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(owner_role_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((tenant_id, user_id, membership_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbac::Permission;

    #[test]
    fn owner_preset_has_all_permissions() {
        assert_eq!(RolePreset::Owner.permissions().len(), Permission::ALL.len());
    }
}
