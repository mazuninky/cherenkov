# crdt-channel-yjs Specification

## Purpose
TBD - created by archiving change crdt-channels. Update Purpose after archive.
## Requirements
### Requirement: YjsChannel implements ChannelKind backed by yrs
`cherenkov-channel-crdt` SHALL ship `yjs::YjsChannel` implementing
`cherenkov_core::ChannelKind`. Each channel name SHALL own a single
`yrs::Doc` held in memory; `on_publish` SHALL decode the payload as a
Y.js update and apply it to that doc, returning `CrdtError::InvalidUpdate`
on decode or apply failure.

#### Scenario: Valid Y.js update is rebroadcast
- **WHEN** a client publishes a well-formed Y.js update on
  `"docs.alpha"`
- **THEN** the channel's `on_publish` returns the same bytes as the
  publication payload, and subscribers receive them via the broker

#### Scenario: Invalid bytes return InvalidUpdate
- **WHEN** a client publishes random bytes on `"docs.alpha"`
- **THEN** `on_publish` returns `Err(CrdtError::InvalidUpdate { engine:
  "yjs", channel: "docs.alpha", reason })` and the broker sees no
  publication

### Requirement: YjsChannel exposes a snapshot helper
`YjsChannel` SHALL expose a `snapshot(channel: &str) -> Option<Vec<u8>>`
that returns the current full-state Y.js encoding for the channel's
doc, or `None` if the channel has not yet seen a publish. This helper
SHALL be safe to call concurrently with `on_publish`.

#### Scenario: Snapshot encodes accumulated state
- **GIVEN** two valid Y.js updates have been published to `"docs.alpha"`
- **WHEN** `snapshot("docs.alpha")` is called
- **THEN** the returned bytes decode to a Y.js doc that contains both
  updates' effects

### Requirement: yjs feature is enabled by default
The `cherenkov-channel-crdt` crate SHALL declare a `yjs` cargo feature
that pulls in the `yrs` dependency, and SHALL include `yjs` in its
`default = [...]` feature set so the channel kind is available without
extra configuration.

#### Scenario: Default build exposes YjsChannel
- **WHEN** a downstream crate adds `cherenkov-channel-crdt = "0.x"` to
  its `Cargo.toml` without specifying `default-features = false`
- **THEN** `cherenkov_channel_crdt::YjsChannel` is in scope

