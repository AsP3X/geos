//! SQLx database access: connection pool, migrations, and tenant-scoped persistence.

mod connectors;
mod events;
mod pool;
mod tasks;

pub use connectors::touch_live_run;
pub use events::upsert_event;
pub use pool::{connect_pool, run_migrations, PgPool};
pub use tasks::{
    claim_task, complete_task, enqueue_task, fail_task, EnqueueTask, WorkerTask, WorkerTaskStatus,
};

use crate::AppError;

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(value: sqlx::migrate::MigrateError) -> Self {
        Self::Database(value.to_string())
    }
}
