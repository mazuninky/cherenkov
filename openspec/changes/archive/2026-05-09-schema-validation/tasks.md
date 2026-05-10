## 1. Hub validation hook (cherenkov-core)

- [x] 1.1 Add `cherenkov-core/src/schema.rs` with the `SchemaValidator`
       trait, `SchemaError` (thiserror), and the no-op `AllowAllValidator`.
- [x] 1.2 Re-export the new symbols from `cherenkov-core/src/lib.rs`.
- [x] 1.3 Extend `HubError` with a `Schema(SchemaError)` variant via
       `#[from]`.
- [x] 1.4 Add `HubBuilder::with_schema_validator`. Default to
       `AllowAllValidator` when no validator is registered.
- [x] 1.5 Call `self.validator.validate(channel, &data).await` in
       `Hub::handle_publish` before the channel kind sees the data.
- [x] 1.6 Add a hub unit test that proves a rejecting validator
       short-circuits both the channel kind (offset unchanged) and the
       broker (no recorded publish).
- [x] 1.7 Run `cargo test -p cherenkov-core` and confirm green.

## 2. JSON Schema registry (cherenkov-schema)

- [x] 2.1 Add `jsonschema = "0.28"` (default-features = false) and
       `serde_json = "1"` to workspace deps; declare `cherenkov-schema`
       deps and dev-deps.
- [x] 2.2 Implement `cherenkov-schema/src/registry.rs` with
       `JsonSchemaRegistry`, `JsonSchemaRegistryBuilder`, `RegistryError`,
       and `namespace_of`.
- [x] 2.3 Compile schemas eagerly in `with_namespace`; emit
       `RegistryError::InvalidSchema` on failure.
- [x] 2.4 Implement `SchemaValidator for JsonSchemaRegistry`: split
       channel by `namespace_of`, fast-path pass-through for unknown
       namespaces, parse with `serde_json::from_slice`, validate with
       `validator.validate(&value)`.
- [x] 2.5 Unit tests: namespace splitting, valid payload, missing field,
       wrong type, unparseable JSON, unknown namespace, empty registry,
       invalid schema reported with namespace name.
- [x] 2.6 Module-level rustdoc with a runnable example via
       `tokio_test::block_on`.

## 3. Wire-protocol error code

- [x] 3.1 Add `ErrorCode::ValidationFailed = 5` to
       `cherenkov-protocol/src/frame.rs`.
- [x] 3.2 In `cherenkov-transport-ws`, map `HubError::Schema` to
       `ErrorCode::ValidationFailed` on publish error; everything else
       still maps to `Internal`.

## 4. Server config and composition

- [x] 4.1 Add `NamespacesConfig`, `NamespaceConfig`, `SchemaKind` types
       to `cherenkov-server/src/config.rs` with `deny_unknown_fields`.
- [x] 4.2 Validate that exactly one of `schema` / `schema_path` is set
       per namespace in `app::build_schema_registry`; return
       `ServerError::Schema` on misconfig.
- [x] 4.3 Wire the registry into `HubBuilder::with_schema_validator` in
       `app::run_with_listener`.
- [x] 4.4 Re-export `NamespaceConfig`, `NamespacesConfig`, `SchemaKind`
       from `cherenkov-server/src/lib.rs` so integration tests can build
       configs programmatically.
- [x] 4.5 Unit tests: default config has `namespaces` empty; YAML loader
       parses an inline schema declaration.

## 5. Integration test

- [x] 5.1 Add `cherenkov-server/tests/schema_validation.rs` that boots
       the server with one declared namespace (`orders`), opens a
       WebSocket client, and asserts:
       - valid payload round-trips as `Publication`
       - invalid payload yields an `Error` with
         `code = ValidationFailed (5)` and the publishing `request_id`
       - opaque namespace (`rooms.lobby`) passes raw bytes through
- [x] 5.2 Run `cargo test -p cherenkov-server --test schema_validation`
       and confirm green.

## 6. Toolchain bump

- [x] 6.1 Bump `rust-toolchain.toml` channel from `1.85` to `1.86`.
- [x] 6.2 Bump `[workspace.package].rust-version` to `1.86`.
- [x] 6.3 Add `MIT-0` to `deny.toml`'s license allowlist (transitive via
       `borrow-or-share`).

## 7. Documentation

- [x] 7.1 Add `docs/adr/0003-schema-as-contract.md` capturing the
       per-namespace decision and validator-trait placement.
- [x] 7.2 Update `docs/adr/README.md` to list ADR 0003.
- [x] 7.3 Update root `README.md`: differentiator #3 reflects M2
       progress; `cherenkov-schema` row is no longer "M2 stub".
- [x] 7.4 Update `examples/echo/config.yaml`: leave `rooms.*` opaque
       (M1 demo behavior preserved), add an `orders` namespace schema
       as a working demo.

## 8. Definition of Done

- [x] 8.1 `cargo fmt --all -- --check` exits 0
- [x] 8.2 `cargo clippy --workspace --all-targets --all-features
       -- -D warnings` exits 0
- [x] 8.3 `cargo test --workspace --all-features` exits 0 (43 tests)
- [x] 8.4 `cargo doc --workspace --no-deps` builds with zero warnings
       under `RUSTDOCFLAGS="-D warnings"`
- [x] 8.5 `cargo deny check` exits 0 (advisories, bans, licenses, sources)
- [ ] 8.6 `cargo audit` exits 0 (deferred to CI — `cargo-audit` not in
       the local environment; `rustsec/audit-check` action runs on PR)
- [ ] 8.7 GitHub Actions CI green on the merge commit (verifies post-push)
- [x] 8.8 Echo demo still boots end-to-end on a clean clone; publishing
       to `rooms.*` is unchanged, publishing malformed payloads to
       `orders.*` returns an `Error` frame with code 5.
- [x] 8.9 Self-review pass.

## Notes

* **MSRV 1.85 → 1.86.** `jsonschema 0.28` transitively requires
  `icu 2.x`, which needs Rust 1.86. Same precedent as the M0 1.83 →
  1.85 bump (`bootstrap-foundation/tasks.md` Notes). Captured in the
  merge-commit body and in `design.md` D8.

* **`MIT-0` allowlist entry.** `borrow-or-share`, pulled by
  `fluent-uri` → `referencing` → `jsonschema`, is licensed `MIT-0`
  (MIT No Attribution). Strictly more permissive than `MIT`; allowed.

* **AsyncAPI / TS SDK deferred.** Both consume the schema registry
  this change introduces, so they will land as follow-up changes
  (`schema-asyncapi-export`, `schema-ts-sdk-generator`). The trait
  surface here is intentionally async so a future remote-registry
  implementation can plug in without rewriting `Hub::handle_publish`.
