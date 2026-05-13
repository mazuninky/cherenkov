//! Admin HTTP API + tiny embedded UI for Cherenkov.
//!
//! The router exposes read-only inspection endpoints under `/admin/v1`
//! and serves a single-page HTML console at `/admin`. It is intended
//! to be mounted on a separate listen socket from the data plane so
//! operators can firewall it independently.
//!
//! # Endpoints
//!
//! * `GET /admin/v1/health` → `200 OK` with `{ "status": "ok" }`.
//! * `GET /admin/v1/sessions` → list of currently registered sessions
//!   with their session id, claim subject (if connected), and channel
//!   subscription count.
//! * `GET /admin/v1/sessions/:id` → details for one session, including
//!   the list of subscribed channels.
//! * `GET /admin/v1/channels/:channel/subscribers` → list of session
//!   ids currently subscribed to a channel.
//! * `GET /admin` → embedded HTML console that polls the JSON endpoints
//!   above and renders a live table.

use std::net::SocketAddr;

use axum::extract::{Path, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cherenkov_core::{Hub, SessionRegistry};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Serialize;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

/// Default mount path for JSON endpoints.
pub const DEFAULT_API_PREFIX: &str = "/admin/v1";

/// Bundle of resources the admin router exposes. Use [`AdminResources::new`]
/// plus the `with_*` setters to construct it; the fields tend to grow as
/// the admin surface area expands.
#[derive(Clone, Default)]
pub struct AdminResources {
    /// Session registry (always required to render basic state).
    pub sessions: Option<Arc<SessionRegistry>>,
    /// Hub handle (enables the `disconnect` endpoint).
    pub hub: Option<Hub>,
    /// Prometheus handle (enables the `metrics` endpoint).
    pub metrics: Option<PrometheusHandle>,
    /// Optional bearer token. When set, every request must carry an
    /// `Authorization: Bearer <token>` header matching this value.
    pub auth_token: Option<String>,
}

impl AdminResources {
    /// Build resources with `sessions` set; everything else defaults to
    /// `None`.
    #[must_use]
    pub fn new(sessions: Arc<SessionRegistry>) -> Self {
        Self {
            sessions: Some(sessions),
            ..Self::default()
        }
    }

    /// Attach a hub for the `disconnect` endpoint.
    #[must_use]
    pub fn with_hub(mut self, hub: Hub) -> Self {
        self.hub = Some(hub);
        self
    }

    /// Attach a Prometheus handle for the `metrics` endpoint.
    #[must_use]
    pub fn with_metrics(mut self, metrics: PrometheusHandle) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Require an `Authorization: Bearer <token>` header on every admin
    /// request.
    #[must_use]
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }
}

/// Build a router that serves both the JSON API and the embedded HTML
/// console. The console at `/admin` is always public; JSON endpoints
/// under [`DEFAULT_API_PREFIX`] are gated by the optional auth token.
pub fn router(resources: AdminResources) -> Router {
    let sessions = resources
        .sessions
        .clone()
        .unwrap_or_else(|| Arc::new(SessionRegistry::new()));
    let state = AppState {
        sessions,
        hub: resources.hub,
        metrics: resources.metrics,
        auth_token: resources.auth_token,
    };

    let api = Router::new()
        .route(&format!("{DEFAULT_API_PREFIX}/health"), get(health))
        .route(
            &format!("{DEFAULT_API_PREFIX}/sessions"),
            get(list_sessions),
        )
        .route(
            &format!("{DEFAULT_API_PREFIX}/sessions/{{id}}"),
            get(get_session),
        )
        .route(
            &format!("{DEFAULT_API_PREFIX}/sessions/{{id}}/disconnect"),
            post(disconnect_session),
        )
        .route(
            &format!("{DEFAULT_API_PREFIX}/channels/{{channel}}/subscribers"),
            get(channel_subscribers),
        )
        .route(
            &format!("{DEFAULT_API_PREFIX}/metrics"),
            get(metrics_endpoint),
        )
        .route_layer(from_fn_with_state(state.clone(), require_token))
        .with_state(state);

    api.route("/admin", get(console))
}

/// Bind the admin router to `listen` and serve until the listener is
/// dropped.
///
/// # Errors
///
/// Returns the underlying [`std::io::Error`] if binding or serving
/// fails.
pub async fn serve(listen: SocketAddr, resources: AdminResources) -> std::io::Result<()> {
    let listener = TcpListener::bind(listen).await?;
    info!(addr = %listen, "admin listening");
    serve_on_listener(listener, resources).await
}

/// Bind the admin router to an existing listener.
///
/// # Errors
///
/// Surfaces the underlying [`std::io::Error`] from `axum::serve`.
pub async fn serve_on_listener(
    listener: TcpListener,
    resources: AdminResources,
) -> std::io::Result<()> {
    let app = router(resources);
    axum::serve(listener, app).await
}

#[derive(Clone)]
struct AppState {
    sessions: Arc<SessionRegistry>,
    hub: Option<Hub>,
    metrics: Option<PrometheusHandle>,
    auth_token: Option<String>,
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let Some(expected) = state.auth_token.as_deref() else {
        return next.run(request).await;
    };
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match provided {
        Some(t) if t == expected => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            "missing or invalid Authorization: Bearer <token>",
        )
            .into_response(),
    }
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    sessions: usize,
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        sessions: state.sessions.len(),
    })
}

