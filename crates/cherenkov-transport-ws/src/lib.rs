//! WebSocket transport for Cherenkov.
//!
//! Mounts an [`axum`] `Router` at a configurable path (`/connect/v1` by
//! default) that upgrades inbound HTTP connections to WebSocket and proxies
//! protobuf frames to a [`cherenkov_core::Hub`].
//!
//! # Frame contract
//!
//! Each WebSocket binary message carries one `ClientFrame` (client → server)
//! or one `ServerFrame` (server → client) encoded as a length-unprefixed
//! Protobuf body. Text messages are not used by the protocol and are
//! ignored. Decode failures send the client a final `Error` frame and then
//! close the socket.

use std::net::SocketAddr;

use async_trait::async_trait;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use cherenkov_core::{Hub, HubError, SessionId, Transport, TransportError};
use cherenkov_protocol::{
    ClientFrame, ErrorCode, ProtocolError, ServerFrame, decode_client, encode_server,
};
use futures::{SinkExt as _, StreamExt as _};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Default mount path for the WebSocket transport.
pub const DEFAULT_PATH: &str = "/connect/v1";
/// Default outbox capacity per session, in frames.
pub const DEFAULT_OUTBOX_CAPACITY: usize = 1024;

/// WebSocket transport.
pub struct WsTransport {
    hub: Hub,
    listen: SocketAddr,
    path: String,
    outbox_capacity: usize,
}

/// Builder for [`WsTransport`].
pub struct WsTransportBuilder {
    listen: SocketAddr,
    path: String,
    outbox_capacity: usize,
}

impl WsTransportBuilder {
    /// Construct a builder bound to `listen` with default path and capacity.
    #[must_use]
    pub fn new(listen: SocketAddr) -> Self {
        Self {
            listen,
            path: DEFAULT_PATH.to_owned(),
            outbox_capacity: DEFAULT_OUTBOX_CAPACITY,
        }
    }

    /// Override the mount path. Must start with `/`.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Override the per-session outbox capacity.
    #[must_use]
    pub fn with_outbox_capacity(mut self, capacity: usize) -> Self {
        self.outbox_capacity = capacity.max(1);
        self
    }

    /// Bind the builder to a hub and finalize.
    #[must_use]
    pub fn build(self, hub: Hub) -> WsTransport {
        WsTransport {
            hub,
            listen: self.listen,
            path: self.path,
            outbox_capacity: self.outbox_capacity,
        }
    }
}

/// Convenience constructor for a [`WsTransportBuilder`].
#[must_use]
pub fn builder(listen: SocketAddr) -> WsTransportBuilder {
    WsTransportBuilder::new(listen)
}

#[derive(Clone)]
struct AppState {
    hub: Hub,
    outbox_capacity: usize,
}

#[async_trait]
impl Transport for WsTransport {
    fn name(&self) -> &'static str {
        "ws"
    }

    async fn serve(self: Box<Self>) -> Result<(), TransportError> {
        let listener = TcpListener::bind(self.listen)
            .await
            .map_err(|e| TransportError::Bind(e.to_string()))?;
        let local = listener
            .local_addr()
            .map_err(|e| TransportError::Bind(e.to_string()))?;
        info!(addr = %local, path = %self.path, "ws transport listening");

        serve_on_listener(listener, self.path, self.hub, self.outbox_capacity).await
    }
}

/// Bind the WebSocket router to an existing [`TcpListener`].
///
/// This is the entry point used by integration tests and the server
/// binary's startup code, where the bound port must be discovered after
/// binding.
pub async fn serve_on_listener(
    listener: TcpListener,
    path: String,
    hub: Hub,
    outbox_capacity: usize,
) -> Result<(), TransportError> {
    let state = AppState {
        hub,
        outbox_capacity: outbox_capacity.max(1),
    };
    let app = Router::new()
        .route(&path, get(ws_handler))
        .with_state(state);
    axum::serve(listener, app)
        .await
        .map_err(|e| TransportError::Other(e.to_string()))
}

async fn ws_handler(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut writer, mut reader) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerFrame>(state.outbox_capacity);
    let session = state.hub.open_session(tx);
    let session_id = session.id();
    let shutdown = session.shutdown_notifier();
    debug!(session_id = %session_id, "ws session opened");

    let mut writer_task = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let bytes = encode_server(&frame);
            if writer.send(Message::Binary(bytes)).await.is_err() {
                break;
            }
        }
        let _ = writer.close().await;
    });

    loop {
        tokio::select! {
            biased;
            _ = &mut writer_task => break,
            _ = shutdown.notified() => {
                debug!(session_id = %session_id, "ws session kicked by hub");
                break;
            }
            msg = reader.next() => {
                let Some(msg) = msg else { break };
                let msg = match msg {
                    Ok(m) => m,
                    Err(err) => {
                        warn!(session_id = %session_id, %err, "ws read error");
                        break;
                    }
                };
                if !dispatch(msg, session_id, &state.hub).await {
                    break;
                }
            }
        }
    }

    writer_task.abort();
    state.hub.close_session(session_id);
    debug!(session_id = %session_id, "ws session closed");
}

