//! In-process fan-out of Postgres event notifications.

use geos_core::db::EventNotifyPayload;
use tokio::sync::broadcast;

/// Broadcast hub for [`EventNotifyPayload`] messages from Postgres `LISTEN`.
#[derive(Clone)]
pub struct EventStreamHub {
    tx: broadcast::Sender<EventNotifyPayload>,
}

impl EventStreamHub {
    /// Create a hub with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Subscribe to upsert notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<EventNotifyPayload> {
        self.tx.subscribe()
    }

    /// Publisher handle for the Postgres listener task.
    pub fn publisher(&self) -> broadcast::Sender<EventNotifyPayload> {
        self.tx.clone()
    }
}

impl Default for EventStreamHub {
    fn default() -> Self {
        Self::new(256)
    }
}
