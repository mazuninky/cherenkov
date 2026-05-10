//! WebTransport (HTTP/3 over QUIC) transport for Cherenkov.
//!
//! Each [`wtransport::Endpoint`] connection represents one Cherenkov
//! session. The transport accepts a single bidirectional stream per
//! session and frames length-prefixed `ClientFrame` / `ServerFrame`
//! payloads exactly as the WebSocket transport does — the difference
//! is the underlying carrier.
//!
//! # Framing
//!
//! Each frame is preceded by a 4-byte big-endian length, followed by
//! the protobuf body. This is necessary because QUIC streams are byte
//! streams, not message-oriented like a WebSocket binary frame.
//!
//! # TLS
//!
//! WebTransport mandates TLS over QUIC. The transport accepts a
//! [`wtransport::Identity`] (certificate + key) at construction time.
//! For local development, you can generate a self-signed identity via
//! `wtransport::Identity::self_signed`.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::BytesMut;
use cherenkov_core::{Hub, SessionId, Transport, TransportError};
use cherenkov_protocol::{
    decode_client, encode_server, ClientFrame, ErrorCode, ProtocolError, ServerFrame,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use wtransport::endpoint::{endpoint_side, IncomingSession};
use wtransport::{Endpoint, Identity, ServerConfig};

/// Default per-session outbox capacity, in frames.
pub const DEFAULT_OUTBOX_CAPACITY: usize = 1024;

/// Errors that may surface from the WebTransport transport during
/// startup, distinct from generic [`TransportError`].
#[derive(Debug, Error)]
pub enum WtError {
    /// Failed to construct the QUIC endpoint (cert/key invalid, port
    /// already bound, etc.).
    #[error("WebTransport endpoint setup failed: {0}")]
    EndpointSetup(String),
}

/// WebTransport transport.
pub struct WtTransport {
    hub: Hub,
    endpoint: Endpoint<endpoint_side::Server>,
    outbox_capacity: usize,
}

/// Builder for [`WtTransport`].
pub struct WtTransportBuilder {
    listen: SocketAddr,
    identity: Identity,
    outbox_capacity: usize,
}

impl WtTransportBuilder {
    /// Construct a builder bound to `listen` with the supplied TLS identity.
    #[must_use]
    pub fn new(listen: SocketAddr, identity: Identity) -> Self {
        Self {
            listen,
            identity,
            outbox_capacity: DEFAULT_OUTBOX_CAPACITY,
        }
    }

    /// Override the per-session outbox capacity.
    #[must_use]
    pub fn with_outbox_capacity(mut self, capacity: usize) -> Self {
        self.outbox_capacity = capacity.max(1);
        self
    }

    /// Bind the builder to a hub and finalize.
    ///
    /// # Errors
    ///
    /// Returns [`WtError::EndpointSetup`] if the QUIC endpoint cannot be
    /// constructed (port already bound, identity invalid, etc.).
    pub fn build(self, hub: Hub) -> Result<WtTransport, WtError> {
        let cfg = ServerConfig::builder()
            .with_bind_address(self.listen)
            .with_identity(self.identity)
            .build();
        let endpoint = Endpoint::server(cfg).map_err(|e| WtError::EndpointSetup(e.to_string()))?;
        Ok(WtTransport {
            hub,
            endpoint,
            outbox_capacity: self.outbox_capacity,
        })
    }
}

#[async_trait]
impl Transport for WtTransport {
    fn name(&self) -> &'static str {
        "wt"
    }

    async fn serve(self: Box<Self>) -> Result<(), TransportError> {
        let local_addr = self
            .endpoint
            .local_addr()
            .map_err(|e| TransportError::Bind(e.to_string()))?;
        info!(addr = %local_addr, "wt transport listening");
        let endpoint = Arc::new(self.endpoint);
        let hub = self.hub;
        let outbox_capacity = self.outbox_capacity;
        loop {
            let incoming: IncomingSession = endpoint.accept().await;
            let hub = hub.clone();
            tokio::spawn(handle_incoming(incoming, hub, outbox_capacity));
        }
    }
}

