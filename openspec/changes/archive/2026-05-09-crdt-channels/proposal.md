## Why

`docs/plan.md` lists "first-class CRDT channels" as one of Cherenkov's
three headline capabilities. M3 closed the auth / ACL gap; M4 makes the
CRDT capability real. With this
change, a namespace can be configured to route through a Y.js doc (via
[`yrs`]) or an Automerge doc, with the server validating each
publication as a CRDT update before rebroadcasting it.

## What Changes

- New module pair in `cherenkov-channel-crdt`: `yjs::YjsChannel` and
  `automerge::AutomergeChannel`. Both implement
  [`cherenkov_core::ChannelKind`].
- Each channel kind owns one in-memory CRDT doc per channel name.
  `on_publish` decodes the payload as an update, applies it, and
  rebroadcasts the same bytes as a `Publication` so subscribers can
  merge them into their local copies.
- `cherenkov-core` gains multi-kind routing: a hub may register a
  default `ChannelKind` plus per-namespace overrides via
  `HubBuilder::with_channel_kind_for`. The new `namespace_of` helper
  returns the part of a channel name before the first `.`.
- `cherenkov-server` config grows a `channel_kinds:` section mapping
  namespace prefixes to `pubsub` / `crdt-yjs` / `crdt-automerge`.
- New integration test
  `cherenkov-server/tests/crdt_yjs.rs` round-trips a Y.js update
  through the WebSocket transport between two clients.

## Impact

- New crate body: `cherenkov-channel-crdt` (was a stub). Ships behind
  default `yjs` and `automerge` features so a downstream user can
  disable either if they want a leaner build.
- `cherenkov-core` API additions: `HubBuilder::with_channel_kind_for`,
  pub `namespace_of` helper. No breaking changes; the single-kind
  builder path still works.
- New workspace dep on `yrs = "0.21"` and `automerge = "0.5"`.
- Test count rises with three new unit tests in the CRDT crate plus
  one new server integration test.

[`yrs`]: https://docs.rs/yrs
