//! Claim tasks from Postgres and dispatch to handlers.

use std::sync::Arc;
use std::time::Duration;

use geos_core::db::{self, PgPool, WorkerTask};
use geos_core::meili::MeiliClient;
use geos_core::Result;
use tracing::{error, info, warn};

use crate::connector::{NwsWeatherConnector, UsgsEarthquakeConnector};
use crate::ingest;

use super::kinds::{INGEST_NWS_LIVE, INGEST_USGS_LIVE};
use super::scheduler::Shutdown;

/// Shared dependencies for task handlers (connectors, clients, etc.).
pub struct WorkerRuntime {
    /// USGS earthquake connector instance.
    pub usgs: UsgsEarthquakeConnector,
    /// NWS weather alerts connector instance.
    pub nws: NwsWeatherConnector,
    /// Meilisearch client for post-upsert indexing.
    pub meili: Option<MeiliClient>,
}

impl WorkerRuntime {
    /// Build runtime with default connector instances.
    pub fn new(meili: Option<MeiliClient>) -> Self {
        Self {
            usgs: UsgsEarthquakeConnector::new(),
            nws: NwsWeatherConnector::new(),
            meili,
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
        other => Err(geos_core::AppError::internal(format!(
            "unknown task type: {other}"
        ))),
    }
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
