//! The [`SchemaValidator`] extension trait.
//!
//! A [`SchemaValidator`] is consulted by the [`crate::Hub`] before every
//! `Publish` is forwarded to a [`crate::ChannelKind`]. It enforces the
//! "schema-as-contract" principle from `docs/plan.md` §2.3: either a
//! namespace declares a schema and every publication is validated, or the
//! namespace is opaque and the validator's [`SchemaValidator::validate`]
//! implementation is a no-op.
//!
//! Concrete implementations live in dedicated crates (notably
//! `cherenkov-schema`) so the core never pulls in `jsonschema`, `prost-reflect`,
//! or any other validator backend.

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

/// Errors a [`SchemaValidator`] may surface to the [`crate::Hub`].
#[derive(Debug, Error)]
pub enum SchemaError {
    /// The payload failed validation against the namespace's declared schema.
    ///
    /// `reason` is human-readable and safe to forward to the client; it
    /// must never include the rejected payload bytes (see `docs/plan.md`
    /// §8.7).
    #[error("payload rejected by schema for `{channel}`: {reason}")]
    PayloadRejected {
        /// The channel that owned the rejected publication.
        channel: String,
        /// Human-readable reason — payload-free.
        reason: String,
    },
    /// An implementation-specific failure (e.g. validator backend crashed).
    #[error("schema validator error: {0}")]
    Other(String),
}

/// Extension trait: validates publication payloads against per-namespace
/// schemas.
///
/// Implementations must be cheap to clone (they are stored as `Arc<dyn _>`
/// inside the hub) and cancel-safe.
///
/// The default no-op validator [`AllowAllValidator`] is used when the
/// hub is built without an explicit schema validator.
#[async_trait]
pub trait SchemaValidator: Send + Sync + 'static {
    /// Stable identifier for this validator backend, used in metrics and
    /// structured logs (e.g. `"json-schema"`, `"protobuf"`, `"allow-all"`).
    fn name(&self) -> &'static str;

    /// Validate `data` for `channel`.
    ///
    /// Implementations resolve the namespace from the channel name (the
    /// canonical convention is "everything before the first `.`") and
    /// look up the registered schema. Channels in namespaces without a
    /// declared schema must return `Ok(())` — schema-as-contract is
    /// per-namespace, not global.
    async fn validate(&self, channel: &str, data: &Bytes) -> Result<(), SchemaError>;
}

/// Validator that accepts every publication. This is the default when the
/// server is configured without any namespace schemas.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAllValidator;

#[async_trait]
impl SchemaValidator for AllowAllValidator {
    fn name(&self) -> &'static str {
        "allow-all"
    }

    async fn validate(&self, _channel: &str, _data: &Bytes) -> Result<(), SchemaError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allow_all_accepts_every_payload() {
        let v = AllowAllValidator;
        v.validate("any.channel", &Bytes::from_static(b"opaque"))
            .await
            .expect("allow-all never rejects");
        assert_eq!(v.name(), "allow-all");
    }
}
