//! Enqueue recurring connector work into `worker_tasks`.

use std::time::Duration;

use chrono::Utc;
use geos_core::db::{self, EnqueueTask, PgPool};
use geos_core::tenancy::SYSTEM_TENANT_ID;
use geos_core::Result;
use tracing::{debug, info};

use super::kinds::{
    ingest_dedupe_key, ingest_live_payload, source_payload, INGEST_NWS_BACKFILL, INGEST_NWS_LIVE,
    INGEST_USGS_BACKFILL, INGEST_USGS_LIVE,
};
use crate::backfill::backfill_pending;
use crate::connector::{NWS_SOURCE, USGS_SOURCE};

/// Default interval between scheduler enqueue attempts for live polls.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Running tasks older than this are considered stuck and reclaimed to `pending`
/// (e.g. a worker died mid-task, or a request hung). Keeps the queue unwedged.
const STALE_TASK_MAX_AGE: Duration = Duration::from_secs(120);

/// Enqueue initial live + backfill ingest tasks if none are already queued.
pub async fn bootstrap_tasks(pool: &PgPool, backfill_enabled: bool) -> Result<()> {
    let usgs = enqueue_usgs_live(pool, Utc::now()).await?;
    if usgs.is_some() {
        info!("bootstrapped USGS live ingest task");
    } else {
        debug!("USGS live ingest task already queued");
    }

    let nws = enqueue_nws_live(pool, Utc::now()).await?;
    if nws.is_some() {
        info!("bootstrapped NWS live ingest task");
    } else {
        debug!("NWS live ingest task already queued");
    }

    if backfill_enabled {
        enqueue_due_backfill_tasks(pool).await?;
    }

    Ok(())
}

/// Periodically enqueue connector tasks; workers claim and execute them.
pub async fn run_scheduler_loop(
    pool: PgPool,
    interval: Duration,
    shutdown: Shutdown,
    backfill_enabled: bool,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = enqueue_due_connector_tasks(&pool, backfill_enabled).await {
                    tracing::error!(error = %err, "scheduler enqueue failed");
                }
            }
            _ = shutdown.wait() => {
                info!("scheduler shutting down");
                break;
            }
        }
    }
}

async fn enqueue_due_connector_tasks(pool: &PgPool, backfill_enabled: bool) -> Result<()> {
    // Reclaim tasks wedged in `running` (dead worker or hung request) so the
    // queue keeps moving without waiting for a worker restart.
    match db::recover_stale_tasks(pool, STALE_TASK_MAX_AGE).await {
        Ok(n) if n > 0 => info!(reclaimed = n, "reclaimed stale running tasks"),
        Ok(_) => {}
        Err(err) => tracing::warn!(error = %err, "stale task recovery failed"),
    }

    enqueue_usgs_live(pool, Utc::now()).await?;
    enqueue_nws_live(pool, Utc::now()).await?;
    if backfill_enabled {
        enqueue_due_backfill_tasks(pool).await?;
    }
    Ok(())
}

/// Enqueue a backfill task when the source still has outstanding history.
pub async fn try_enqueue_backfill(pool: &PgPool, task_type: &str, source_key: &str) -> Result<()> {
    enqueue_backfill(pool, task_type, source_key).await
}

/// Enqueue backfill tasks for sources that still have outstanding history.
async fn enqueue_due_backfill_tasks(pool: &PgPool) -> Result<()> {
    enqueue_backfill(pool, INGEST_USGS_BACKFILL, USGS_SOURCE).await?;
    enqueue_backfill(pool, INGEST_NWS_BACKFILL, NWS_SOURCE).await?;
    Ok(())
}

async fn enqueue_backfill(pool: &PgPool, task_type: &str, source_key: &str) -> Result<()> {
    // Skip once a source's backfill is complete (avoids endless no-op tasks).
    if !backfill_pending(pool, SYSTEM_TENANT_ID, source_key).await? {
        return Ok(());
    }
    let dedupe_key = ingest_dedupe_key(task_type, source_key);
    db::enqueue_task(
        pool,
        EnqueueTask {
            tenant_id: SYSTEM_TENANT_ID,
            task_type,
            payload: source_payload(source_key),
            dedupe_key: &dedupe_key,
            run_at: Utc::now(),
            // Lower priority than live polls so fresh data is never starved.
            priority: -1,
        },
    )
    .await?;
    Ok(())
}

async fn enqueue_usgs_live(
    pool: &PgPool,
    run_at: chrono::DateTime<Utc>,
) -> Result<Option<uuid::Uuid>> {
    enqueue_live_ingest(pool, INGEST_USGS_LIVE, USGS_SOURCE, run_at).await
}

async fn enqueue_nws_live(
    pool: &PgPool,
    run_at: chrono::DateTime<Utc>,
) -> Result<Option<uuid::Uuid>> {
    enqueue_live_ingest(pool, INGEST_NWS_LIVE, NWS_SOURCE, run_at).await
}

async fn enqueue_live_ingest(
    pool: &PgPool,
    task_type: &str,
    source_key: &str,
    run_at: chrono::DateTime<Utc>,
) -> Result<Option<uuid::Uuid>> {
    let dedupe_key = ingest_dedupe_key(task_type, source_key);
    db::enqueue_task(
        pool,
        EnqueueTask {
            tenant_id: SYSTEM_TENANT_ID,
            task_type,
            payload: ingest_live_payload(source_key),
            dedupe_key: &dedupe_key,
            run_at,
            priority: 0,
        },
    )
    .await
}

/// Cooperative shutdown signal shared by scheduler and worker loops.
#[derive(Clone)]
pub struct Shutdown {
    notify: std::sync::Arc<tokio::sync::Notify>,
}

impl Shutdown {
    /// Create a new shutdown handle.
    pub fn new() -> Self {
        Self {
            notify: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Signal all listeners to stop.
    pub fn trigger(&self) {
        self.notify.notify_waiters();
    }

    /// Wait until [`Self::trigger`] is called.
    pub async fn wait(&self) {
        self.notify.notified().await;
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_poll_interval_is_one_minute() {
        assert_eq!(DEFAULT_POLL_INTERVAL.as_secs(), 60);
    }
}
