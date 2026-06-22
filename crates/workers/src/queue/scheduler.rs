//! Enqueue recurring connector work into `worker_tasks`.

use std::time::Duration;

use chrono::Utc;
use geos_core::db::{self, EnqueueTask, PgPool};
use geos_core::tenancy::SYSTEM_TENANT_ID;
use geos_core::Result;
use tracing::{debug, info};

use super::kinds::{ingest_dedupe_key, ingest_usgs_live_payload, INGEST_USGS_LIVE};

/// Default interval between scheduler enqueue attempts for live polls.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Enqueue the initial USGS live ingest task if none is already pending/running.
pub async fn bootstrap_tasks(pool: &PgPool) -> Result<()> {
    let inserted = enqueue_usgs_live(pool, Utc::now()).await?;
    if inserted.is_some() {
        info!("bootstrapped USGS live ingest task");
    } else {
        debug!("USGS live ingest task already queued");
    }
    Ok(())
}

/// Periodically enqueue connector tasks; workers claim and execute them.
pub async fn run_scheduler_loop(pool: PgPool, interval: Duration, shutdown: Shutdown) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = enqueue_due_connector_tasks(&pool).await {
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

async fn enqueue_due_connector_tasks(pool: &PgPool) -> Result<()> {
    enqueue_usgs_live(pool, Utc::now()).await?;
    Ok(())
}

async fn enqueue_usgs_live(
    pool: &PgPool,
    run_at: chrono::DateTime<Utc>,
) -> Result<Option<uuid::Uuid>> {
    let dedupe_key = ingest_dedupe_key(INGEST_USGS_LIVE, crate::connector::USGS_SOURCE);
    db::enqueue_task(
        pool,
        EnqueueTask {
            tenant_id: SYSTEM_TENANT_ID,
            task_type: INGEST_USGS_LIVE,
            payload: ingest_usgs_live_payload(),
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
