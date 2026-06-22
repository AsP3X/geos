//! Refresh token persistence (hashed at rest).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// Store a hashed refresh token for a user.
pub async fn insert_refresh_token(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<Uuid> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Row returned when a valid refresh token is presented.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefreshTokenRow {
    /// Token row id.
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
}

/// Look up a non-revoked, unexpired refresh token by hash.
pub async fn find_valid_refresh_token(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<RefreshTokenRow>> {
    let row = sqlx::query_as::<_, RefreshTokenRow>(
        r#"
        SELECT id, user_id
        FROM refresh_tokens
        WHERE token_hash = $1
          AND revoked_at IS NULL
          AND expires_at > now()
        "#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Revoke a refresh token after use (rotation).
pub async fn revoke_refresh_token(pool: &PgPool, token_id: Uuid) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE refresh_tokens
        SET revoked_at = now()
        WHERE id = $1
        "#,
    )
    .bind(token_id)
    .execute(pool)
    .await?;
    Ok(())
}
