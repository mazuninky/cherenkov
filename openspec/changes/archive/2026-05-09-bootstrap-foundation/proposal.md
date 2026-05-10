## Why

Cherenkov is at pre-`0.1.0` with an empty repository. Before any feature work
can land, the project needs a healthy workspace skeleton (M0) and a minimum
lovable product that proves the architecture works end-to-end: two browsers
chatting in a room over WebSocket (M1). This change captures both phases as a
single foundational milestone — without it, downstream work (schemas, CRDTs,
auth, WebTransport) has nothing to plug into.

The phases are intentionally bundled: M0 alone produces a repo that compiles
nothing useful, and M1 alone cannot land without the workspace, CI, and
lint guardrails from M0. Shipping them together gives reviewers a single
coherent demo and gives the project a credible "hello world" to point at.

## What Changes

- Establish the 14-crate Cargo workspace (`cherenkov-protocol`,
  `cherenkov-core`, `cherenkov-channel-pubsub`, `cherenkov-broker`,
  `cherenkov-transport-ws`, `cherenkov-server`, plus stubs for the
  yet-to-be-implemented crates) with a pinned MSRV of 1.83.
- Wire up the non-negotiable CI gates: `fmt`, `clippy -D warnings`,
  `cargo test --workspace --all-features`, `cargo doc --no-deps`,
  `cargo deny`, and `cargo audit`. Linux only; macOS/Windows deferred.
- Add the canonical workspace tooling files: `rust-toolchain.toml`,
  `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.cargo/config.toml`,
  `.gitignore`, `LICENSE` (MIT), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
  `SECURITY.md`, `.github/` issue and PR templates.
- Define the `v1` wire protocol in `proto/v1.proto` (Protobuf), generated
  via `prost-build`, with hand-written ergonomic wrapper types and snapshot
  + property round-trip tests.
- Define the three core extension traits in `cherenkov-core`: `ChannelKind`,
  `Transport`, `Broker`. No concrete implementations imported by core.
- Implement the `Hub` skeleton with `handle_subscribe`, `handle_unsubscribe`,
  `handle_publish`. Auth and schema validation are stubbed (always-allow)
  for this milestone.
- Implement `Session` and `SessionRegistry` with sharded `DashMap`s, plus a
  reverse `channel → Vec<SessionId>` index for fan-out.
- Ship the first concrete channel kind (`PubSubChannel`), the first broker
  (`MemoryBroker` over `tokio::sync::broadcast`), and the first transport
  (`WsTransport` over `axum` + `tokio-tungstenite`).
- Ship `cherenkov-server`, the binary that loads YAML config (figment) and
  wires everything together, single-node only.
- Ship `examples/echo/` (HTML + JS, no SDK yet) and an integration test in
  `cherenkov-server/tests/echo.rs` that proves end-to-end fan-out.
- Capture the pluggable architecture rationale as ADR
  `docs/adr/0001-pluggable-architecture.md`.
- Move the working brief to `docs/plan.md` (already done as a precursor to
  this change).

## Capabilities

### New Capabilities

- `workspace-bootstrap`: workspace layout, MSRV, lint/format/deny config,
  CI gates, license and governance files, ADR scaffolding.
- `wire-protocol`: `v1` Protobuf schema, generated Rust types, ergonomic
  frame wrappers, encode/decode helpers, round-trip and snapshot tests.
- `hub-core`: `Hub`, `Session`, `SessionRegistry`, and the three extension
  traits (`ChannelKind`, `Transport`, `Broker`). No concrete implementations.
- `pubsub-channel`: `PubSubChannel` implementing `ChannelKind` with bounded
  in-memory history (TTL + max size).
- `memory-broker`: `MemoryBroker` implementing `Broker` over
  `tokio::sync::broadcast` with one channel per topic.
- `ws-transport`: `WsTransport` implementing `Transport` over `axum` +
  `tokio-tungstenite`; decodes `ClientFrame`, dispatches to the hub,
  encodes `ServerFrame` back.
- `server-binary`: `cherenkov-server` binary, YAML config loader, hub
  composition, the `examples/echo/` demo, and the end-to-end integration
  test.

### Modified Capabilities

None — this is the founding change.

## Impact

- **Code**: creates the entire repository structure under `crates/`,
  `examples/`, `docs/`, `.github/`, `proto/`. Previously empty repo gains
  ~14 crate stubs plus ~5 fully implemented crates.
- **APIs**: establishes the `v1` wire protocol and the three core traits.
  All three are versioned: protocol via `/connect/v1`, traits via crate
  semver. Wire protocol changes from this point forward require an ADR
  (per `docs/plan.md` §2.5).
- **Dependencies**: introduces the canonical pinned versions from
  `docs/plan.md` §3 — `tokio`, `prost`, `axum`, `tokio-tungstenite`,
  `dashmap`, `figment`, `tracing`, plus dev-dependencies (`insta`,
  `proptest`, `criterion`).
- **CI/release**: green CI on every PR is now non-negotiable. No release
  artifacts yet — that is M2+.
- **Out of scope**: WebTransport, SSE, schemas/validation, CRDT channels,
  real auth, backend proxy events, recovery on reconnect, Redis/NATS
  brokers, admin UI. Each gets its own change in a later milestone.
