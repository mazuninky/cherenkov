//! The [`ChannelKind`] extension trait.
//!
//! A [`ChannelKind`] is responsible for the semantics of a class of channels:
//! plain pub/sub, CRDT-backed documents, presence rooms, and so on. The hub
//! routes subscribe / unsubscribe / publish through the kind, and the kind
//! returns ready-to-broadcast [`Publication`]s.
//!
//! Implementations live in dedicated crates (e.g. `cherenkov-channel-pubsub`,
//! `cherenkov-channel-crdt`) so that the core does not pull in their
//! dependencies.

use async_trait::async_trait;
use bytes::Bytes;
use cherenkov_protocol::Publication;
use thiserror::Error;

/// Cursor describing a channel's position at the moment of a subscribe.
///
/// `epoch` advances when history is invalidated (e.g. server restart);
/// `offset` is monotonically increasing within an epoch and identifies the
/// most recent publication delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ChannelCursor {
    /// Channel epoch. Increments each time the kind invalidates history.
    pub epoch: u64,
    /// Monotonic offset within the current epoch.
    pub offset: u64,
}

/// Errors a [`ChannelKind`] may surface to the [`crate::Hub`].
#[derive(Debug, Error)]
pub enum ChannelError {
    /// The channel name is malformed for this kind (e.g. empty, wrong prefix).
    #[error("invalid channel name: {channel}")]
    InvalidChannel {
        /// The offending channel name.
        channel: String,
    },
    /// The kind rejected the publication (size limit, schema, etc.).
    #[error("publication rejected by channel kind: {reason}")]
    RejectedPublication {
        /// Human-readable reason; safe for inclusion in error responses.
        reason: String,
    },
    /// An implementation-specific failure.
    #[error("channel kind error: {0}")]
    Other(String),
}

/// Extension trait: a class of channels with shared semantics.
///
/// Implementations must be cheap to clone (they are stored as `Arc<dyn _>`
/// inside the hub) and cancel-safe across all `async fn`s.
#[async_trait]
pub trait ChannelKind: Send + Sync + 'static {
    /// Stable identifier for this kind, used in metrics and structured logs
    /// (e.g. `"pubsub"`, `"crdt-yjs"`).
    fn name(&self) -> &'static str;

    /// Called when a session subscribes to `channel`. The kind may use this
    /// to lazily allocate per-channel state.
    ///
    /// Returns the channel cursor at the moment of subscription, which is
    /// echoed back in the protocol-level `SubscribeOk`.
    async fn on_subscribe(&self, channel: &str) -> Result<ChannelCursor, ChannelError>;

    /// Called when a session unsubscribes from `channel`. The kind may use
    /// this hook to reap idle per-channel state once the last subscriber
    /// leaves.
    async fn on_unsubscribe(&self, channel: &str) -> Result<(), ChannelError>;

    /// Called when a session publishes `data` to `channel`. The kind is
    /// responsible for assigning an offset and constructing the resulting
    /// [`Publication`]; the hub then hands it to the [`crate::Broker`] for
    /// fan-out.
    async fn on_publish(&self, channel: &str, data: Bytes) -> Result<Publication, ChannelError>;

    /// Replay any retained publications on `channel` whose offset is
    /// strictly greater than `since_offset`.
    ///
    /// Default implementation returns an empty `Vec` — channel kinds
    /// without a history layer (CRDT documents, ephemeral pub/sub) do
    /// not implement replay. Implementations with bounded history
    /// (e.g. [`PubSubChannel`]) override this to return the retained
    /// entries; if the requested offset has already been evicted, the
    /// implementation returns an empty `Vec` rather than an error so
    /// the caller can fall back to "live only" semantics.
    ///
    /// [`PubSubChannel`]: https://docs.rs/cherenkov-channel-pubsub
    async fn replay_since(
        &self,
        _channel: &str,
        _since_offset: u64,
    ) -> Result<Vec<Publication>, ChannelError> {
        Ok(Vec::new())
    }
}
