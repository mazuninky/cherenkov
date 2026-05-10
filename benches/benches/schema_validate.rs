//! Schema validation microbenchmark.
//!
//! Compares three publish-time validation paths the hub may take:
//! `AllowAllValidator` (no parsing), an unknown-namespace registry hit
//! (pass-through after a hash lookup), and a registered JSON Schema
//! namespace (parse + validate).

use std::sync::Arc;

use bytes::Bytes;
use cherenkov_core::{AllowAllValidator, SchemaValidator};
use cherenkov_schema::JsonSchemaRegistry;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde_json::json;
use tokio::runtime::Runtime;

fn orders_payload() -> Bytes {
    Bytes::from(serde_json::to_vec(&json!({ "sku": "ABC-123", "qty": 4 })).unwrap())
}

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

fn bench_validators(c: &mut Criterion) {
    let runtime = Runtime::new().expect("tokio runtime");
    let payload = orders_payload();

    let allow_all = AllowAllValidator;
    c.bench_function("validator/allow_all", |b| {
        b.to_async(&runtime).iter(|| {
            let payload = payload.clone();
            async move {
                allow_all
                    .validate(black_box("orders.created"), &payload)
                    .await
                    .expect("allow-all");
            }
        });
    });

    let registry = Arc::new(build_registry());
    let registry_ok = Arc::clone(&registry);
    c.bench_function("validator/json_schema/registered_ok", |b| {
        b.to_async(&runtime).iter(|| {
            let payload = payload.clone();
            let registry = Arc::clone(&registry_ok);
            async move {
                registry
                    .validate(black_box("orders.created"), &payload)
                    .await
                    .expect("valid orders payload");
            }
        });
    });

    let opaque_payload = Bytes::from_static(b"opaque-bytes");
    let registry_unk = Arc::clone(&registry);
    c.bench_function("validator/json_schema/unknown_namespace", |b| {
        b.to_async(&runtime).iter(|| {
            let payload = opaque_payload.clone();
            let registry = Arc::clone(&registry_unk);
            async move {
                registry
                    .validate(black_box("rooms.lobby"), &payload)
                    .await
                    .expect("unknown namespace pass-through");
            }
        });
    });
}

criterion_group!(benches, bench_validators);
criterion_main!(benches);
