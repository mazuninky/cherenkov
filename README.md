# Cherenkov

A self-hosted, language-agnostic real-time messaging server written in Rust.

> **Status:** pre-`0.1.0`, alpha. The wire protocol may still break. CI gates
> are non-negotiable, but no backwards-compatibility promises yet.

Three things Cherenkov sets out to do well:

1. **First-class CRDT channels** — Y.js and Automerge as channel kinds
   (M4).
2. **WebTransport with per-channel delivery profiles** — datagrams vs
   streams (M5).
3. **Schema-aware everything** — Protobuf / JSON-Schema validation,
   AsyncAPI export, typed TypeScript SDK generator (M2 in progress —
   JSON Schema validation lands in this milestone; AsyncAPI export and
   the TS SDK generator follow).

Today Cherenkov ships M1 (the minimum lovable product — two browsers
chatting in a room over WebSocket) and the first slice of M2: per-namespace
JSON Schema validation enforced server-side, with malformed publications
rejected before they reach the broker. See
[`docs/adr/0003-schema-as-contract.md`](docs/adr/0003-schema-as-contract.md)
for the design rationale.

## Quickstart

```sh
# Boot the server with the bundled echo-demo config.
cargo run -p cherenkov-server -- --config examples/echo/config.yaml
```

The server listens on `127.0.0.1:7000` and exposes the v1 WebSocket
transport at `/connect/v1`.

In a second terminal, serve the static demo files:

```sh
cd examples/echo && python3 -m http.server 8080
```

Open <http://127.0.0.1:8080/index.html>. The page embeds two iframes that
each open a WebSocket, subscribe to `rooms.lobby`, and round-trip
publications through the hub. Type into either pane and press Enter — the
other pane sees the publication arrive.

The same flow is exercised hermetically in
[`crates/cherenkov-server/tests/echo.rs`](crates/cherenkov-server/tests/echo.rs):

```sh
cargo test -p cherenkov-server --test echo
```

## What is in the box

| Crate                          | Role                                              | M1?     |
| ------------------------------ | ------------------------------------------------- | ------- |
| `cherenkov-protocol`           | v1 Protobuf wire schema + ergonomic Rust wrappers | yes     |
| `cherenkov-core`               | `Hub`, `Session`, the three extension traits      | yes     |
| `cherenkov-channel-pubsub`     | `PubSubChannel` (in-memory bounded history)       | yes     |
| `cherenkov-broker`             | `MemoryBroker` (in-process fan-out)               | yes     |
| `cherenkov-transport-ws`       | WebSocket transport over `axum`                   | yes     |
| `cherenkov-server`             | binary that wires the above together              | yes     |
| `cherenkov-schema`             | per-namespace JSON Schema registry + validation   | yes (M2) |
| `cherenkov-channel-crdt`       | Y.js + Automerge CRDT channels                    | M4 stub |
| `cherenkov-broker-redis`       | Redis broker                                      | M6 stub |
| `cherenkov-broker-nats`        | NATS broker                                       | M6 stub |
| `cherenkov-transport-wt`       | WebTransport (HTTP/3 + QUIC)                      | M5 stub |
| `cherenkov-transport-sse`      | Server-Sent Events                                | post-M5 stub |
| `cherenkov-auth`               | JWT, ACLs, backend proxy events                   | M3 stub |
| `cherenkov-admin`              | Admin HTTP API + UI                               | M7 stub |
| `cherenkov-sdk-ts`             | TypeScript SDK generator CLI                      | M2 stub |
| `cherenkov`                    | Facade crate re-exporting the public API          | yes     |
| `cherenkov-test-support`       | Shared test fixtures, behind `test-support` feature | scaffold |

## Architecture in one minute

```
              +--------------+         +-----------------+
  client ---> |   Transport  | <-----> |       Hub       | <-- Broker (cross-node)
              | (ws/wt/sse)  |         |  (Sessions,     |
              +--------------+         |   ChannelKinds) |
                                       +-----------------+
                                                 |
                                                 v
                                        Channel kind state
                                       (pub/sub history,
                                        CRDT documents, ...)
```

Three pluggable traits in `cherenkov-core`:

```rust
trait ChannelKind: Send + Sync + 'static { /* pub/sub, crdt, ... */ }
trait Transport:   Send + Sync + 'static { /* ws, wt, sse, ... */ }
trait Broker:      Send + Sync + 'static { /* memory, redis, nats, ... */ }
```

The core never imports a concrete kind, transport, or broker. The
`cherenkov-server` binary picks concrete implementations at startup. See
[`docs/adr/0001-pluggable-architecture.md`](docs/adr/0001-pluggable-architecture.md)
for the rationale and rejected alternatives.

## Local checks

The same six gates run in CI; running them locally before pushing is the
shortest path to a green PR.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo deny check
cargo audit
```

## Documentation

* [`docs/architecture.md`](docs/architecture.md) — high-level
  architectural overview: layers, extension traits, request flow, wire
  protocol, workspace layout. Start here.
* [`docs/plan.md`](docs/plan.md) — the working contract for AI and human
  contributors. The single most important document in the repo.
* [`docs/adr/`](docs/adr/) — architectural decision records.
* [`examples/echo/`](examples/echo) — the M1 minimum-lovable-product demo.
* [`examples/price-feed/`](examples/price-feed) — schema-validated tick
  publisher + live table; demonstrates the wire-level `ValidationFailed`
  error frame.
* [`examples/live-cursors/`](examples/live-cursors) — multi-tab "ghost
  cursors" demo; exercises pub/sub fan-out at ~30 Hz per peer.
* [`openspec/specs/`](openspec/specs) — archived per-capability specs;
  active proposals (when any) live in
  [`openspec/changes/`](openspec/changes).

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Security reports go to the
address in [`SECURITY.md`](SECURITY.md), not the public tracker.
Participation is governed by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## License

MIT. See [`LICENSE`](LICENSE).
