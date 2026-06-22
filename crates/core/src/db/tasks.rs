//! Postgres-backed worker task queue (`worker_tasks` table).
//!
//! Workers claim tasks with `FOR UPDATE SKIP LOCKED` so any replica can process
//! any pending row without double execution.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// Lifecycle status of a row in `worker_tasks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerTaskStatus {
    /// Waiting to be claimed.
    Pending,
    /// Claimed and executing.
    Running,
    /// Finished successfully.
    Completed,
    /// Exhausted retries or permanently failed.
    Failed,
}

/// A claimed or fetched worker task row.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerTask {
    /// Primary key.
    pub id: Uuid,
    /// Tenant scope for the work unit.
    pub tenant_id: Uuid,
    /// Handler key (e.g. `ingest_usgs_live`).
    pub task_type: String,
    /// Handler-specific JSON payload.
    pub payload: Value,
    /// Current attempt count (incremented on claim).
    pub attempts: i16,
    /// Maximum attempts before marking failed.
    pub max_attempts: i16,
}

/// Input for enqueueing a new task (or no-op if dedupe key already queued).
#[derive(Debug, Clone)]
pub struct EnqueueTask<'a> {
    /// Tenant that owns the work.
    pub tenant_id: Uuid,
    /// Handler key.
    pub task_type: &'a str,
    /// Handler payload.
    pub payload: Value,
    /// Dedupe key preventing duplicate pending/running tasks.
    pub dedupe_key: &'a str,
    /// When the task becomes eligible for claim.
    pub run_at: DateTime<Utc>,
    /// Higher runs first among ready tasks.
    pub priority: i16,
}

/// Insert a task if no pending/running row exists for `(tenant_id, dedupe_key)`.
///
/// Returns the new task id when inserted, or `None` when deduped.
pub async fn enqueue_task(pool: &PgPool, task: EnqueueTask<'_>) -> Result<Option<Uuid>> {
    let row = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO worker_tasks (
            tenant_id, task_type, payload, dedupe_key, run_at, priority
        ) VALUES ($1, $2, $3::jsonb, $4, $5, $6)
        ON CONFLICT (tenant_id, dedupe_key)
            WHERE status IN ('pending', 'running')
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(task.tenant_id)
    .bind(task.task_type)
    .bind(task.payload)
    .bind(task.dedupe_key)
    .bind(task.run_at)
    .bind(task.priority)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Claim the next ready task for `worker_id`, or `None` if the queue is empty.
pub async fn claim_task(pool: &PgPool, worker_id: &str) -> Result<Option<WorkerTask>> {
    let row = sqlx::query_as::<_, ClaimedTaskRow>(
        r#"
        UPDATE worker_tasks
        SET
            status = 'running',
            locked_at = now(),
            locked_by = $1,
            attempts = attempts + 1,
            updated_at = now()
        WHERE id = (
            SELECT id
            FROM worker_tasks
            WHERE status = 'pending'
              AND run_at <= now()
            ORDER BY priority DESC, run_at ASC
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        RETURNING id, tenant_id, task_type, payload, attempts, max_attempts
        "#,
    )
    .bind(worker_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(ClaimedTaskRow::into_worker_task))
}

/// Mark a task completed after successful handler execution.
pub async fn complete_task(pool: &PgPool, task_id: Uuid) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE worker_tasks
        SET
            status = 'completed',
            finished_at = now(),
            updated_at = now(),
            locked_at = NULL,
            locked_by = NULL
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record failure; re-queue with backoff until `max_attempts`, then mark failed.
pub async fn fail_task(pool: &PgPool, task_id: Uuid, error: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE worker_tasks
        SET
            status = CASE
                WHEN attempts >= max_attempts THEN 'failed'::worker_task_status
                ELSE 'pending'::worker_task_status
            END,
            last_error = $2,
            run_at = CASE
                WHEN attempts >= max_attempts THEN run_at
                ELSE now() + make_interval(secs => (attempts * 30))
            END,
            locked_at = NULL,
            locked_by = NULL,
            updated_at = now(),
            finished_at = CASE
                WHEN attempts >= max_attempts THEN now()
                ELSE NULL
            END
        WHERE id = $1
        "#,
    )
    .bind(task_id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct ClaimedTaskRow {
    id: Uuid,
    tenant_id: Uuid,
    task_type: String,
    payload: Value,
    attempts: i16,
    max_attempts: i16,
}

impl ClaimedTaskRow {
    fn into_worker_task(self) -> WorkerTask {
        WorkerTask {
            id: self.id,
            tenant_id: self.tenant_id,
            task_type: self.task_type,
            payload: self.payload,
            attempts: self.attempts,
            max_attempts: self.max_attempts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_task_status_variants_exist() {
        assert_ne!(WorkerTaskStatus::Pending, WorkerTaskStatus::Running);
        assert_ne!(WorkerTaskStatus::Completed, WorkerTaskStatus::Failed);
    }
}
