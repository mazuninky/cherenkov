//! The [`Transport`] extension trait.
//!
//! A transport accepts inbound connections, decodes wire frames into
//! [`cherenkov_protocol::ClientFrame`]s, dispatches them to a [`crate::Hub`],
//! and encodes outbound [`cherenkov_protocol::ServerFrame`]s back to the peer.
//!
//! Implementations live in dedicated crates: `cherenkov-transport-ws`,
//! `cherenkov-transport-wt`, `cherenkov-transport-sse`. The core never
//! imports a concrete transport.

use async_trait::async_trait;
use thiserror::Error;

/// Errors a [`Transport`] may surface during startup or while running.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Failed to bind the configured address.
    #[error("transport failed to bind: {0}")]
    Bind(String),
    /// An implementation-specific failure.
    #[error("transport error: {0}")]
    Other(String),
}

/// Extension trait: a network transport that proxies frames to a hub.
///
/// Implementations are typically constructed via a builder, given a
/// reference to a [`crate::Hub`], and then driven via [`Transport::serve`].
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Stable identifier for this transport, used in metrics and structured
    /// logs (e.g. `"ws"`, `"wt"`, `"sse"`).
    fn name(&self) -> &'static str;

    /// Drive the transport's accept loop until shutdown.
    ///
    /// The future resolves when the transport stops accepting new
    /// connections — either because of a shutdown signal or because of an
    /// unrecoverable error. The transport is responsible for cancel-safe
    /// disconnect cleanup of in-flight connections (see
    /// [`docs/plan.md`](https://github.com/mazuninky/cherenkov/blob/main/docs/plan.md)
    /// §8.9).
    async fn serve(self: Box<Self>) -> Result<(), TransportError>;
}
