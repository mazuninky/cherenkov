//! Server-Sent Events transport for Cherenkov.
//!
//! HTTP/SSE carrier:
//!
//! * `GET /sse/v1/subscribe?channel=ROOM[&channel=...]` opens a long-lived
//!   `text/event-stream` response that emits one event per publication
//!   on the requested channels.
//! * `POST /sse/v1/publish?channel=ROOM` with a binary body publishes to
//!   `channel`. The body is the opaque payload.
//!
//! Both endpoints accept an optional `Authorization: Bearer <token>`
//! header that maps to a `Connect` frame; absent header means anonymous.
//!
//! # Encoding
//!
//! Publications are sent as SSE `data:` lines containing the
//! base64-encoded payload, prefixed with the channel name and offset.
//! Each event has the canonical `event: publication` type.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{Query, RawQuery, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use bytes::Bytes;
use cherenkov_core::{Hub, HubError, Transport, TransportError};
use cherenkov_protocol::ServerFrame;
use futures::stream::Stream;
use futures::StreamExt as _;
use serde::Deserialize;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, warn};

/// Default mount path prefix.
pub const DEFAULT_PATH: &str = "/sse/v1";

/// SSE transport.
pub struct SseTransport {
    hub: Hub,
    listen: SocketAddr,
    path_prefix: String,
}

/// Builder for [`SseTransport`].
pub struct SseTransportBuilder {
    listen: SocketAddr,
    path_prefix: String,
}

/// Errors specific to the SSE transport.
#[derive(Debug, Error)]
pub enum SseError {
    /// The query string was malformed.
    #[error("malformed query: {0}")]
    BadQuery(String),
}

impl SseTransportBuilder {
    /// Construct a builder bound to `listen`.
    #[must_use]
    pub fn new(listen: SocketAddr) -> Self {
        Self {
            listen,
            path_prefix: DEFAULT_PATH.to_owned(),
        }
    }

    /// Override the mount-path prefix. Must start with `/` and not end
    /// with `/`.
    #[must_use]
    pub fn with_path_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefix = prefix.into();
        self
    }

    /// Bind the builder to a hub and finalize.
    #[must_use]
    pub fn build(self, hub: Hub) -> SseTransport {
        SseTransport {
            hub,
            listen: self.listen,
            path_prefix: self.path_prefix,
        }
    }
}

#[derive(Clone)]
struct AppState {
    hub: Hub,
}

#[async_trait]
impl Transport for SseTransport {
    fn name(&self) -> &'static str {
        "sse"
    }

    async fn serve(self: Box<Self>) -> Result<(), TransportError> {
        let listener = TcpListener::bind(self.listen)
            .await
            .map_err(|e| TransportError::Bind(e.to_string()))?;
        info!(addr = %self.listen, prefix = %self.path_prefix, "sse transport listening");
        serve_on_listener(listener, self.path_prefix, self.hub).await
    }
}

/// Bind the SSE router to an existing [`TcpListener`].
pub async fn serve_on_listener(
    listener: TcpListener,
    path_prefix: String,
    hub: Hub,
) -> Result<(), TransportError> {
    let state = AppState { hub };
    let router = Router::new()
        .route(&format!("{path_prefix}/subscribe"), get(subscribe_handler))
        .route(&format!("{path_prefix}/publish"), post(publish_handler))
        .with_state(state);
    axum::serve(listener, router)
        .await
        .map_err(|e| TransportError::Other(e.to_string()))
}

#[cfg(test)]
mod query_tests {
    use super::*;

    #[test]
    fn parses_single_channel() {
        let v = channels_from_raw_query(Some("channel=rooms.lobby"));
        assert_eq!(v, vec!["rooms.lobby".to_owned()]);
    }

    #[test]
    fn parses_repeated_channel_keys() {
        let v = channels_from_raw_query(Some("channel=rooms.a&channel=rooms.b"));
        assert_eq!(v, vec!["rooms.a".to_owned(), "rooms.b".to_owned()]);
    }

    #[test]
    fn ignores_unrelated_keys() {
        let v = channels_from_raw_query(Some("foo=bar&channel=x&baz=qux"));
        assert_eq!(v, vec!["x".to_owned()]);
    }

    #[test]
    fn empty_query_returns_empty() {
        assert!(channels_from_raw_query(None).is_empty());
        assert!(channels_from_raw_query(Some("")).is_empty());
    }

    #[test]
    fn percent_decoding_works() {
        let v = channels_from_raw_query(Some("channel=rooms%2Elobby"));
        assert_eq!(v, vec!["rooms.lobby".to_owned()]);
    }

    #[test]
    fn token_from_headers_strips_bearer_prefix() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Bearer abc-123".parse().unwrap());
        assert_eq!(token_from_headers(&h).as_deref(), Some("abc-123"));
    }

    #[test]
    fn token_from_headers_returns_none_for_other_schemes() {
        let mut h = HeaderMap::new();
        h.insert(header::AUTHORIZATION, "Basic dXNlcjpwYXNz".parse().unwrap());
        assert!(token_from_headers(&h).is_none());
    }
}

/// Parse repeated `channel` keys out of a raw URL query string.
///
/// Axum's default `Query<T>` extractor uses `serde_urlencoded`, which
/// only accepts the *last* value for a given key. SSE subscribers
/// commonly want to subscribe to several channels in one request, so
/// we walk the raw string ourselves.
fn channels_from_raw_query(query: Option<&str>) -> Vec<String> {
    let Some(q) = query else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for pair in q.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if k != "channel" {
            continue;
        }
        if let Ok(decoded) = urlencoding_decode(v) {
            if !decoded.is_empty() {
                out.push(decoded);
            }
        }
    }
    out
}

