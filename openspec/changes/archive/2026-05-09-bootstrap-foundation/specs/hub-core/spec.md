## ADDED Requirements

### Requirement: Core defines three pluggable extension traits
`cherenkov-core` SHALL define the traits `ChannelKind`, `Transport`, and
`Broker`, all `Send + Sync + 'static`. The crate SHALL NOT import any
concrete kind, transport, or broker implementation.

#### Scenario: Core has no concrete-impl dependencies
- **WHEN** a reviewer runs `cargo tree -p cherenkov-core`
- **THEN** the output does not list `redis`, `tokio-tungstenite`, `wtransport`,
  `yrs`, `automerge`, or any other concrete kind/transport/broker library

#### Scenario: Trait objects are usable through Arc<dyn Trait>
- **WHEN** a binary constructs `Arc<dyn ChannelKind>`,
  `Arc<dyn Transport>`, and `Arc<dyn Broker>`
- **THEN** the binary compiles, because each trait is object-safe

### Requirement: Hub provides subscribe, unsubscribe, and publish entry points
The crate SHALL expose a `Hub` type with `async fn handle_subscribe`,
`async fn handle_unsubscribe`, and `async fn handle_publish` that dispatch
to the registered `ChannelKind` for the target channel and update the
`SessionRegistry` accordingly. Auth and schema validation SHALL be stubbed
to always-allow at this milestone.

#### Scenario: Subscribe registers the session for fan-out
- **WHEN** a session calls `handle_subscribe(channel)`
- **THEN** subsequent `handle_publish(channel, payload)` deliveries reach
  that session via the channel kind's `on_publication` path

#### Scenario: Unsubscribe removes the session from fan-out
- **WHEN** a session calls `handle_unsubscribe(channel)` and then
  `handle_publish(channel, payload)` runs
- **THEN** the session does not receive the publication

### Requirement: SessionRegistry maintains sharded reverse index
The crate SHALL provide `Session` and `SessionRegistry` types backed by
sharded `DashMap`s, with a reverse index `channel → Vec<SessionId>` so that
fan-out is O(subscribers) and lock contention is bounded.

#### Scenario: Reverse index returns subscribers for a channel
- **WHEN** N sessions have subscribed to a channel and the hub fans out
  a publication
- **THEN** the reverse index returns exactly those N session ids and no
  others, even under concurrent subscribe/unsubscribe traffic

### Requirement: Errors are typed and contextful
The crate SHALL define a `HubError` enum via `thiserror`, with variants
that carry enough context to diagnose without a stack trace (per
`docs/plan.md` §4.3). The crate SHALL NOT use `anyhow`.

#### Scenario: Error variants carry context
- **WHEN** a reviewer reads `HubError`
- **THEN** every variant carries either a structured field or a `#[from]`
  source, never a bare unit variant for an operational failure

### Requirement: Public items are documented
Every `pub` item in `cherenkov-core` SHALL have a rustdoc comment.
`#![warn(missing_docs)]` SHALL be enabled at the crate root and CI rejects
warnings.

#### Scenario: cargo doc emits no warnings
- **WHEN** CI runs `cargo doc --workspace --no-deps`
- **THEN** the build completes with zero warnings
