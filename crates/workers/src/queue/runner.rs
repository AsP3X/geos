//! Claim tasks from Postgres and dispatch to handlers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use geos_core::db::{self, PgPool, WorkerTask};
use geos_core::meili::MeiliClient;
use geos_core::Result;
use tracing::{error, info, warn};

use crate::backfill::{backfill_pending, run_backfill, BackfillConfig};
use crate::connector::{
    NwsWeatherConnector, UsgsEarthquakeConnector, NWS_HISTORY_DAYS, NWS_SOURCE, USGS_SOURCE,
};
use crate::ingest;

use super::kinds::{INGEST_NWS_BACKFILL, INGEST_NWS_LIVE, INGEST_USGS_BACKFILL, INGEST_USGS_LIVE};
use super::scheduler::{try_enqueue_backfill, Shutdown};

/// Keep backfill tasks short so workers stay available for live polls.
const BACKFILL_TASK_BUDGET: Duration = Duration::from_secs(25);

/// Shared dependencies for task handlers (connectors, clients, etc.).
pub struct WorkerRuntime {
    /// USGS earthquake connector instance.
    pub usgs: UsgsEarthquakeConnector,
    /// NWS weather alerts connector instance.
    pub nws: NwsWeatherConnector,
    /// Meilisearch client for post-upsert indexing.
    pub meili: Option<MeiliClient>,
    /// Historical backfill behavior.
    pub backfill: BackfillConfig,
}

impl WorkerRuntime {
    /// Build runtime with default connector instances and backfill config.
    pub fn new(meili: Option<MeiliClient>) -> Self {
        Self::with_config(meili, BackfillConfig::default())
    }

    /// Build runtime, applying the backfill request delay to the connectors.
    pub fn with_config(meili: Option<MeiliClient>, backfill: BackfillConfig) -> Self {
        Self {
            usgs: UsgsEarthquakeConnector::new()
                .with_request_delay(backfill.request_delay)
                .with_min_magnitude(backfill.usgs_min_magnitude)
                .with_max_concurrency(backfill.usgs_max_concurrency),
            nws: NwsWeatherConnector::new().with_request_delay(backfill.request_delay),
            meili,
            backfill,
        }
    }
}

