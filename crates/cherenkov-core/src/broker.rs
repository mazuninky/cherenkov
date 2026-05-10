//! The [`Broker`] extension trait.
//!
//! A broker propagates [`Publication`]s between hub instances. In a
//! single-node deployment the [`MemoryBroker`] from `cherenkov-broker`
//! delivers via local channels; in a clustered deployment, a Redis or NATS
//! broker fans out across nodes.
//!
//! [`MemoryBroker`]: https://docs.rs/cherenkov-broker/latest/cherenkov_broker/struct.MemoryBroker.html

use std::pin::Pin;

use async_trait::async_trait;
use cherenkov_protocol::Publication;
use futures::Stream;
use thiserror::Error;

/// Stream of [`Publication`]s yielded by a [`Broker`] subscription.
pub type BrokerStream = Pin<Box<dyn Stream<Item = Publication> + Send>>;

/// Errors a [`Broker`] may surface.
#[derive(Debug, Error)]
pub enum BrokerError {
    /// The broker rejected the publish because the topic is unknown or the
    /// payload exceeds an implementation-specific limit.
    #[error("broker rejected publish on `{topic}`: {reason}")]
    PublishRejected {
        /// The topic the publish was directed at.
        topic: String,
        /// Human-readable rejection reason.
        reason: String,
    },
    /// The broker's underlying transport (Redis, NATS, etc.) is unavailable.
    #[error("broker backend unavailable: {0}")]
    Unavailable(String),
    /// An implementation-specific failure.
    #[error("broker error: {0}")]
    Other(String),
}

/// Extension trait: a propagation layer for [`Publication`]s.
///
/// Implementations must be cheap to clone (stored as `Arc<dyn _>`) and
/// cancel-safe across all `async fn`s.
#[async_trait]
pub trait Broker: Send + Sync + 'static {
    /// Stable identifier for this broker (e.g. `"memory"`, `"redis"`,
    /// `"nats"`).
    fn name(&self) -> &'static str;

    /// Subscribe to all future [`Publication`]s on `topic`.
    ///
    /// The returned stream yields publications in the order the broker
    /// observes them. Slow consumers may experience back pressure; the
    /// concrete broker chooses whether to block, drop, or surface a
    /// `Lagged`-style error.
    async fn subscribe(&self, topic: &str) -> Result<BrokerStream, BrokerError>;

    /// Publish `publication` to `topic`. Delivery to subscribers may be
    /// asynchronous; the call returns once the broker has accepted the
    /// publication.
    async fn publish(&self, topic: &str, publication: Publication) -> Result<(), BrokerError>;
}
