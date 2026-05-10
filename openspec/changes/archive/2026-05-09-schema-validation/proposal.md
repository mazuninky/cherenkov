## Why

Phase 1 (`bootstrap-foundation`) shipped the M0 workspace and the M1
WebSocket pub/sub demo with schema validation explicitly stubbed to
always-allow. `docs/plan.md` §1 lists "schema-aware everything" as one of
Cherenkov's three headline capabilities; without schema enforcement the
project has no answer to "what stops a client from publishing garbage?".
M2 is the milestone where that capability becomes real.

This change delivers the first slice of M2: per-namespace JSON Schema
validation enforced server-side, before publications reach the broker.
AsyncAPI export and the TypeScript SDK generator (the other two M2
deliverables called out in the README) are deliberately deferred to
follow-up changes — they consume the registry this change introduces, so
landing them on top of an already-shipped registry keeps each PR small
enough to review.

## What Changes

- Define a new `SchemaValidator` extension trait in `cherenkov-core`,
  alongside `ChannelKind`, `Transport`, and `Broker`. The default
  `AllowAllValidator` keeps the M1 echo demo's behavior intact when no
  schema validator is wired in.
- Add `HubBuilder::with_schema_validator` and route every `Publish` frame
  through `validator.validate(channel, &data).await` *before* the channel
  kind sees it, so a rejected publication does not advance the channel
  kind's offset or hit the broker.
- Introduce `HubError::Schema(SchemaError)` and a new wire-protocol
  `ErrorCode::ValidationFailed = 5`. Schema rejections surface to the
  client as an `Error` frame whose `request_id` is the publishing
  frame's, with a payload-free human-readable reason.
- Implement `JsonSchemaRegistry` in `cherenkov-schema`, backed by the
  `jsonschema` crate. Schemas are declared per namespace (the part of a
  channel name before the first `.`), compile eagerly so configuration
  errors surface at startup, and namespaces without a registered schema
  are pass-through.
- Extend `cherenkov-server`'s YAML config with a `namespaces:` map.
  Each namespace declares either an inline `schema` JSON value or a
  `schema_path` to a file on disk; exactly one form is required. The
  server composes a `JsonSchemaRegistry` from the config and hands it to
  the hub builder.
- Map `HubError::Schema` to `ErrorCode::ValidationFailed` in
  `cherenkov-transport-ws`'s publish-error handler so the wire-protocol
  code is always correct regardless of which transport is in use.
- Bump MSRV to **1.86** to accommodate `jsonschema`'s `icu_*` transitive
  dependencies (deliberate, documented bump — same precedent as the M0
  1.83 → 1.85 bump captured in `bootstrap-foundation/tasks.md` notes).
- Allow the `MIT-0` license in `deny.toml` (transitive via
  `borrow-or-share`, strictly more permissive than `MIT`, no
  attribution required).
- Add ADR `0003-schema-as-contract.md` capturing the per-namespace
  granularity decision and the validator-trait placement.
- Update the `examples/echo/config.yaml` to declare an `orders`
  namespace schema alongside the existing opaque `rooms.*` channels;
  document the demo path in `README.md`.
- Add an end-to-end integration test
  (`cherenkov-server/tests/schema_validation.rs`) that exercises valid
  publish, invalid publish (Error frame check), and opaque pass-through
  on the same socket.

## Capabilities

### New Capabilities

- `schema-registry`: `cherenkov-schema` crate with `JsonSchemaRegistry`,
  `JsonSchemaRegistryBuilder`, `RegistryError`, and the `namespace_of`
  helper. Implements `cherenkov_core::SchemaValidator` for JSON Schema
  documents.
- `hub-core-validation`: `SchemaValidator` trait, `SchemaError`,
  `AllowAllValidator`, `HubBuilder::with_schema_validator`, and the
  publish-time validation hook in `Hub::handle_publish`.
- `server-namespaces`: `NamespacesConfig` / `NamespaceConfig` /
  `SchemaKind` types in `cherenkov-server`, the YAML schema for them,
  and the `app::run_with_listener` composition path that builds a
  `JsonSchemaRegistry` from config.

### Modified Capabilities

- `wire-protocol`: adds `ErrorCode::ValidationFailed = 5`. Wire format
  is unchanged (the `code` field is `uint32`); the new variant is
  appended per the §2.5 stability rule.
- `ws-transport`: maps `HubError::Schema` to
  `ErrorCode::ValidationFailed` on publish error. No protocol surface
  change.

## Impact

- **Code**: new module `cherenkov-core/src/schema.rs`; full
  implementation of `cherenkov-schema` (was a stub); new
  `NamespacesConfig` plumbing in `cherenkov-server`; one new integration
  test; one new ADR.
- **APIs**: `cherenkov-core` exports `SchemaValidator`, `SchemaError`,
  `AllowAllValidator`. `HubBuilder` gains `with_schema_validator`.
  `HubError` gains a `Schema` variant. None of the existing APIs change
  shape.
- **Wire protocol**: `ErrorCode::ValidationFailed` appended. No
  breaking change inside v1.
- **Dependencies**: adds `jsonschema = "0.28"` (with `default-features =
  false`) and `serde_json = "1"` to workspace deps; transitively pulls
  the `icu_*` family which raises MSRV to 1.86.
- **CI/release**: gates remain non-negotiable. `cargo deny` allowlist
  gains `MIT-0`. Test count rises from 30 → 43.
- **Out of scope (deferred to follow-up changes)**: AsyncAPI export from
  the registry, TypeScript SDK generator, Protobuf descriptor sets as a
  schema language, per-channel schema overrides, schema versioning,
  hot-reload of namespace declarations, multi-kind hub routing
  (`pubsub:*` vs `crdt:*` resolution by namespace).
