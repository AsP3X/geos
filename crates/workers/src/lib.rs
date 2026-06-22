//! # geos-workers
//!
//! Ingestion workers: [`connector::Connector`] implementations, normalization
//! into the canonical [`geos_core::events::Event`], enrichment, correlation,
//! and schedulers. Fetch returns raw records; persistence is a separate stage.

pub mod backfill;
pub mod connector;
pub mod ingest;
pub mod normalizer;
pub mod queue;
