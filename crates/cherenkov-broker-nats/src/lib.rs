//! NATS-backed [`cherenkov_core::Broker`] for cross-node fan-out.
//!
//! Topics map directly to NATS subjects. Publications are encoded as
//! protobuf via [`cherenkov_protocol::encode_publication`] and dropped
//! into the subject; subscribers receive them via the standard NATS
//! [`async_nats::Subscriber`] stream and decode with
//! [`cherenkov_protocol::decode_publication`].
//!
//! Best-effort delivery: NATS core is fire-and-forget. JetStream-backed
//! durable broker behavior is reserved for a follow-up change.

use async_trait::async_trait;
use cherenkov_core::{Broker, BrokerError, BrokerStream};
use cherenkov_protocol::{decode_publication, encode_publication, Publication};
use futures::StreamExt as _;
use tracing::{debug, warn};

/// NATS broker configuration handed to [`NatsBroker::connect`].
#[derive(Clone, Debug)]
pub struct NatsBrokerConfig {
    /// NATS connection URL (e.g. `nats://127.0.0.1:4222`).
    pub url: String,
}

impl NatsBrokerConfig {
    /// Construct a config pointing at `url`.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

/// NATS [`Broker`] implementation.
#[derive(Clone)]
pub struct NatsBroker {
    client: async_nats::Client,
}

impl NatsBroker {
    /// Connect to the configured NATS server and return a ready broker.
    ///
    /// # Errors
    ///
    /// Returns [`BrokerError::Other`] if the connection fails.
    pub async fn connect(config: NatsBrokerConfig) -> Result<Self, BrokerError> {
        let client = async_nats::connect(&config.url)
            .await
            .map_err(|e| BrokerError::Other(format!("nats connect: {e}")))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Broker for NatsBroker {
    fn name(&self) -> &'static str {
        "nats"
    }

    async fn subscribe(&self, topic: &str) -> Result<BrokerStream, BrokerError> {
        let subscriber = self
            .client
            .subscribe(topic.to_owned())
            .await
            .map_err(|e| BrokerError::Other(format!("nats SUB: {e}")))?;
        let stream = subscriber.filter_map(|msg| async move {
            match decode_publication(&msg.payload) {
                Ok(p) => Some(p),
                Err(err) => {
                    warn!(%err, "nats broker decode failed");
                    None
                }
            }
        });
        debug!(topic, "nats broker subscribed");
        Ok(Box::pin(stream))
    }

    async fn publish(&self, topic: &str, publication: Publication) -> Result<(), BrokerError> {
        let bytes = encode_publication(&publication);
        self.client
            .publish(topic.to_owned(), bytes)
            .await
            .map_err(|e| BrokerError::Other(format!("nats PUB: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip() {
        let cfg = NatsBrokerConfig::new("nats://127.0.0.1:4222");
        assert_eq!(cfg.url, "nats://127.0.0.1:4222");
    }
}
