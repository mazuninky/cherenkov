//! Schema registry and validation for Cherenkov.
//!
//! Provides [`JsonSchemaRegistry`] — a [`SchemaValidator`](cherenkov_core::SchemaValidator)
//! that resolves a per-namespace JSON Schema and validates every publication
//! payload against it before the hub forwards it.
//!
//! # Channel-to-namespace resolution
//!
//! By convention a channel name looks like `<namespace>.<rest>`, so
//! `rooms.lobby` belongs to namespace `rooms`. The registry exposes the
//! split rule via [`namespace_of`] so every component agrees on the same
//! convention.
//!
//! Namespaces without a registered schema are pass-through: the validator
//! returns `Ok(())` and the publication continues to the channel kind.
//! This honours the schema-as-contract principle from `docs/plan.md` §2.3:
//! either a namespace is fully validated or fully opaque, never mixed.
//!
//! # Example
//!
//! ```
//! use bytes::Bytes;
//! use cherenkov_schema::JsonSchemaRegistry;
//! use cherenkov_core::SchemaValidator;
//! use serde_json::json;
//!
//! # tokio_test::block_on(async {
//! let registry = JsonSchemaRegistry::builder()
//!     .with_namespace("orders", json!({
//!         "type": "object",
//!         "required": ["sku"],
//!         "properties": { "sku": { "type": "string" } }
//!     }))
//!     .expect("schema compiles")
//!     .build();
//!
//! // Valid payload passes.
//! let ok = Bytes::from_static(br#"{"sku":"abc"}"#);
//! registry.validate("orders.created", &ok).await.unwrap();
//!
//! // Missing required field is rejected.
//! let bad = Bytes::from_static(br#"{}"#);
//! assert!(registry.validate("orders.created", &bad).await.is_err());
//!
//! // Namespace without a schema is pass-through.
//! let opaque = Bytes::from_static(b"\x00\x01\x02");
//! registry.validate("rooms.lobby", &opaque).await.unwrap();
//! # });
//! ```

mod registry;

pub use registry::{JsonSchemaRegistry, JsonSchemaRegistryBuilder, RegistryError, namespace_of};
