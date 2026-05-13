//! In-process broker for Cherenkov.
//!
//! [`MemoryBroker`] is the simplest possible [`cherenkov_core::Broker`]
//! implementation: one [`tokio::sync::broadcast`] channel per topic, lazy
//! creation, automatic reaping when no subscribers remain.
//!
//! It is intended for single-node deployments and tests. For cross-node
//! fan-out, swap in `cherenkov-broker-redis` or `cherenkov-broker-nats`
//! once they are implemented.
//!
//! # Back-pressure
//!
//! `tokio::sync::broadcast` is bounded. When a subscriber falls behind by
//! more than the channel capacity, the broker drops messages for that
//! subscriber and increments the `cherenkov_broker_dropped_total` counter.
//! Slow subscribers do not block fast ones, but they do lose messages.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use cherenkov_core::{Broker, BrokerError, BrokerStream};
use cherenkov_protocol::Publication;
use dashmap::DashMap;
use futures::StreamExt as _;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tracing::warn;

/// Default per-topic broadcast capacity.
///
/// `1024` was picked as a benchmark-anchor default in `docs/plan.md` open
/// question Q2; revisit once we have a benchmark.
pub const DEFAULT_TOPIC_CAPACITY: usize = 1024;

/// In-process broker backed by [`tokio::sync::broadcast`] channels.
#[derive(Clone)]
pub struct MemoryBroker {
    inner: Arc<Inner>,
}

struct Inner {
    topics: DashMap<String, broadcast::Sender<Publication>>,
    capacity: usize,
}

impl MemoryBroker {
    /// Construct a broker with the [`DEFAULT_TOPIC_CAPACITY`] per-topic
    /// queue depth.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_TOPIC_CAPACITY)
    }

    /// Construct a broker with a custom per-topic queue depth.
    ///
    /// `capacity` must be at least 1; values smaller than 1 are clamped.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                topics: DashMap::new(),
                capacity: capacity.max(1),
            }),
        }
    }

    /// Number of topics currently allocated.
    #[must_use]
    pub fn topic_count(&self) -> usize {
        self.inner.topics.len()
    }

    fn sender_for(&self, topic: &str) -> broadcast::Sender<Publication> {
        if let Some(s) = self.inner.topics.get(topic) {
            return s.clone();
        }
        // Compete for insertion under DashMap's per-shard write lock.
        self.inner
            .topics
            .entry(topic.to_owned())
            .or_insert_with(|| broadcast::channel(self.inner.capacity).0)
            .clone()
    }
}

impl Default for MemoryBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Broker for MemoryBroker {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn subscribe(&self, topic: &str) -> Result<BrokerStream, BrokerError> {
        let sender = self.sender_for(topic);
        let topic_label = topic.to_owned();
        let stream = BroadcastStream::new(sender.subscribe()).filter_map(move |item| {
            let topic_label = topic_label.clone();
            async move {
                match item {
                    Ok(publication) => Some(publication),
                    Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                        warn!(
                            topic = %topic_label,
                            skipped,
                            "memory broker subscriber lagged",
                        );
                        metrics::counter!(
                            "cherenkov_broker_dropped_total",
                            "topic" => topic_label.clone(),
                        )
                        .increment(skipped);
                        None
                    }
                }
            }
        });
        Ok(Box::pin(stream) as Pin<Box<_>>)
    }

    async fn publish(&self, topic: &str, publication: Publication) -> Result<(), BrokerError> {
        // Avoid creating a topic just to broadcast into the void.
        let Some(sender) = self.inner.topics.get(topic).map(|e| e.clone()) else {
            return Ok(());
        };
        // `send` only fails if there are no active receivers; in that case
        // we reap the topic to keep the map bounded.
        if sender.send(publication).is_err() {
            self.inner
                .topics
                .remove_if(topic, |_, s| s.receiver_count() == 0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use cherenkov_protocol::Publication;
    use futures::StreamExt as _;

    use super::*;

    fn pub_(channel: &str, offset: u64, body: &'static [u8]) -> Publication {
        Publication {
            channel: channel.to_owned(),
            offset,
            data: Bytes::from_static(body),
        }
    }

    #[tokio::test]
    async fn lazy_topic_creation() {
        let broker = MemoryBroker::new();
        assert_eq!(broker.topic_count(), 0);
        let _stream = broker.subscribe("rooms.lobby").await.unwrap();
        assert_eq!(broker.topic_count(), 1);
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_silent() {
        let broker = MemoryBroker::new();
        broker
            .publish("ghosts", pub_("ghosts", 0, b"x"))
            .await
            .unwrap();
        // No subscribers: topic was never created, no failure.
        assert_eq!(broker.topic_count(), 0);
    }

    #[tokio::test]
    async fn publish_delivers_to_active_subscribers() {
        let broker = MemoryBroker::new();
        let mut a = broker.subscribe("rooms.lobby").await.unwrap();
        let mut b = broker.subscribe("rooms.lobby").await.unwrap();
        broker
            .publish("rooms.lobby", pub_("rooms.lobby", 0, b"hi"))
            .await
            .unwrap();
        assert_eq!(a.next().await.unwrap().offset, 0);
        assert_eq!(b.next().await.unwrap().offset, 0);
    }

    #[tokio::test]
    async fn topic_is_reaped_after_last_subscriber_gone() {
        let broker = MemoryBroker::new();
        {
            let _s = broker.subscribe("rooms.lobby").await.unwrap();
            // _s dropped at end of scope -> receiver_count goes to 0.
        }
        broker
            .publish("rooms.lobby", pub_("rooms.lobby", 0, b"x"))
            .await
            .unwrap();
        assert_eq!(broker.topic_count(), 0, "reaping happens on next publish");
    }
}
