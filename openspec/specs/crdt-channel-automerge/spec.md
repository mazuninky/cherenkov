# crdt-channel-automerge Specification

## Purpose
TBD - created by archiving change crdt-channels. Update Purpose after archive.
## Requirements
### Requirement: AutomergeChannel implements ChannelKind backed by automerge
`cherenkov-channel-crdt` SHALL ship `automerge::AutomergeChannel`
implementing `cherenkov_core::ChannelKind`. Each channel name SHALL own
a single `automerge::AutoCommit` (or equivalent) document held in
memory; `on_publish` SHALL decode the payload as one or more Automerge
binary changes and apply them to that document, returning
`CrdtError::InvalidUpdate` on decode or apply failure.

#### Scenario: Valid Automerge change is rebroadcast
- **WHEN** a client publishes a well-formed Automerge change on
  `"sheets.beta"`
- **THEN** the channel's `on_publish` returns the same bytes as the
  publication payload, and subscribers receive them via the broker

#### Scenario: Malformed change returns InvalidUpdate
- **WHEN** a client publishes random bytes on `"sheets.beta"`
- **THEN** `on_publish` returns `Err(CrdtError::InvalidUpdate { engine:
  "automerge", channel: "sheets.beta", reason })` and the broker sees no
  publication

### Requirement: AutomergeChannel exposes a snapshot helper
`AutomergeChannel` SHALL expose a `snapshot(channel: &str) ->
Option<Vec<u8>>` that returns the document's `save()` bytes for the
channel, or `None` if the channel has not yet seen a publish. The
returned bytes SHALL be loadable by an Automerge client to reconstruct
the full document state.

#### Scenario: Snapshot is loadable on a fresh client
- **GIVEN** a channel with one applied change
- **WHEN** `snapshot(channel)` is called and the resulting bytes are
  loaded into a new Automerge document
- **THEN** the new document contains the same materialised state as the
  server's copy

### Requirement: automerge feature is enabled by default
The `cherenkov-channel-crdt` crate SHALL declare an `automerge` cargo
feature that pulls in the `automerge` dependency, and SHALL include
`automerge` in its `default = [...]` feature set so the channel kind
is available without extra configuration.

#### Scenario: Downstream may opt out
- **WHEN** a downstream crate sets `cherenkov-channel-crdt = { version
  = "0.x", default-features = false, features = ["yjs"] }`
- **THEN** the build excludes the `automerge` crate from the dependency
  graph

