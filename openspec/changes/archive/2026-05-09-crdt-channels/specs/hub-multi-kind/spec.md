## ADDED Requirements

### Requirement: HubBuilder routes namespaces to channel kinds
`HubBuilder` SHALL expose `with_channel_kind_for(namespace: impl
Into<String>, kind: Arc<dyn ChannelKind>)` that registers a per-namespace
override for the channel kind. The existing `with_channel_kind(kind)`
SHALL continue to register the *default* kind used for any channel
whose namespace was not overridden.

#### Scenario: Namespace override beats default
- **GIVEN** a builder with default kind `pubsub` and override
  `with_channel_kind_for("docs", crdt_yjs)`
- **WHEN** the hub routes `"docs.alice"` and `"rooms.lobby"`
- **THEN** `"docs.alice"` resolves to the Y.js kind and `"rooms.lobby"`
  resolves to the pubsub kind

#### Scenario: Single-kind callers stay source-compatible
- **WHEN** a binary builds a `Hub` with only `with_channel_kind(...)`
- **THEN** the build succeeds and every channel uses the registered kind

### Requirement: namespace_of returns the prefix before the first dot
`cherenkov-core` SHALL expose a `pub fn namespace_of(channel: &str) ->
&str` that returns the substring of `channel` before the first `.`. If
the channel name contains no `.`, the entire name SHALL be returned.

#### Scenario: Dotted channel splits at the first separator
- **WHEN** `namespace_of("orders.created.v2")` is called
- **THEN** the function returns `"orders"`

#### Scenario: Non-dotted channel returns itself
- **WHEN** `namespace_of("lobby")` is called
- **THEN** the function returns `"lobby"`

### Requirement: Hub::kind_for resolves namespace overrides
`Hub` SHALL provide an internal `kind_for(channel)` that consults the
namespace override map first and falls back to the registered default
kind. Subscribe / unsubscribe / publish SHALL all dispatch through this
single resolution path so namespace routing is consistent across
operations.

#### Scenario: Subscribe and publish see the same routing decision
- **GIVEN** override `with_channel_kind_for("docs", crdt_yjs)`
- **WHEN** a session subscribes to `"docs.x"` and another publishes to
  `"docs.x"`
- **THEN** both calls reach the Y.js channel kind, never the default
