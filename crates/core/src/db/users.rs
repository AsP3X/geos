//! User account persistence.

use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// Row with credential material for authentication only — never serialize to clients.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserAuthRow {
    /// User primary key.
    pub id: Uuid,
    /// Email address.
    pub email: String,
    /// Argon2 PHC hash; absent for SSO-only accounts.
    pub password_hash: Option<String>,
    /// Account status string from DB.
    pub status: String,
}

/// Look up a user by case-insensitive email.
pub async fn find_user_by_email(pool: &PgPool, email: &str) -> Result<Option<UserAuthRow>> {
    let row = sqlx::query_as::<_, UserAuthRow>(
        r#"
        SELECT id, email::text AS email, password_hash, status::text AS status
        FROM users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Insert a new user with hashed password inside an existing transaction.
pub async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    email: &str,
    password_hash: &str,
    display_name: Option<&str>,
) -> Result<Uuid> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO users (email, password_hash, display_name)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}
