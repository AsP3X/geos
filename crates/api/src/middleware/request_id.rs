//! Per-request correlation id and client metadata.

use std::net::SocketAddr;

use axum::{
    extract::ConnectInfo,
    http::{HeaderMap, Request},
    middleware::Next,
    response::Response,
};
use geos_core::audit::RequestMeta;
use uuid::Uuid;

/// Extension inserted by the request-id middleware.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Correlation id for logs and audit rows.
    pub request_id: String,
    /// Parsed client metadata.
    pub meta: RequestMeta,
}

/// Assign `x-request-id` (or generate) and capture client metadata.
pub async fn assign_request_context(
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let user_agent = user_agent(request.headers());
    let ip_address = connect_info_ip(request.extensions().get::<ConnectInfo<SocketAddr>>());

    request.extensions_mut().insert(RequestContext {
        request_id: request_id.clone(),
        meta: RequestMeta {
            ip_address,
            user_agent,
            request_id: Some(request_id.clone()),
        },
    });

    let mut response = next.run(request).await;
    if let Ok(value) = http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn connect_info_ip(info: Option<&ConnectInfo<SocketAddr>>) -> Option<std::net::IpAddr> {
    info.map(|ConnectInfo(addr)| addr.ip())
}
