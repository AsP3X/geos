//! Normalize connector raw records into canonical [`Event`] values.

mod nws;
mod usgs;

pub use nws::normalize_nws_record;
pub use usgs::normalize_usgs_record;

use geos_core::events::Event;

use crate::connector::{ConnectorError, RawRecord, NWS_SOURCE, USGS_SOURCE};

/// Normalize a [`RawRecord`] from the given source into a canonical [`Event`].
///
/// Unknown sources return an error; add arms as new connectors land.
pub fn normalize_record(
    record: &RawRecord,
    tenant_id: uuid::Uuid,
) -> Result<Event, ConnectorError> {
    match record.source.as_str() {
        USGS_SOURCE => normalize_usgs_record(record, tenant_id),
        NWS_SOURCE => normalize_nws_record(record, tenant_id),
        other => Err(ConnectorError::InvalidRecord(format!(
            "unsupported source for normalization: {other}"
        ))),
    }
}
