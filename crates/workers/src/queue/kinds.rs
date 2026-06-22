//! Known worker task type strings and payload helpers.

use serde_json::{json, Value};

use crate::connector::{NWS_SOURCE, USGS_SOURCE};

/// Live USGS earthquake feed ingestion.
pub const INGEST_USGS_LIVE: &str = "ingest_usgs_live";

/// Live NWS weather alerts feed ingestion.
pub const INGEST_NWS_LIVE: &str = "ingest_nws_live";

/// Build dedupe key for a source-scoped ingest task.
pub fn ingest_dedupe_key(task_type: &str, source_key: &str) -> String {
    format!("{task_type}:{source_key}")
}

/// Payload for live ingest tasks keyed by source.
pub fn ingest_live_payload(source_key: &str) -> Value {
    json!({ "source_key": source_key })
}

/// Payload for [`INGEST_USGS_LIVE`] tasks.
pub fn ingest_usgs_live_payload() -> Value {
    ingest_live_payload(USGS_SOURCE)
}

/// Payload for [`INGEST_NWS_LIVE`] tasks.
pub fn ingest_nws_live_payload() -> Value {
    ingest_live_payload(NWS_SOURCE)
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
        assert_eq!(
            ingest_dedupe_key(INGEST_NWS_LIVE, NWS_SOURCE),
            "ingest_nws_live:nws"
        );
    }
}
