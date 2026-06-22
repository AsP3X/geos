//! Known worker task type strings and payload helpers.

use serde_json::{json, Value};

use crate::connector::USGS_SOURCE;

/// Live USGS earthquake feed ingestion.
pub const INGEST_USGS_LIVE: &str = "ingest_usgs_live";

/// Build dedupe key for a source-scoped ingest task.
pub fn ingest_dedupe_key(task_type: &str, source_key: &str) -> String {
    format!("{task_type}:{source_key}")
}

/// Payload for [`INGEST_USGS_LIVE`] tasks.
pub fn ingest_usgs_live_payload() -> Value {
    json!({ "source_key": USGS_SOURCE })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_key_includes_type_and_source() {
        assert_eq!(
            ingest_dedupe_key(INGEST_USGS_LIVE, USGS_SOURCE),
            "ingest_usgs_live:usgs"
        );
    }
}
