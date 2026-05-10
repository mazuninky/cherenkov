## 1. Workspace skeleton (M0.1 – M0.6)

- [x] 1.1 Add root `Cargo.toml` with `[workspace]`, `resolver = "2"`,
       `members = [...]` listing all 17 crates from `docs/plan.md` §11
- [x] 1.2 Declare shared dependency versions under
       `[workspace.dependencies]` per `docs/plan.md` §3.1 – §3.10
- [x] 1.3 Add `rust-toolchain.toml` pinning the channel and
       `components = ["rustfmt", "clippy"]` (MSRV bumped to 1.85 — see Notes)
- [x] 1.4 Add `rustfmt.toml` (Note: `imports_granularity` /
       `group_imports` deferred — nightly-only)
- [x] 1.5 Add `clippy.toml` enforcing the §4 / §8 rules
- [x] 1.6 Add `deny.toml` rejecting GPL/AGPL/SSPL, yanked versions, and
       unmaintained advisories
- [x] 1.7 Add `.cargo/config.toml` with workspace-level network config and
       release profile in `Cargo.toml`

## 2. Crate stubs (M0.2)

- [x] 2.1 Create `crates/cherenkov/src/lib.rs` with crate-level rustdoc
- [x] 2.2 Create `crates/cherenkov-protocol/{Cargo.toml,src/lib.rs,build.rs,proto/v1.proto}`
- [x] 2.3 Create `crates/cherenkov-core/{Cargo.toml,src/lib.rs}`
- [x] 2.4 Create `crates/cherenkov-schema/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.5 Create `crates/cherenkov-transport-ws/{Cargo.toml,src/lib.rs}`
- [x] 2.6 Create `crates/cherenkov-transport-wt/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.7 Create `crates/cherenkov-transport-sse/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.8 Create `crates/cherenkov-channel-pubsub/{Cargo.toml,src/lib.rs}`
- [x] 2.9 Create `crates/cherenkov-channel-crdt/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.10 Create `crates/cherenkov-broker/{Cargo.toml,src/lib.rs}`
- [x] 2.11 Create `crates/cherenkov-broker-redis/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.12 Create `crates/cherenkov-broker-nats/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.13 Create `crates/cherenkov-auth/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.14 Create `crates/cherenkov-admin/{Cargo.toml,src/lib.rs}` (stub)
- [x] 2.15 Create `crates/cherenkov-server/{Cargo.toml,src/main.rs,src/lib.rs}`
- [x] 2.16 Create `crates/cherenkov-sdk-ts/{Cargo.toml,src/main.rs}` (stub)
- [x] 2.17 Create `crates/cherenkov-test-support/{Cargo.toml,src/lib.rs}` with
       `test-support` feature flag
- [x] 2.18 Run `cargo build --workspace` and ensure every stub compiles

## 3. Governance, license, templates (M0.8 – M0.10)

- [x] 3.1 Add `LICENSE` (MIT, current year, owner attribution)
- [x] 3.2 Add `CONTRIBUTING.md` referencing `docs/plan.md` §9
- [x] 3.3 Add `CODE_OF_CONDUCT.md` (links to Contributor Covenant 2.1)
- [x] 3.4 Add `SECURITY.md` with disclosure address and supported versions
- [x] 3.5 Add `.gitignore` covering `target/`, IDE files, and the
       `docs/plan.md` §8.10 secret patterns
- [x] 3.6 Add `.github/ISSUE_TEMPLATE/bug_report.md` and `feature_request.md`
- [x] 3.7 Add `.github/PULL_REQUEST_TEMPLATE.md` matching `docs/plan.md` §9.4

## 4. CI gates (M0.7)

- [x] 4.1 Add `.github/workflows/ci.yml` running on push and pull request
- [x] 4.2 Add `fmt` job: `cargo fmt --all -- --check`
- [x] 4.3 Add `clippy` job:
       `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [x] 4.4 Add `test` job: `cargo test --workspace --all-features`
- [x] 4.5 Add `doc` job: `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"`
- [x] 4.6 Add `deny` job: `cargo deny check`
- [x] 4.7 Add `audit` job: `rustsec/audit-check` action
- [x] 4.8 Configure cargo and rustup caches via `Swatinem/rust-cache`
- [ ] 4.9 Configure GitHub branch protection so all six jobs are required
       (deferred — repo admin / maintainer task post-merge)

## 5. ADR scaffolding (M0.11)

- [x] 5.1 Create `docs/adr/` directory with a `README.md` index
- [x] 5.2 Write `docs/adr/0001-pluggable-architecture.md` capturing
       `docs/plan.md` §2.1
- [x] 5.3 Write `docs/adr/0002-broker-crate-boundary.md` (resolves the
       open question from M1.6)

## 6. Wire protocol (M1.1 – M1.2)

- [x] 6.1 Define `proto/v1.proto` with `package cherenkov.v1;`,
       `ClientFrame`/`ServerFrame` oneofs, `Subscribe`, `Unsubscribe`,
       `Publish`, `Publication`, `SubscribeOk`, `Error` messages
- [x] 6.2 Wire `prost-build` in `cherenkov-protocol/build.rs`, using
       `protoc-bin-vendored` for hermetic builds
- [x] 6.3 Hand-write `cherenkov-protocol/src/frame.rs` ergonomic wrappers
- [x] 6.4 Implement `encode_*(frame) -> Bytes` and
       `decode_*(&[u8]) -> Result<Frame, DecodeError>` helpers
- [x] 6.5 Add `proptest` round-trip test for both directions
- [x] 6.6 Add `insta` snapshot tests for canonical encodings of every variant
- [x] 6.7 Run `cargo test -p cherenkov-protocol` and confirm green

