//! WebSocket live event stream (`/api/v1/stream`).

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use geos_core::db::get_event;
use geos_core::events::Event;
use geos_core::rbac::Permission;
use geos_core::AppError;
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::warn;

use crate::auth::jwt::decode_access_token;
use crate::middleware::auth::AuthContext;
use crate::state::AppState;

/// Optional bearer token for browser clients that cannot set WS headers.
#[derive(Debug, Deserialize)]
pub struct StreamAuthQuery {
    /// JWT access token (alternative to `Authorization: Bearer`).
    pub token: Option<String>,
}

/// Upgrade to a tenant-scoped live event stream.
pub async fn stream(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<StreamAuthQuery>,
    headers: HeaderMap,
) -> Response {
    let token = bearer_token(&headers)
        .or(query.token.as_deref())
        .map(str::to_owned);

    let Some(token) = token else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let auth = match authenticate_stream(&state, &token) {
        Ok(auth) => auth,
        Err(status) => return status.into_response(),
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, auth))
}

fn authenticate_stream(state: &AppState, token: &str) -> Result<AuthContext, StatusCode> {
    let claims = decode_access_token(&state.config.jwt_secret, token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let auth = AuthContext {
        user_id: claims.sub,
        tenant_id: claims.tenant_id,
        membership_id: claims.membership_id,
        role: claims.role,
        permissions: claims.permissions,
    };

    auth.require_permission(Permission::EventsRead)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    Ok(auth)
}

async fn handle_socket(socket: WebSocket, state: AppState, auth: AuthContext) {
    let (mut sender, mut receiver) = socket.split();
    let mut notify_rx = state.stream.subscribe();

    loop {
        tokio::select! {
            payload = notify_rx.recv() => {
                match payload {
                    Ok(payload) if payload.tenant_id == auth.tenant_id => {
                        match load_stream_event(&state, auth.tenant_id, payload.event_id).await {
                            Ok(message) => {
                                if sender.send(Message::Text(message.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(err) => {
                                warn!(
                                    tenant_id = %auth.tenant_id,
                                    event_id = %payload.event_id,
                                    error = %err,
                                    "failed to load event for stream push"
                                );
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

async fn load_stream_event(
    state: &AppState,
    tenant_id: uuid::Uuid,
    event_id: uuid::Uuid,
) -> Result<String, AppError> {
    let event = get_event(&state.pool, tenant_id, event_id)
        .await?
        .ok_or_else(|| AppError::internal("event missing after notification"))?;

    let envelope = StreamEnvelope {
        kind: "event.upsert",
        event,
    };

    serde_json::to_string(&envelope)
        .map_err(|err| AppError::internal(format!("stream encode: {err}")))
}

#[derive(Serialize)]
struct StreamEnvelope {
    kind: &'static str,
    event: Event,
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}