/// Returns `false` if the socket should be closed after this message.
async fn dispatch(msg: Message, session_id: SessionId, hub: &Hub) -> bool {
    match msg {
        Message::Binary(bytes) => match decode_client(&bytes) {
            Ok(frame) => {
                handle_frame(frame, session_id, hub).await;
                true
            }
            Err(err) => {
                warn!(session_id = %session_id, %err, "ws decode error");
                deliver(
                    hub,
                    session_id,
                    ServerFrame::Error(ProtocolError {
                        request_id: 0,
                        code: ErrorCode::InvalidFrame.into(),
                        message: format!("invalid frame: {err}"),
                    }),
                )
                .await;
                false
            }
        },
        Message::Text(_) | Message::Ping(_) | Message::Pong(_) => true,
        Message::Close(_) => false,
    }
}

async fn handle_frame(frame: ClientFrame, session_id: SessionId, hub: &Hub) {
    match frame {
        ClientFrame::Subscribe(sub) => {
            match hub
                .handle_subscribe(session_id, sub.request_id, &sub.channel, sub.since_offset)
                .await
            {
                Ok(ok) => deliver(hub, session_id, ServerFrame::SubscribeOk(ok)).await,
                Err(err) => {
                    let code = error_code_for(&err);
                    deliver(
                        hub,
                        session_id,
                        ServerFrame::Error(ProtocolError {
                            request_id: sub.request_id,
                            code: code.into(),
                            message: err.to_string(),
                        }),
                    )
                    .await;
                }
            }
        }
        ClientFrame::Unsubscribe(unsub) => {
            match hub
                .handle_unsubscribe(session_id, unsub.request_id, &unsub.channel)
                .await
            {
                Ok(ok) => deliver(hub, session_id, ServerFrame::UnsubscribeOk(ok)).await,
                Err(err) => {
                    let code = error_code_for(&err);
                    deliver(
                        hub,
                        session_id,
                        ServerFrame::Error(ProtocolError {
                            request_id: unsub.request_id,
                            code: code.into(),
                            message: err.to_string(),
                        }),
                    )
                    .await;
                }
            }
        }
        ClientFrame::Publish(p) => {
            if let Err(err) = hub.handle_publish(session_id, &p.channel, p.data).await {
                let code = error_code_for(&err);
                deliver(
                    hub,
                    session_id,
                    ServerFrame::Error(ProtocolError {
                        request_id: p.request_id,
                        code: code.into(),
                        message: err.to_string(),
                    }),
                )
                .await;
            }
        }
        ClientFrame::Connect(c) => {
            match hub.handle_connect(session_id, c.request_id, &c.token).await {
                Ok(ok) => deliver(hub, session_id, ServerFrame::ConnectOk(ok)).await,
                Err(err) => {
                    let code = error_code_for(&err);
                    deliver(
                        hub,
                        session_id,
                        ServerFrame::Error(ProtocolError {
                            request_id: c.request_id,
                            code: code.into(),
                            message: err.to_string(),
                        }),
                    )
                    .await;
                }
            }
        }
    }
}

/// Map a [`HubError`] to its on-the-wire [`ErrorCode`].
///
/// Schema rejections become [`ErrorCode::ValidationFailed`] so clients can
/// distinguish "the server is broken" from "this payload was malformed";
/// every other variant collapses to [`ErrorCode::Internal`] for now.
fn error_code_for(err: &HubError) -> ErrorCode {
    match err {
        HubError::Schema(_) => ErrorCode::ValidationFailed,
        HubError::Auth(_) => ErrorCode::InvalidToken,
        HubError::Acl(_) => ErrorCode::AclDenied,
        HubError::NotConnected { .. } => ErrorCode::NotConnected,
        _ => ErrorCode::Internal,
    }
}

async fn deliver(hub: &Hub, session_id: SessionId, frame: ServerFrame) {
    let Some(session) = hub.sessions().get(&session_id) else {
        return;
    };
    if session.outbox().send(frame).await.is_err() {
        debug!(session_id = %session_id, "ws outbox closed before delivery");
    }
}
