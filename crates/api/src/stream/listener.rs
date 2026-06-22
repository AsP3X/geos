//! Bridge Postgres `LISTEN` into the in-process event stream hub.

use geos_core::db::{EventNotifyPayload, EVENT_NOTIFY_CHANNEL};
use sqlx::postgres::PgListener;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Connect to Postgres and forward `NOTIFY` payloads to `publisher`.
pub async fn run_event_listener(
    database_url: &str,
    publisher: broadcast::Sender<EventNotifyPayload>,
) {
    loop {
        match listen_loop(database_url, publisher.clone()).await {
            Ok(()) => info!("postgres event listener exited cleanly"),
            Err(err) => error!(error = %err, "postgres event listener failed; retrying in 5s"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn listen_loop(
    database_url: &str,
    publisher: broadcast::Sender<EventNotifyPayload>,
) -> geos_core::Result<()> {
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen(EVENT_NOTIFY_CHANNEL).await?;
    info!(
        channel = EVENT_NOTIFY_CHANNEL,
        "postgres event listener ready"
    );

    loop {
        let notification = listener.recv().await?;
        let payload = match serde_json::from_str::<EventNotifyPayload>(notification.payload()) {
            Ok(payload) => payload,
            Err(err) => {
                warn!(
                    error = %err,
                    payload = notification.payload(),
                    "ignored malformed event notification"
                );
                continue;
            }
        };

        if publisher.send(payload).is_err() {
            // No active subscribers — normal when no websocket clients are connected.
            continue;
        }
    }
}
