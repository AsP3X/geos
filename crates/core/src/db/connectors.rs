//! Connector cursor / last-run tracking (`connectors_state` table).

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// Record a successful live poll for `(tenant_id, source_key)`.
pub async fn touch_live_run(pool: &PgPool, tenant_id: Uuid, source_key: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE connectors_state
        SET last_live_run_at = $3, updated_at = $3
        WHERE tenant_id = $1 AND source_key = $2
        "#,
    )
    .bind(tenant_id)
    .bind(source_key)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}
