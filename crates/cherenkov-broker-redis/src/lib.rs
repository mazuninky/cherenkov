//! Redis-backed [`cherenkov_core::Broker`] for cross-node fan-out.
//!
//! Topics map 1:1 to Redis Pub/Sub channels. Publications are encoded
//! as protobuf via [`cherenkov_protocol::encode_publication`] and
//! published with `PUBLISH`; subscribers receive them via `SUBSCRIBE`
//! and decode with [`cherenkov_protocol::decode_publication`].
//!
//! Best-effort delivery: Redis Pub/Sub is fire-and-forget. If a
//! subscriber misses a publication (network blip, slow consumer), it
//! is lost. Recovery is the responsibility of the channel kind's
//! history layer, not the broker.

use std::sync::Arc;

use async_trait::async_trait;
use cherenkov_core::{Broker, BrokerError, BrokerStream};
use cherenkov_protocol::{Publication, decode_publication, encode_publication};
use fred::clients::SubscriberClient;
use fred::interfaces::{ClientLike, EventInterface, PubsubInterface};
use fred::prelude::*;
use fred::types::Value as FredValue;
use fred::types::config::Config as FredConfig;
use futures::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, warn};

/// Redis broker configuration handed to [`RedisBroker::connect`].
#[derive(Clone, Debug)]
pub struct RedisBrokerConfig {
    /// `redis://` URL (e.g. `redis://127.0.0.1:6379`).
    pub url: String,
}

impl RedisBrokerConfig {
    /// Construct a config pointing at `url`.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

/// Redis [`Broker`] implementation.
#[derive(Clone)]
pub struct RedisBroker {
    inner: Arc<Inner>,
}

struct Inner {
    publisher: Client,
    subscriber: SubscriberClient,
}

impl RedisBroker {
    /// Connect to the configured Redis instance and return a ready
    /// broker.
    ///
    /// # Errors
    ///
    /// Returns [`BrokerError::Other`] if either the publisher or
    /// subscriber connection fails.
    pub async fn connect(config: RedisBrokerConfig) -> Result<Self, BrokerError> {
        let cfg = FredConfig::from_url(&config.url)
            .map_err(|e| BrokerError::Other(format!("redis url: {e}")))?;
        let publisher = Builder::from_config(cfg.clone())
            .build()
            .map_err(|e| BrokerError::Other(format!("redis publisher build: {e}")))?;
        publisher
            .init()
            .await
            .map_err(|e| BrokerError::Other(format!("redis publisher init: {e}")))?;
        let subscriber = Builder::from_config(cfg)
            .build_subscriber_client()
            .map_err(|e| BrokerError::Other(format!("redis subscriber build: {e}")))?;
        subscriber
            .init()
            .await
            .map_err(|e| BrokerError::Other(format!("redis subscriber init: {e}")))?;
        Ok(Self {
            inner: Arc::new(Inner {
                publisher,
                subscriber,
            }),
        })
    }
}

#[async_trait]
impl Broker for RedisBroker {
    fn name(&self) -> &'static str {
        "redis"
    }

    async fn subscribe(&self, topic: &str) -> Result<BrokerStream, BrokerError> {
        self.inner
            .subscriber
            .subscribe(topic)
            .await
            .map_err(|e| BrokerError::Other(format!("redis SUBSCRIBE: {e}")))?;
        let topic_label = topic.to_owned();
        let receiver = self.inner.subscriber.message_rx();
        let stream = BroadcastStream::new(receiver).filter_map(move |msg| {
            let topic_label = topic_label.clone();
            async move {
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => {
                        warn!(%e, "redis broker message_rx lagged");
                        return None;
                    }
                };
                let channel_str: &str = &msg.channel;
                if channel_str != topic_label.as_str() {
                    return None;
                }
                let bytes = match msg.value {
                    FredValue::Bytes(b) => b,
                    FredValue::String(s) => bytes::Bytes::copy_from_slice(s.as_bytes()),
                    other => {
                        warn!(?other, "redis broker non-binary payload");
                        return None;
                    }
                };
                match decode_publication(&bytes) {
                    Ok(p) => Some(p),
                    Err(err) => {
                        warn!(%err, "redis broker decode failed");
                        None
                    }
                }
            }
        });
        debug!(topic, "redis broker subscribed");
        Ok(Box::pin(stream))
    }

    async fn publish(&self, topic: &str, publication: Publication) -> Result<(), BrokerError> {
        let bytes = encode_publication(&publication);
        let _: i64 = self
            .inner
            .publisher
            .publish(topic, bytes.to_vec())
            .await
            .map_err(|e| BrokerError::Other(format!("redis PUBLISH: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip() {
        let cfg = RedisBrokerConfig::new("redis://127.0.0.1:6379");
        assert_eq!(cfg.url, "redis://127.0.0.1:6379");
    }
}
