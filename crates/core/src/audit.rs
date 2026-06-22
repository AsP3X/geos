//! Append-only audit log writes (`audit_log` table).

use std::net::IpAddr;

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// Metadata captured from the HTTP request for traceability.
#[derive(Debug, Clone, Default)]
pub struct RequestMeta {
    /// Client IP when available.
    pub ip_address: Option<IpAddr>,
    /// User-Agent header value.
    pub user_agent: Option<String>,
    /// Correlation id from `x-request-id` or generated.
    pub request_id: Option<String>,
}

/// Input for a single audit row.
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    /// Tenant scope; `None` for pre-tenant or platform actions.
    pub tenant_id: Option<Uuid>,
    /// Acting user; `None` for unauthenticated system actions.
    pub actor_user_id: Option<Uuid>,
    /// Dotted action key (e.g. `auth.login`).
    pub action: &'a str,
    /// Resource type label (e.g. `user`, `tenant`).
    pub resource_type: Option<&'a str>,
    /// Resource identifier string.
    pub resource_id: Option<&'a str>,
    /// Non-secret JSON context.
    pub context: Value,
    /// Request metadata.
    pub request: RequestMeta,
}

/// Insert one audit log row.
pub async fn write(pool: &PgPool, entry: AuditEntry<'_>) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO audit_log (
            tenant_id, actor_user_id, action, resource_type, resource_id,
            context, ip_address, user_agent, request_id
        ) VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7, $8, $9)
        "#,
    )
    .bind(entry.tenant_id)
    .bind(entry.actor_user_id)
    .bind(entry.action)
    .bind(entry.resource_type)
    .bind(entry.resource_id)
    .bind(entry.context)
    .bind(entry.request.ip_address.map(|ip| ip.to_string()))
    .bind(entry.request.user_agent)
    .bind(entry.request.request_id)
    .execute(pool)
    .await?;
    Ok(())
}