impl Default for WorkerRuntime {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Poll the queue and execute claimed tasks until shutdown is signaled.
pub async fn run_worker_loop(
    pool: PgPool,
    worker_id: String,
    runtime: Arc<WorkerRuntime>,
    idle_sleep: Duration,
    shutdown: Shutdown,
) {
    info!(%worker_id, "worker loop started");

    loop {
        tokio::select! {
            biased;

            _ = shutdown.wait() => {
                info!(%worker_id, "worker loop shutting down");
                break;
            }
            result = claim_and_run(&pool, &worker_id, runtime.as_ref()) => {
                match result {
                    Ok(true) => continue,
                    Ok(false) => {
                        tokio::time::sleep(idle_sleep).await;
                    }
                    Err(err) => {
                        error!(%worker_id, error = %err, "task claim or execution failed");
                        tokio::time::sleep(idle_sleep).await;
                    }
                }
            }
        }
    }
}

async fn claim_and_run(pool: &PgPool, worker_id: &str, runtime: &WorkerRuntime) -> Result<bool> {
    let Some(task) = db::claim_task(pool, worker_id).await? else {
        return Ok(false);
    };

    info!(
        %worker_id,
        task_id = %task.id,
        task_type = %task.task_type,
        attempt = task.attempts,
        "claimed task"
    );

    match execute_task(pool, runtime, &task).await {
        Ok(()) => {
            db::complete_task(pool, task.id).await?;
            info!(task_id = %task.id, task_type = %task.task_type, "task completed");
        }
        Err(err) => {
            warn!(
                task_id = %task.id,
                task_type = %task.task_type,
                error = %err,
                "task failed"
            );
            db::fail_task(pool, task.id, &err.to_string()).await?;
        }
    }

    Ok(true)
}

async fn execute_task(pool: &PgPool, runtime: &WorkerRuntime, task: &WorkerTask) -> Result<()> {
    match task.task_type.as_str() {
        INGEST_USGS_LIVE => {
            let stats = ingest::ingest_connector_live(
                pool,
                &runtime.usgs,
                task.tenant_id,
                runtime.meili.as_ref(),
            )
            .await
            .map_err(|err| geos_core::AppError::internal(err.to_string()))?;
            info!(?stats, task_id = %task.id, "USGS live ingest finished");
            Ok(())
        }
        INGEST_NWS_LIVE => {
            let stats = ingest::ingest_connector_live(
                pool,
                &runtime.nws,
                task.tenant_id,
                runtime.meili.as_ref(),
            )
            .await
            .map_err(|err| geos_core::AppError::internal(err.to_string()))?;
            info!(?stats, task_id = %task.id, "NWS live ingest finished");
            Ok(())
        }
        INGEST_USGS_BACKFILL => {
            let upserted = run_backfill_during_budget(
                pool,
                &runtime.usgs,
                task.tenant_id,
                runtime.backfill.usgs_start,
                &runtime.backfill,
            )
            .await
            .map_err(|err| geos_core::AppError::internal(err.to_string()))?;
            info!(upserted, task_id = %task.id, "USGS backfill batch finished");
            chain_backfill_if_pending(pool, INGEST_USGS_BACKFILL, USGS_SOURCE, task.tenant_id)
                .await?;
            Ok(())
        }
        INGEST_NWS_BACKFILL => {
            let earliest = Utc::now() - chrono::Duration::days(NWS_HISTORY_DAYS);
            let upserted = run_backfill_during_budget(
                pool,
                &runtime.nws,
                task.tenant_id,
                earliest,
                &runtime.backfill,
            )
            .await
            .map_err(|err| geos_core::AppError::internal(err.to_string()))?;
            info!(upserted, task_id = %task.id, "NWS backfill batch finished");
            chain_backfill_if_pending(pool, INGEST_NWS_BACKFILL, NWS_SOURCE, task.tenant_id)
                .await?;
            Ok(())
        }
        other => Err(geos_core::AppError::internal(format!(
            "unknown task type: {other}"
        ))),
    }
}

/// Run several backfill batches within a time budget so one task claim does not
/// monopolize a worker for hours.
async fn run_backfill_during_budget(
    pool: &PgPool,
    connector: &dyn crate::connector::Connector,
    tenant_id: uuid::Uuid,
    earliest: chrono::DateTime<Utc>,
    config: &BackfillConfig,
) -> std::result::Result<u64, crate::ingest::IngestError> {
    let deadline = Instant::now() + BACKFILL_TASK_BUDGET;
    let source_key = connector.source_key();
    let mut total = 0u64;

    while Instant::now() < deadline
        && backfill_pending(pool, tenant_id, source_key)
            .await
            .map_err(crate::ingest::IngestError::Core)?
    {
        let upserted = run_backfill(pool, connector, tenant_id, earliest, config).await?;
        total += upserted;
        if upserted == 0 {
            break;
        }
    }

    Ok(total)
}

/// Immediately queue the next backfill batch when work remains (avoids waiting
/// for the scheduler tick).
async fn chain_backfill_if_pending(
    pool: &PgPool,
    task_type: &str,
    source_key: &str,
    tenant_id: uuid::Uuid,
) -> Result<()> {
    if backfill_pending(pool, tenant_id, source_key).await? {
        try_enqueue_backfill(pool, task_type, source_key).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::Connector;

    #[test]
    fn worker_runtime_default_has_connectors() {
        let runtime = WorkerRuntime::default();
        assert_eq!(runtime.usgs.source_key(), "usgs");
        assert_eq!(runtime.nws.source_key(), "nws");
    }
}