## 7. Hub core (M1.3 – M1.5)

- [x] 7.1 Define trait `ChannelKind` in `cherenkov-core/src/channel.rs`
- [x] 7.2 Define trait `Transport` in `cherenkov-core/src/transport.rs`
- [x] 7.3 Define trait `Broker` in `cherenkov-core/src/broker.rs` (kept in
       core per ADR 0002)
- [x] 7.4 Implement `Session` and `SessionRegistry` with sharded `DashMap`s
       and the reverse `channel → Vec<SessionId>` index
- [x] 7.5 Implement `Hub` with `handle_subscribe`, `handle_unsubscribe`,
       `handle_publish`, plus a `HubBuilder`
- [x] 7.6 Define `HubError` via `thiserror`, with contextful variants
- [x] 7.7 Add unit tests for happy path, double-subscribe, double-unsubscribe,
       and publish-routing
- [x] 7.8 Enable `#![warn(missing_docs)]` (via `[lints.rust]`) and document
       every `pub` item

## 8. Memory broker (M1.6)

- [x] 8.1 ADR 0002 records the decision: `Broker` lives in
       `cherenkov-core`; `MemoryBroker` lives in `cherenkov-broker`
- [x] 8.2 Implement `MemoryBroker` over `tokio::sync::broadcast` with
       lazy topic creation and reaping when receiver count drops to 0
- [x] 8.3 Surface `Lagged` events as a `cherenkov_broker_dropped_total`
       metric via the `metrics` crate
- [x] 8.4 Add unit tests for lazy creation, reaping, and fan-out

## 9. Pub/sub channel (M1.7)

- [x] 9.1 Implement `PubSubChannel` implementing `ChannelKind`
- [x] 9.2 Maintain per-channel ring buffer with TTL and max-size bounds
- [x] 9.3 Return `epoch` and current `offset` on subscribe
- [x] 9.4 Add unit tests for happy path, eviction by size, eviction by TTL,
       and absence of replay on reconnect

## 10. WebSocket transport (M1.8)

- [x] 10.1 Implement `WsTransport` mounted at `/connect/v1` via `axum`
- [x] 10.2 Decode incoming binary messages with
       `cherenkov_protocol::decode_client` and dispatch to the hub
- [x] 10.3 Encode hub publications as `ServerFrame` and write to the
       client as binary messages
- [x] 10.4 Cancel-safe disconnect cleanup: explicit `close_session` plus
       writer-task `abort` on inbound EOF (no `Drop`-driven state changes)
- [x] 10.5 On decode errors, send a final `Error` frame and close the
       socket gracefully

## 11. Server binary and demo (M1.9 – M1.11)

- [x] 11.1 Implement `cherenkov-server` `main.rs` with `clap` for
       `--config <path>` and `tracing-subscriber` for JSON / pretty logs
- [x] 11.2 Load YAML via `figment` with `CHERENKOV_*` env overrides
- [x] 11.3 Compose `Hub` with `MemoryBroker`, `PubSubChannel`, and
       `WsTransport`; bind to configured address; SIGINT / SIGTERM clean exit
- [x] 11.4 Write `examples/echo/index.html`, `client.html`, `config.yaml`,
       and `README.md`
- [x] 11.5 Add `crates/cherenkov-server/tests/echo.rs` integration test
- [x] 11.6 Verify `cargo run -p cherenkov-server -- --config examples/echo/config.yaml`
       boots and the integration test passes

## 12. Documentation (M0.12, M1.12)

- [x] 12.1 `docs/plan.md` is the renamed brief (already done as precursor)
- [x] 12.2 Write `README.md` with the project pitch, the `0.1.0` quickstart,
       and the differentiators from `docs/plan.md` §1
- [x] 12.3 Cross-link the README, `docs/plan.md`, ADR index, and OpenSpec
       change

## 13. Definition of Done

- [x] 13.1 `cargo fmt --all -- --check` exits 0
- [x] 13.2 `cargo clippy --workspace --all-targets --all-features -- -D warnings`
       exits 0
- [x] 13.3 `cargo test --workspace --all-features` exits 0 (30 tests)
- [x] 13.4 `cargo doc --workspace --no-deps` builds with zero warnings under
       `RUSTDOCFLAGS="-D warnings"`
- [x] 13.5 `cargo deny check` exits 0 (advisories, bans, licenses, sources)
- [ ] 13.6 `cargo audit` exits 0 (deferred to CI — `cargo-audit` not in the
       local environment; `rustsec/audit-check` action runs it on every PR)
- [ ] 13.7 GitHub Actions CI is green on the merge commit (verifies post-push)
- [x] 13.8 Echo demo runs end-to-end via a single command on a clean clone
- [x] 13.9 Self-review pass

## Notes

* **MSRV bump.** Plan §3.11 pins MSRV to 1.83. The current dependency
  ecosystem (notably `clap_lex 1.x`) requires Rust edition 2024, which is
  only available from Rust 1.85 onwards. The `rust-toolchain.toml` and
  `[workspace.package].rust-version` were both bumped to **1.85**; this is a
  deliberate, documented bump (not silent) and should be captured in a
  CHANGELOG entry alongside the merge commit.
* **rustfmt unstable options.** `imports_granularity = "Module"` and
  `group_imports = "StdExternalCrate"` from `docs/plan.md` §4.8 are
  nightly-only rustfmt features and emit warnings on stable. They were
  removed from `rustfmt.toml`; the convention applies in code review only
  until we run a nightly-rustfmt CI job.
* **Branch protection.** Task 4.9 requires GitHub admin permissions
  (configuring required status checks) and is deferred to the maintainer
  after the PR merges.