async fn handle_incoming(incoming: IncomingSession, hub: Hub, outbox_capacity: usize) {
    let session_request = match incoming.await {
        Ok(req) => req,
        Err(err) => {
            warn!(%err, "wt session handshake failed");
            return;
        }
    };
    let connection = match session_request.accept().await {
        Ok(c) => c,
        Err(err) => {
            warn!(%err, "wt session accept failed");
            return;
        }
    };
    let (mut write, mut read) = match connection.accept_bi().await {
        Ok(streams) => streams,
        Err(err) => {
            warn!(%err, "wt accept_bi failed");
            return;
        }
    };

    let (tx, mut rx) = mpsc::channel::<ServerFrame>(outbox_capacity);
    let session = hub.open_session(tx);
    let session_id = session.id();
    let shutdown = session.shutdown_notifier();
    debug!(session_id = %session_id, "wt session opened");

    let writer_task = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let bytes = encode_server(&frame);
            let len = bytes.len() as u32;
            if write.write_all(&len.to_be_bytes()).await.is_err()
                || write.write_all(&bytes).await.is_err()
            {
                break;
            }
        }
        let _ = write.finish().await;
    });

    let mut buf = BytesMut::with_capacity(4096);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                debug!(session_id = %session_id, "wt session kicked by hub");
                break;
            }
            res = async {
                let mut len_buf = [0u8; 4];
                if read.read_exact(&mut len_buf).await.is_err() {
                    return Err(());
                }
                let len = u32::from_be_bytes(len_buf) as usize;
                if len > 16 * 1024 * 1024 {
                    return Err(());
                }
                buf.resize(len, 0);
                if read.read_exact(&mut buf).await.is_err() {
                    return Err(());
                }
                Ok::<usize, ()>(len)
            } => {
                if res.is_err() {
                    break;
                }
            }
        }
        match decode_client(&buf) {
            Ok(frame) => dispatch(frame, session_id, &hub).await,
            Err(err) => {
                warn!(session_id = %session_id, %err, "wt decode error");
                deliver(
                    &hub,
                    session_id,
                    ServerFrame::Error(ProtocolError {
                        request_id: 0,
                        code: ErrorCode::InvalidFrame.into(),
                        message: format!("invalid frame: {err}"),
                    }),
                )
                .await;
                break;
            }
        }
    }

    writer_task.abort();
    hub.close_session(session_id);
    debug!(session_id = %session_id, "wt session closed");
}

async fn dispatch(frame: ClientFrame, session_id: SessionId, hub: &Hub) {
    match frame {
        ClientFrame::Connect(c) => {
            match hub.handle_connect(session_id, c.request_id, &c.token).await {
                Ok(ok) => deliver(hub, session_id, ServerFrame::ConnectOk(ok)).await,
                Err(err) => deliver_error(hub, session_id, c.request_id, &err).await,
            }
        }
        ClientFrame::Subscribe(s) => {
            match hub
                .handle_subscribe(session_id, s.request_id, &s.channel, s.since_offset)
                .await
            {
                Ok(ok) => deliver(hub, session_id, ServerFrame::SubscribeOk(ok)).await,
                Err(err) => deliver_error(hub, session_id, s.request_id, &err).await,
            }
        }
        ClientFrame::Unsubscribe(u) => {
            match hub
                .handle_unsubscribe(session_id, u.request_id, &u.channel)
                .await
            {
                Ok(ok) => deliver(hub, session_id, ServerFrame::UnsubscribeOk(ok)).await,
                Err(err) => deliver_error(hub, session_id, u.request_id, &err).await,
            }
        }
        ClientFrame::Publish(p) => {
            if let Err(err) = hub.handle_publish(session_id, &p.channel, p.data).await {
                deliver_error(hub, session_id, p.request_id, &err).await;
            }
        }
    }
}

async fn deliver_error(
    hub: &Hub,
    session_id: SessionId,
    request_id: u64,
    err: &cherenkov_core::HubError,
) {
    use cherenkov_core::HubError;
    let code = match err {
        HubError::Schema(_) => ErrorCode::ValidationFailed,
        HubError::Auth(_) => ErrorCode::InvalidToken,
        HubError::Acl(_) => ErrorCode::AclDenied,
        HubError::NotConnected { .. } => ErrorCode::NotConnected,
        _ => ErrorCode::Internal,
    };
    deliver(
        hub,
        session_id,
        ServerFrame::Error(ProtocolError {
            request_id,
            code: code.into(),
            message: err.to_string(),
        }),
    )
    .await;
}

async fn deliver(hub: &Hub, session_id: SessionId, frame: ServerFrame) {
    let Some(session) = hub.sessions().get(&session_id) else {
        return;
    };
    if session.outbox().send(frame).await.is_err() {
        debug!(session_id = %session_id, "wt outbox closed before delivery");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_constructs_with_self_signed_identity() {
        let identity =
            Identity::self_signed(["localhost", "127.0.0.1"]).expect("self-signed identity");
        let listen: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let _builder = WtTransportBuilder::new(listen, identity).with_outbox_capacity(64);
    }
}
