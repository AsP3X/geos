//! Entry point for the Geos ingestion workers.

use std::sync::Arc;
use std::time::Duration;

use geos_core::meili::MeiliClient;
use geos_workers::queue::{
    bootstrap_tasks, run_scheduler_loop, run_worker_loop, Shutdown, WorkerRuntime,
    DEFAULT_POLL_INTERVAL,
};

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tracing::info!(version = geos_core::VERSION, "geos-workers starting");

    let (pool, meili) = match connect_database().await {
        Ok(services) => services,
        Err(reason) => {
            tracing::error!(%reason, "database unavailable; queue workers idle until shutdown");
            wait_for_shutdown().await;
            tracing::info!("geos-workers shutting down");
            return;
        }
    };

    if meili.is_none() {
        tracing::info!("MEILI_URL not configured; skipping search indexing");
    }

    let shutdown = Shutdown::new();
    let runtime = Arc::new(WorkerRuntime::new(meili));
    let poll_interval = worker_poll_interval();
    let concurrency = worker_concurrency();

    if let Err(err) = bootstrap_tasks(&pool).await {
        tracing::error!(error = %err, "failed to bootstrap task queue");
    }

    let scheduler_shutdown = shutdown.clone();
    let scheduler_pool = pool.clone();
    let scheduler_handle = tokio::spawn(async move {
        run_scheduler_loop(scheduler_pool, poll_interval, scheduler_shutdown).await;
    });

    let mut worker_handles = Vec::with_capacity(concurrency);
    for index in 0..concurrency {
        let worker_id = worker_instance_id(index);
        let worker_pool = pool.clone();
        let worker_runtime = Arc::clone(&runtime);
        let worker_shutdown = shutdown.clone();
        worker_handles.push(tokio::spawn(async move {
            run_worker_loop(
                worker_pool,
                worker_id,
                worker_runtime,
                Duration::from_millis(500),
                worker_shutdown,
            )
            .await;
        }));
    }

    tracing::info!(
        concurrency,
        poll_interval_secs = poll_interval.as_secs(),
        "geos-workers ready; task queue active"
    );

    wait_for_shutdown().await;
    shutdown.trigger();

    for handle in worker_handles {
        let _ = handle.await;
    }
    let _ = scheduler_handle.await;

    tracing::info!("geos-workers shutting down");
}

async fn connect_database() -> Result<(geos_core::db::PgPool, Option<MeiliClient>), String> {
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => return Err("DATABASE_URL not set".to_owned()),
    };

    let pool = geos_core::db::connect_pool(&db_url)
        .await
        .map_err(|err| format!("connection failed: {err}"))?;

    geos_core::db::run_migrations(&pool)
        .await
        .map_err(|err| format!("migration failed: {err}"))?;

    let meili = init_meili().await;
    Ok((pool, meili))
}

async fn init_meili() -> Option<MeiliClient> {
    let url = std::env::var("MEILI_URL").ok()?;
    let key = std::env::var("MEILI_MASTER_KEY").ok()?;
    if url.trim().is_empty() || key.trim().is_empty() {
        return None;
    }

    let client = MeiliClient::new(&url, &key).ok()?;
    if let Err(err) = geos_core::meili::ensure_events_index(client.client()).await {
        tracing::warn!(error = %err, "meilisearch index setup failed");
    }
    Some(client)
}

fn worker_concurrency() -> usize {
    std::env::var("WORKER_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(2)
}

fn worker_poll_interval() -> Duration {
    std::env::var("WORKER_POLL_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_POLL_INTERVAL)
}

fn worker_instance_id(index: usize) -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "local".to_owned());
    format!("{host}-{index}-{}", uuid::Uuid::new_v4())
}

async fn wait_for_shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
