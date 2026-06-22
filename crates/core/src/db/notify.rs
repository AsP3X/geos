//! Postgres `NOTIFY` payloads for live event streaming.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Result;

/// Channel name for event upsert notifications (`LISTEN` / `NOTIFY`).
pub const EVENT_NOTIFY_CHANNEL: &str = "geos_event_upsert";

/// Action carried on an event notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventNotifyAction {
    /// A canonical event row was inserted or updated.
    Upsert,
}

/// JSON payload emitted on [`EVENT_NOTIFY_CHANNEL`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventNotifyPayload {
    /// Owning tenant for isolation checks downstream.
    pub tenant_id: Uuid,
    /// Canonical event id.
    pub event_id: Uuid,
    /// What changed.
    pub action: EventNotifyAction,
}

/// Emit a tenant-scoped upsert notification after persistence succeeds.
pub async fn notify_event_upsert(pool: &PgPool, tenant_id: Uuid, event_id: Uuid) -> Result<()> {
    let payload = EventNotifyPayload {
        tenant_id,
        event_id,
        action: EventNotifyAction::Upsert,
    };
    let json = serde_json::to_string(&payload)
        .map_err(|err| crate::error::AppError::internal(format!("notify encode: {err}")))?;

    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(EVENT_NOTIFY_CHANNEL)
        .bind(json)
        .execute(pool)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(payload: &EventNotifyPayload) -> Option<EventNotifyPayload> {
        let json = serde_json::to_string(payload).ok()?;
        serde_json::from_str(&json).ok()
    }

    #[test]
    fn payload_round_trips_json() {
        let tenant_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();
        let payload = EventNotifyPayload {
            tenant_id,
            event_id,
            action: EventNotifyAction::Upsert,
        };
        assert_eq!(roundtrip(&payload), Some(payload));
    }
}
