//! Postgres connection pool helpers.

use sqlx::postgres::PgPoolOptions;
pub use sqlx::PgPool;

use crate::Result;

/// Open a Postgres connection pool from `DATABASE_URL`.
pub async fn connect_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Apply pending sqlx migrations embedded at compile time from repo `migrations/`.
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}
