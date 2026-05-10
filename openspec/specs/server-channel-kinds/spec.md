# server-channel-kinds Specification

## Purpose
TBD - created by archiving change crdt-channels. Update Purpose after archive.
## Requirements
### Requirement: Server config exposes channel_kinds map
`cherenkov-server::config::Config` SHALL expose a
`channel_kinds: ChannelKindsConfig` field. `ChannelKindsConfig` SHALL
deserialize as a transparent map from namespace prefix to a
`ChannelKindName` enum whose variants serialize as kebab-case strings:
`pubsub`, `crdt-yjs`, `crdt-automerge`. Pubsub SHALL be the default.

#### Scenario: YAML uses kebab-case kind names
- **WHEN** the loader reads `channel_kinds: { docs: crdt-yjs, sheets:
  crdt-automerge }`
- **THEN** the resulting `ChannelKindsConfig` maps `"docs"` to
  `CrdtYjs` and `"sheets"` to `CrdtAutomerge`

#### Scenario: Missing field defaults to empty
- **WHEN** the loader reads a config that omits `channel_kinds`
- **THEN** the field defaults to an empty map and every channel uses
  the registered default kind (`pubsub`)

### Requirement: Server materializes channel kinds into HubBuilder
`app::run_with_listener` SHALL invoke `build_namespace_kinds(&config.
channel_kinds)` to materialize each `(namespace, ChannelKindName)`
entry into an `Arc<dyn ChannelKind>` and SHALL register every entry on
the `HubBuilder` via `with_channel_kind_for`. The default kind
(`PubSubChannel`) SHALL remain registered via `with_channel_kind`.

#### Scenario: Mixed config wires both kinds
- **GIVEN** `channel_kinds: { docs: crdt-yjs }` in the YAML
- **WHEN** the server boots
- **THEN** `"docs.alpha"` routes to a `YjsChannel`, while
  `"rooms.lobby"` routes to the default `PubSubChannel`

### Requirement: cherenkov-server re-exports channel-kinds types
`cherenkov-server::lib` SHALL re-export `ChannelKindName` and
`ChannelKindsConfig` so integration tests in `cherenkov-server/tests/`
can build configs programmatically.

#### Scenario: Test crate constructs ChannelKindsConfig in code
- **WHEN** an integration test inserts `("docs", ChannelKindName::
  CrdtYjs)` into a programmatically built `ChannelKindsConfig`
- **THEN** the code compiles using only the public re-exports

