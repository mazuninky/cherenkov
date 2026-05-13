//! [`JsonSchemaRegistry`] — a [`SchemaValidator`] backed by per-namespace
//! JSON Schema documents.

use std::collections::HashMap;

use async_trait::async_trait;
use bytes::Bytes;
use cherenkov_core::{SchemaError, SchemaValidator};
use jsonschema::{ValidationError, Validator};
use serde_json::Value;
use thiserror::Error;
use tracing::trace;

/// Errors raised while building a [`JsonSchemaRegistry`].
#[derive(Debug, Error)]
pub enum RegistryError {
    /// The supplied JSON Schema document failed to compile.
    #[error("schema for namespace `{namespace}` is invalid: {reason}")]
    InvalidSchema {
        /// The namespace whose schema failed to compile.
        namespace: String,
        /// Compiler error in human-readable form.
        reason: String,
    },
}

/// Resolve the namespace component of a Cherenkov channel name.
///
/// Channels follow the convention `<namespace>.<rest>`. If `channel`
/// contains no `.`, the entire string is treated as the namespace.
#[must_use]
pub fn namespace_of(channel: &str) -> &str {
    channel.split_once('.').map_or(channel, |(ns, _)| ns)
}

/// Builder for [`JsonSchemaRegistry`].
///
/// Each call to [`with_namespace`](Self::with_namespace) compiles its schema
/// eagerly so configuration errors surface at startup, not on the first
/// publication.
#[derive(Default)]
pub struct JsonSchemaRegistryBuilder {
    schemas: HashMap<String, Validator>,
}

impl JsonSchemaRegistryBuilder {
    /// Construct an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `schema` for `namespace`. Compiles the schema eagerly.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::InvalidSchema`] if the document fails to
    /// compile. The compiler error is included verbatim.
    pub fn with_namespace(
        mut self,
        namespace: impl Into<String>,
        schema: Value,
    ) -> Result<Self, RegistryError> {
        let namespace = namespace.into();
        let validator = jsonschema::validator_for(&schema).map_err(|e: ValidationError<'_>| {
            RegistryError::InvalidSchema {
                namespace: namespace.clone(),
                reason: e.to_string(),
            }
        })?;
        self.schemas.insert(namespace, validator);
        Ok(self)
    }

    /// Finalize the builder.
    #[must_use]
    pub fn build(self) -> JsonSchemaRegistry {
        JsonSchemaRegistry {
            schemas: self.schemas,
        }
    }
}

/// [`SchemaValidator`] backed by per-namespace JSON Schema documents.
///
/// Construct via [`JsonSchemaRegistry::builder`].
pub struct JsonSchemaRegistry {
    schemas: HashMap<String, Validator>,
}

impl JsonSchemaRegistry {
    /// Construct a builder.
    #[must_use]
    pub fn builder() -> JsonSchemaRegistryBuilder {
        JsonSchemaRegistryBuilder::new()
    }

    /// Construct a registry with no schemas — every namespace is opaque.
    /// Equivalent in effect to `cherenkov_core::AllowAllValidator`, but
    /// surfaces the same metric label as the JSON Schema variant.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Number of registered namespaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    /// Whether the registry is empty (no namespaces registered).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }
}

#[async_trait]
impl SchemaValidator for JsonSchemaRegistry {
    fn name(&self) -> &'static str {
        "json-schema"
    }

    async fn validate(&self, channel: &str, data: &Bytes) -> Result<(), SchemaError> {
        let namespace = namespace_of(channel);
        let Some(validator) = self.schemas.get(namespace) else {
            trace!(channel, namespace, "no schema registered; pass-through");
            return Ok(());
        };

        let value: Value = serde_json::from_slice(data).map_err(|err| {
            // We deliberately do not include the offending bytes — only
            // the parser error position, which serde_json renders as
            // "expected ... at line X column Y" without payload content.
            SchemaError::PayloadRejected {
                channel: channel.to_owned(),
                reason: format!("payload is not valid JSON: {err}"),
            }
        })?;

        if let Err(err) = validator.validate(&value) {
            return Err(SchemaError::PayloadRejected {
                channel: channel.to_owned(),
                reason: format!("{err}"),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn build_registry() -> JsonSchemaRegistry {
        JsonSchemaRegistry::builder()
            .with_namespace(
                "orders",
                json!({
                    "type": "object",
                    "required": ["sku", "qty"],
                    "properties": {
                        "sku": { "type": "string", "minLength": 1 },
                        "qty": { "type": "integer", "minimum": 1 }
                    }
                }),
            )
            .expect("schema compiles")
            .build()
    }

    #[test]
    fn namespace_of_splits_on_first_dot() {
        assert_eq!(namespace_of("rooms.lobby"), "rooms");
        assert_eq!(namespace_of("rooms.lobby.subroom"), "rooms");
        assert_eq!(namespace_of("standalone"), "standalone");
        assert_eq!(namespace_of(""), "");
    }

    #[tokio::test]
    async fn valid_payload_passes() {
        let r = build_registry();
        let ok = Bytes::from_static(br#"{"sku":"abc","qty":3}"#);
        r.validate("orders.created", &ok)
            .await
            .expect("valid payload accepted");
    }

    #[tokio::test]
    async fn missing_required_field_rejected() {
        let r = build_registry();
        let bad = Bytes::from_static(br#"{"sku":"abc"}"#);
        let err = r
            .validate("orders.created", &bad)
            .await
            .expect_err("missing field rejected");
        assert!(matches!(err, SchemaError::PayloadRejected { .. }));
    }

    #[tokio::test]
    async fn wrong_type_rejected() {
        let r = build_registry();
        let bad = Bytes::from_static(br#"{"sku":"abc","qty":"three"}"#);
        let err = r
            .validate("orders.created", &bad)
            .await
            .expect_err("wrong type rejected");
        match err {
            SchemaError::PayloadRejected { channel, reason } => {
                assert_eq!(channel, "orders.created");
                assert!(!reason.is_empty());
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn unparsable_json_rejected() {
        let r = build_registry();
        let bad = Bytes::from_static(b"\x00not json");
        let err = r
            .validate("orders.created", &bad)
            .await
            .expect_err("non-json payload rejected");
        match err {
            SchemaError::PayloadRejected { reason, .. } => {
                assert!(reason.contains("not valid JSON"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_namespace_is_pass_through() {
        let r = build_registry();
        let opaque = Bytes::from_static(b"\xff\xff\xff");
        r.validate("rooms.lobby", &opaque)
            .await
            .expect("unknown namespace passes through");
    }

    #[tokio::test]
    async fn empty_registry_accepts_anything() {
        let r = JsonSchemaRegistry::empty();
        assert!(r.is_empty());
        r.validate("any.channel", &Bytes::from_static(b"\x00"))
            .await
            .expect("empty registry never rejects");
    }

    #[test]
    fn invalid_schema_is_reported() {
        let result =
            JsonSchemaRegistry::builder().with_namespace("bad", json!({"type": "not-a-real-type"}));
        let err = match result {
            Ok(_) => panic!("invalid schema must be rejected"),
            Err(err) => err,
        };
        match err {
            RegistryError::InvalidSchema { namespace, .. } => assert_eq!(namespace, "bad"),
        }
    }
}