fn urlencoding_decode(s: &str) -> Result<String, std::str::Utf8Error> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = std::str::from_utf8(&bytes[i + 1..i + 3])?;
                if let Ok(byte) = u8::from_str_radix(h, 16) {
                    out.push(byte);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|e| e.utf8_error())
}

#[derive(Debug, Deserialize)]
struct PublishQuery {
    channel: String,
    #[serde(default)]
    request_id: u64,
}

fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_owned())
}

async fn subscribe_handler(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Response> {
    let channels = channels_from_raw_query(query.as_deref());
    if channels.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "missing `channel` query parameter").into_response());
    }

    let (tx, rx) = mpsc::channel::<ServerFrame>(1024);
    let session = state.hub.open_session(tx);
    let session_id = session.id();

    if let Some(token) = token_from_headers(&headers) {
        if let Err(err) = state.hub.handle_connect(session_id, 0, &token).await {
            state.hub.close_session(session_id);
            return Err(into_response(&err));
        }
    } else if state.hub.requires_connect() {
        state.hub.close_session(session_id);
        return Err((StatusCode::UNAUTHORIZED, "Authorization header required").into_response());
    }

    for (i, channel) in channels.iter().enumerate() {
        if let Err(err) = state
            .hub
            .handle_subscribe(session_id, i as u64, channel, 0)
            .await
        {
            state.hub.close_session(session_id);
            return Err(into_response(&err));
        }
    }

    let shutdown = session.shutdown_notifier();
    let hub = state.hub.clone();
    let live = ReceiverStream::new(rx).map(move |frame| Ok(frame_to_event(&frame)));
    let stream = live
        .take_until(async move { shutdown.notified().await })
        .chain(futures::stream::once(async move {
            hub.close_session(session_id);
            Ok(Event::default().event("close"))
        }));

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn frame_to_event(frame: &ServerFrame) -> Event {
    match frame {
        ServerFrame::Publication(p) => {
            let payload = format!(
                "{{\"channel\":{},\"offset\":{},\"data\":{}}}",
                serde_json::Value::String(p.channel.clone()),
                p.offset,
                serde_json::Value::String(BASE64.encode(&p.data)),
            );
            Event::default().event("publication").data(payload)
        }
        ServerFrame::Error(e) => {
            let payload = format!(
                "{{\"request_id\":{},\"code\":{},\"message\":{}}}",
                e.request_id,
                e.code,
                serde_json::Value::String(e.message.clone()),
            );
            Event::default().event("error").data(payload)
        }
        ServerFrame::SubscribeOk(ok) => Event::default().event("subscribe-ok").data(format!(
            "{{\"request_id\":{},\"channel\":{}}}",
            ok.request_id,
            serde_json::Value::String(ok.channel.clone())
        )),
        ServerFrame::UnsubscribeOk(ok) => Event::default().event("unsubscribe-ok").data(format!(
            "{{\"request_id\":{},\"channel\":{}}}",
            ok.request_id,
            serde_json::Value::String(ok.channel.clone())
        )),
        ServerFrame::ConnectOk(ok) => Event::default().event("connect-ok").data(format!(
            "{{\"request_id\":{},\"subject\":{}}}",
            ok.request_id,
            serde_json::Value::String(ok.subject.clone()),
        )),
    }
}

async fn publish_handler(
    State(state): State<AppState>,
    Query(q): Query<PublishQuery>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let bytes = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            warn!(%e, "sse publish body read failed");
            return (StatusCode::BAD_REQUEST, format!("body: {e}")).into_response();
        }
    };

    let (tx, _rx) = mpsc::channel::<ServerFrame>(8);
    let session = state.hub.open_session(tx);
    let session_id = session.id();

    if let Some(token) = token_from_headers(&headers) {
        if let Err(err) = state.hub.handle_connect(session_id, 0, &token).await {
            state.hub.close_session(session_id);
            return into_response(&err);
        }
    } else if state.hub.requires_connect() {
        state.hub.close_session(session_id);
        return (StatusCode::UNAUTHORIZED, "Authorization header required").into_response();
    }

    let result = state
        .hub
        .handle_publish(session_id, &q.channel, Bytes::from(bytes.to_vec()))
        .await;
    state.hub.close_session(session_id);
    match result {
        Ok(p) => {
            let body = format!(
                "{{\"channel\":{},\"offset\":{}}}\n",
                serde_json::Value::String(p.channel),
                p.offset,
            );
            (StatusCode::OK, body).into_response()
        }
        Err(err) => {
            debug!(channel = %q.channel, request_id = q.request_id, %err, "sse publish error");
            into_response(&err)
        }
    }
}

fn into_response(err: &HubError) -> Response {
    let status = match err {
        HubError::Auth(_) => StatusCode::UNAUTHORIZED,
        HubError::Acl(_) | HubError::NotConnected { .. } => StatusCode::FORBIDDEN,
        HubError::Schema(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, err.to_string()).into_response()
}

/// Keep this for downstream binaries that want a small no-arg builder
/// when they already have a hub on hand.
#[must_use]
pub fn builder(listen: SocketAddr) -> SseTransportBuilder {
    SseTransportBuilder::new(listen)
}

// Workaround: silence unused-import for `Arc` in case the file evolves
// to need it; futures-stream `chain` already needs it indirectly.
#[allow(dead_code)]
fn _arc_anchor() -> Arc<()> {
    Arc::new(())
}
