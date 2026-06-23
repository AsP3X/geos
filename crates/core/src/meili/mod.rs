//! Meilisearch client for event full-text search (`events` index).

mod events;

pub use events::{
    ensure_events_index, search_events, upsert_event_document, EventSearchHit, SearchFilters,
    SearchResults, SearchSort,
};

use std::sync::Arc;

use meilisearch_sdk::client::Client;

use crate::Result;

/// Shared Meilisearch HTTP client (cheap to clone).
#[derive(Clone)]
pub struct MeiliClient {
    inner: Arc<Client>,
}

impl MeiliClient {
    /// Connect to Meilisearch using base URL and master/API key.
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        let client = Client::new(base_url, Some(api_key))
            .map_err(|err| crate::error::AppError::Config(format!("meili client: {err}")))?;
        Ok(Self {
            inner: Arc::new(client),
        })
    }

    /// Underlying SDK client for index operations.
    pub fn client(&self) -> &Client {
        &self.inner
    }
}
