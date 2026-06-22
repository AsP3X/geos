//! Postgres task queue: scheduler enqueues work, worker loops claim and run it.

mod kinds;
mod runner;
mod scheduler;

pub use kinds::{
    ingest_dedupe_key, ingest_live_payload, ingest_nws_live_payload, ingest_usgs_live_payload,
    INGEST_NWS_LIVE, INGEST_USGS_LIVE,
};
pub use runner::{run_worker_loop, WorkerRuntime};
pub use scheduler::{bootstrap_tasks, run_scheduler_loop, Shutdown, DEFAULT_POLL_INTERVAL};