#[derive(Serialize)]
struct SessionSummary {
    id: u64,
    subject: Option<String>,
    channels: usize,
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<SessionSummary>> {
    let mut out: Vec<SessionSummary> = state
        .sessions
        .snapshot()
        .into_iter()
        .map(|session| SessionSummary {
            id: session.id().0,
            subject: session.claims().map(|c| c.subject.clone()),
            channels: session.channels().len(),
        })
        .collect();
    out.sort_by_key(|s| s.id);
    Json(out)
}

#[derive(Serialize)]
struct SessionDetail {
    id: u64,
    subject: Option<String>,
    channels: Vec<String>,
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<SessionDetail>, Response> {
    let session_id = cherenkov_core::SessionId(id);
    let Some(session) = state.sessions.get(&session_id) else {
        return Err((StatusCode::NOT_FOUND, "session not found").into_response());
    };
    Ok(Json(SessionDetail {
        id,
        subject: session.claims().map(|c| c.subject.clone()),
        channels: session.channels(),
    }))
}

#[derive(Serialize)]
struct SubscribersResponse {
    channel: String,
    subscribers: Vec<u64>,
}

async fn channel_subscribers(
    State(state): State<AppState>,
    Path(channel): Path<String>,
) -> Json<SubscribersResponse> {
    let subs = state
        .sessions
        .subscribers_of(&channel)
        .into_iter()
        .map(|sid| sid.0)
        .collect();
    Json(SubscribersResponse {
        channel,
        subscribers: subs,
    })
}

#[derive(Serialize)]
struct DisconnectResponse {
    id: u64,
    removed: bool,
}

async fn disconnect_session(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<DisconnectResponse>, Response> {
    let Some(hub) = state.hub.clone() else {
        return Err((
            StatusCode::NOT_FOUND,
            "disconnect endpoint not enabled (admin built without hub handle)",
        )
            .into_response());
    };
    let removed = hub.kick_session(cherenkov_core::SessionId(id));
    Ok(Json(DisconnectResponse { id, removed }))
}

async fn metrics_endpoint(State(state): State<AppState>) -> Response {
    match state.metrics {
        Some(handle) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4",
            )],
            handle.render(),
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "metrics recorder not installed",
        )
            .into_response(),
    }
}

const CONSOLE_HTML: &str = include_str!("console.html");

async fn console() -> Html<&'static str> {
    Html(CONSOLE_HTML)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::to_bytes;
    use axum::http::Request;
    use http_body_util::BodyExt as _;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("\"status\":\"ok\""));
    }

    #[tokio::test]
    async fn console_route_returns_html() {
        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&bytes).starts_with("<!DOCTYPE html"));
    }

    #[tokio::test]
    async fn list_sessions_returns_registered_sessions() {
        use cherenkov_core::Session;
        use tokio::sync::mpsc;

        let registry = Arc::new(SessionRegistry::new());
        let (tx, _rx) = mpsc::channel(8);
        let s1 = Arc::new(Session::new(registry.next_id(), tx));
        registry.register(s1);
        let (tx2, _rx2) = mpsc::channel(8);
        let s2 = Arc::new(Session::new(registry.next_id(), tx2));
        registry.register(s2);

        let app = router(AdminResources::new(registry));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/sessions")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("\"id\":0"));
        assert!(body.contains("\"id\":1"));
    }

    #[tokio::test]
    async fn metrics_endpoint_503_without_handle() {
        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/metrics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn metrics_endpoint_renders_when_handle_installed() {
        use metrics_exporter_prometheus::PrometheusBuilder;
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();

        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry).with_metrics(handle));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/metrics")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn disconnect_endpoint_404_without_hub() {
        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/v1/sessions/0/disconnect")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn disconnect_endpoint_kicks_session_when_hub_present() {
        use cherenkov_broker::MemoryBroker;
        use cherenkov_channel_pubsub::PubSubChannel;
        use cherenkov_core::HubBuilder;
        use tokio::sync::mpsc;

        let broker = Arc::new(MemoryBroker::new());
        let kind = Arc::new(PubSubChannel::new());
        let built = HubBuilder::new()
            .with_channel_kind(kind)
            .with_broker(broker)
            .build()
            .unwrap();
        let hub = built.hub;
        let (tx, _rx) = mpsc::channel(8);
        let session = hub.open_session(tx);
        let id = session.id().0;

        let app = router(AdminResources::new(hub.sessions().clone()).with_hub(hub.clone()));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/v1/sessions/{id}/disconnect"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("\"removed\":true"), "body: {body}");
        assert!(hub.sessions().get(&cherenkov_core::SessionId(id)).is_none());
    }

    #[tokio::test]
    async fn auth_token_required_when_set() {
        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry).with_auth_token("s3cr3t"));

        // No header → 401
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Wrong token → 401
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/health")
                    .header("Authorization", "Bearer nope")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Correct token → 200
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/health")
                    .header("Authorization", "Bearer s3cr3t")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn console_html_is_public_even_when_auth_set() {
        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry).with_auth_token("s3cr3t"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn channel_subscribers_returns_empty_when_unused() {
        let registry = Arc::new(SessionRegistry::new());
        let app = router(AdminResources::new(registry));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/v1/channels/rooms.lobby/subscribers")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), 1024).await.unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("\"subscribers\":[]"));
    }
}
