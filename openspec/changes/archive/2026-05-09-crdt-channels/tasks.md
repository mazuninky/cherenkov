## 1. Multi-kind hub routing

- [x] 1.1 `HubBuilder::with_channel_kind_for(namespace, kind)`.
- [x] 1.2 `Hub::kind_for(channel)` resolves namespace → kind, falling
       back to the default.
- [x] 1.3 `pub fn namespace_of(channel: &str) -> &str` exposed at the
       crate root.

## 2. CRDT channel kinds

- [x] 2.1 `cherenkov-channel-crdt/src/yjs.rs` with `YjsChannel`
       implementing `ChannelKind`. Persistence in-memory.
- [x] 2.2 `cherenkov-channel-crdt/src/automerge.rs` with
       `AutomergeChannel` implementing `ChannelKind`.
- [x] 2.3 `CrdtError::InvalidUpdate` carrying the engine name + channel
       so client errors are payload-free.
- [x] 2.4 Snapshot helpers (`YjsChannel::snapshot`,
       `AutomergeChannel::snapshot`) for future replay layers.
- [x] 2.5 Default `yjs` and `automerge` cargo features so downstream
       users can opt out of either.

## 3. Server wiring

- [x] 3.1 `ChannelKindsConfig` + `ChannelKindName` in server config.
- [x] 3.2 `app::build_namespace_kinds` materializes the config.
- [x] 3.3 Re-export `ChannelKindName`, `ChannelKindsConfig` from
       `cherenkov-server`'s lib.

## 4. Integration test

- [x] 4.1 `cherenkov-server/tests/crdt_yjs.rs` exercises end-to-end
       round-trip of a Y.js update through the WebSocket transport.

## 5. Definition of Done

- [x] 5.1 `cargo fmt --all -- --check` exits 0
- [x] 5.2 `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits 0
- [x] 5.3 `cargo test --workspace --all-features` exits 0
- [x] 5.4 `cargo doc --workspace --no-deps` builds clean
- [x] 5.5 `cargo deny check` exits 0
