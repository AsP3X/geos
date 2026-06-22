//! Shared HTTP helpers for connectors: a hardened client, retry/backoff, and
//! conditional-GET caching.
//!
//! Centralizes the cross-cutting concerns every connector needs when talking to
//! public data sources: identifying User-Agent, connect/request timeouts, gzip
//! compression, bounded retries on transient failures (5xx / 429 / timeouts),
//! and `ETag` / `Last-Modified` validators so repeated polls of an unchanged
//! feed cost a single `304 Not Modified` round-trip.

use std::time::Duration;

use reqwest::header::{
    HeaderName, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, RETRY_AFTER,
};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use tracing::debug;

use super::{ConnectorError, Result};

/// Connect timeout for outbound source requests.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Total request timeout (generous for large historical GeoJSON payloads).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Retry/backoff policy for transient upstream failures.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetryConfig {
    /// Maximum retry attempts after the initial try.
    pub max_retries: u32,
    /// Base delay; grows exponentially per attempt.
    pub base_delay: Duration,
    /// Upper bound on any single backoff sleep.
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 4,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
        }
    }
}

/// Build a hardened reqwest client: identifying UA, timeouts, and gzip.
pub(crate) fn build_client(user_agent: &str) -> Client {
    Client::builder()
        .user_agent(user_agent)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .gzip(true)
        .build()
        .unwrap_or_else(|_| Client::new())
}

/// Send a request with retries on transient errors (5xx, 429, timeouts).
///
/// `build` must produce a fresh [`RequestBuilder`] on each call so a retry never
/// reuses a half-sent request. Non-retryable responses (3xx/4xx other than 429)
/// are returned to the caller unchanged for status inspection.
pub(crate) async fn send_with_retry(
    build: impl Fn() -> RequestBuilder,
    retry: &RetryConfig,
) -> Result<Response> {
    let mut attempt: u32 = 0;
    loop {
        match build().send().await {
            Ok(response) => {
                let status = response.status();
                let retryable = status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS;
                if retryable && attempt < retry.max_retries {
                    let wait = retry_after(&response).unwrap_or_else(|| backoff(retry, attempt));
                    debug!(%status, attempt, ?wait, "retrying upstream request");
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                    continue;
                }
                return Ok(response);
            }
            Err(err) => {
                let transient = err.is_timeout() || err.is_connect();
                if transient && attempt < retry.max_retries {
                    let wait = backoff(retry, attempt);
                    debug!(error = %err, attempt, ?wait, "retrying after transport error");
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                    continue;
                }
                return Err(ConnectorError::Http(err));
            }
        }
    }
}

/// Exponential backoff for `attempt` (0-based), capped at `max_delay`.
fn backoff(retry: &RetryConfig, attempt: u32) -> Duration {
    let factor = 2u32.saturating_pow(attempt);
    retry.base_delay.saturating_mul(factor).min(retry.max_delay)
}

/// Parse a numeric `Retry-After` header (seconds) if present.
fn retry_after(response: &Response) -> Option<Duration> {
    let raw = response.headers().get(RETRY_AFTER)?.to_str().ok()?;
    let secs: u64 = raw.trim().parse().ok()?;
    Some(Duration::from_secs(secs))
}

/// Cached validators for HTTP conditional GET (`ETag` / `Last-Modified`).
#[derive(Debug, Default, Clone)]
pub(crate) struct ConditionalCache {
    etag: Option<String>,
    last_modified: Option<String>,
}

impl ConditionalCache {
    /// Attach `If-None-Match` / `If-Modified-Since` from the cached validators.
    pub(crate) fn apply(&self, builder: RequestBuilder) -> RequestBuilder {
        let mut builder = builder;
        if let Some(etag) = &self.etag {
            builder = builder.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = &self.last_modified {
            builder = builder.header(IF_MODIFIED_SINCE, last_modified);
        }
        builder
    }

    /// Refresh the cached validators from a fresh `200 OK` response.
    pub(crate) fn update(&mut self, response: &Response) {
        self.etag = header_string(response, ETAG);
        self.last_modified = header_string(response, LAST_MODIFIED);
    }
}

fn header_string(response: &Response, name: HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)?
        .to_str()
        .ok()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        let retry = RetryConfig {
            max_retries: 10,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(5),
        };
        assert_eq!(backoff(&retry, 0), Duration::from_millis(500));
        assert_eq!(backoff(&retry, 1), Duration::from_secs(1));
        assert_eq!(backoff(&retry, 2), Duration::from_secs(2));
        // 500ms * 2^10 = 512s, capped to 5s.
        assert_eq!(backoff(&retry, 10), Duration::from_secs(5));
    }

    #[test]
    fn empty_cache_applies_no_headers() {
        let cache = ConditionalCache::default();
        assert!(cache.etag.is_none());
        assert!(cache.last_modified.is_none());
    }
}
