# Cherenkov architecture

This document describes the shape of the Cherenkov codebase: the layers,
the extension points, the data flow between them, and how the workspace
crates fit together. It is meant as a starting point for new
contributors and integrators; deeper rationale lives in the ADRs under
[`docs/adr/`](adr/) and the long-form working brief in
[`docs/plan.md`](plan.md).

## 1. Mental model in one diagram

```
                                  +----------------------------+
                                  |        cherenkov-server    |
                                  |  (binary; wires it up)     |
                                  +------------+---------------+
                                               |
                                               v
   +-----------+        +-------------+    +----+-------+    +---------+
   |  Client   | <----> | Transport   |<-->|    Hub     |<-->| Broker  |
   |  (any     |  wire  | ws/wt/sse   |    |            |    | memory/ |
   |  language)|        +-------------+    +----+-------+    | redis/  |
   +-----------+                                |            | nats    |
                                                v            +---------+
                                       +--------+--------+
                                       |  ChannelKind    |
                                       | pubsub / crdt-* |
                                       +-----------------+
```

The hub is the single coordination point. Around it sit three swappable
pieces: a transport (how clients reach the server), a channel kind (how a
class of channels behaves), and a broker (how publications cross node
boundaries). Two more extension points — `Authenticator` and `AclChecker`
— sit on the hub's request path; a sixth, `SchemaValidator`, runs before
publications reach the channel kind.

## 2. The pluggable core

Six traits in `cherenkov-core` describe everything that can be replaced.
The core itself is intentionally minimal: it never imports a concrete
implementation. Concrete pieces live in their own crates and are wired
in by `cherenkov-server` at startup. ADRs
[0001](adr/0001-pluggable-architecture.md) and
[0002](adr/0002-broker-crate-boundary.md) cover the rationale.

| Trait | Responsibility | Default impl | Concrete impls |
| --- | --- | --- | --- |
| `Transport` | accept connections, frame in/out, push frames into the hub | — | `cherenkov-transport-ws`, `cherenkov-transport-sse`, `cherenkov-transport-wt` |
| `ChannelKind` | semantics of a class of channels (subscribe / publish / replay) | — | `cherenkov-channel-pubsub`, `cherenkov-channel-crdt` |
| `Broker` | propagate publications between hub instances | `MemoryBroker` (in-process) | `cherenkov-broker-redis`, `cherenkov-broker-nats` |
| `SchemaValidator` | validate a publication's bytes against the namespace's contract | `AllowAllValidator` | `cherenkov-schema` (JSON Schema) |
| `Authenticator` | turn a `Connect` token into `SessionClaims` | `AllowAllAuthenticator` (anonymous) | `cherenkov-auth` (JWT HS256/RS256) |
| `AclChecker` | decide whether `(subject, action, channel)` is permitted | `AllowAllAcl` | `cherenkov-auth` (`NamespaceAcl`, glob rules) |

All defaults are no-ops — the M1 echo demo runs with anonymous sessions,
no schema validation, no ACL, and a memory broker. A production
deployment swaps each piece in via the YAML config.

## 3. Hub, sessions, and the request path

The `Hub` owns:

- a `SessionRegistry` (sharded `DashMap` of `SessionId → Session`, plus a
  reverse `channel → [SessionId]` index used for local fan-out)
- one default `ChannelKind`, plus optional per-namespace overrides keyed
  by the prefix before the first `.` in a channel name
- one `Broker` (memory by default; Redis or NATS in clustered mode)
- one `SchemaValidator`, one `Authenticator`, one `AclChecker`

Each connected client is a `Session`: a session id, an outbox `mpsc`
sender (filled by the hub, drained by the transport's writer task),
auth claims (set by `handle_connect`), a per-channel map of forwarder
tasks, and a shutdown `Notify` for `kick_session`.

A typical request flow for a client that connects, subscribes, and
publishes:

```
Client                 Transport            Hub             ChannelKind   Broker
  |  --- Connect ----->   |  -- decode  -->  |
  |                       |                  | -> Authenticator.verify(token)
  |                       |                  | -> Session.set_claims(...)
  |  <-- ConnectOk -----  |                  |
  |                                          |
  |  --- Subscribe ---->  |  -- decode  -->  | -> AclChecker(subject, Subscribe, channel)
  |                       |                  | -> ChannelKind.on_subscribe(channel)
  |                       |                  | -> Broker.subscribe(channel) -> stream
  |                       |                  |    spawn forwarder task: stream -> session.outbox
  |  <-- SubscribeOk ---  |                  |
  |                                          |
  |  --- Publish ------>  |  -- decode  -->  | -> AclChecker(subject, Publish, channel)
  |                       |                  | -> SchemaValidator.validate(channel, &data)
  |                       |                  | -> ChannelKind.on_publish(channel, data) -> Publication
  |                       |                  | -> Broker.publish(topic, publication)
  |                                          |                              |
  |                                          |                              v
  |  <-- Publication ---  |                                            (fanned out
  |                       (forwarder task copied it from broker stream  to all subscribers)
  |                        into session.outbox)
```

Two invariants that hold across this flow:

1. **The hub never touches payload bytes**. The wire `data` field is
   opaque from transport in to broker out. Schema validation reads it,
   but never logs it.
2. **Per-session ordering is preserved**. The session outbox is a single
   `mpsc::Sender`, and the transport's writer task drains it in FIFO
   order.

## 4. Wire protocol (v1)

Defined in `cherenkov-protocol`. Frames are Protobuf messages, written
without a length prefix because every transport already provides one
(WebSocket binary frame, SSE event, WebTransport datagram/stream).

`ClientFrame` is one of: `Connect`, `Subscribe`, `Unsubscribe`,
`Publish`. `ServerFrame` is one of: `ConnectOk`, `SubscribeOk`,
`UnsubscribeOk`, `Publication`, `Error`. Every request carries a
`request_id` that is echoed back in the matching ack or error frame.

Error codes (numeric, append-only within v1):

| Code | Variant | Meaning |
| ---: | --- | --- |
| 1 | `InvalidFrame` | frame did not match the v1 schema |
| 2 | `InvalidChannel` | channel name is malformed |
| 3 | `Unauthorized` | operation not permitted for this session |
| 4 | `Internal` | server-side bug; clients retry with backoff |
| 5 | `ValidationFailed` | publish payload failed schema validation |
| 6 | `InvalidToken` | `Connect` token is expired/malformed |
| 7 | `AclDenied` | ACL rejected the action |
| 8 | `NotConnected` | non-`Connect` frame before `Connect` (when auth required) |

The hand-written wrappers in
[`crates/cherenkov-protocol/src/frame.rs`](../crates/cherenkov-protocol/src/frame.rs)
are the only types that cross crate boundaries — the `prost`-generated
types stay private so codegen tooling can change without breaking the
public surface.

## 5. Workspace layout

```
crates/
├── cherenkov-protocol/         # v1 wire schema (.proto + Rust wrappers)
├── cherenkov-core/             # Hub, Session, the six extension traits
│
├── cherenkov-channel-pubsub/   # in-memory bounded-history pub/sub
├── cherenkov-channel-crdt/     # Y.js + Automerge channel kinds
│
├── cherenkov-transport-ws/     # WebSocket over axum
├── cherenkov-transport-sse/    # Server-Sent Events
├── cherenkov-transport-wt/     # WebTransport (HTTP/3 + QUIC)
│
├── cherenkov-broker/           # MemoryBroker (in-process fan-out)
├── cherenkov-broker-redis/     # Redis pub/sub broker
├── cherenkov-broker-nats/      # NATS broker
│
├── cherenkov-schema/           # JsonSchemaRegistry (per-namespace)
├── cherenkov-auth/             # JWT authenticator + NamespaceAcl
├── cherenkov-admin/            # admin HTTP API + console UI
│
├── cherenkov-server/           # the binary: config, composition, main()
├── cherenkov-sdk-ts/           # TypeScript SDK + AsyncAPI generator CLI
│
├── cherenkov/                  # facade crate; feature-gated re-exports
└── cherenkov-test-support/     # shared test fixtures (test-support feature)
```

A few invariants enforced by `cargo deny` and review:

- **`cherenkov-core` has minimum dependencies.** No transport libraries,
  no broker libraries, no schema libraries, no HTTP framework. If a
  feature requires a heavy dependency, it goes in an adjacent crate.
- **Concrete impls live in dedicated crates.** Pulling in
  `cherenkov-core` should not pull in axum, jsonschema, redis, etc.
- **The facade crate is feature-gated.** `cherenkov = { features = [...]
  }` lets a downstream user pick exactly the pieces they need.

## 6. Server composition

`cherenkov-server` is the production binary. Its job is to read a YAML
config (with `CHERENKOV_*` env overrides via `figment`), instantiate the
right concrete trait impls, hand them to a `HubBuilder`, and call
`build()`. The result is a `Hub` plus the spawned transport tasks.

The `ServerConfig` struct in
[`crates/cherenkov-server/src/config.rs`](../crates/cherenkov-server/src/config.rs)
defines the YAML schema:

- `transport.ws` / `transport.sse` — listen addresses, paths, outbox
  capacity
- `broker` — backend (`memory` / `redis` / `nats`) plus URL
- `channel_pubsub` — history depth, payload limits
- `channel_kinds` — per-namespace override map
  (`pubsub` / `crdt-yjs` / `crdt-automerge`)
- `namespaces` — per-namespace JSON Schema declarations (inline or
  `schema_path`)
- `auth` — optional JWT settings (HMAC secret or RSA public key)
- `acl` — optional rule list `(subject_glob, action, channel_glob,
  effect)`
- `admin` — admin HTTP API + console settings
- `log` — format, level

For tests, `cherenkov-server::app::run_with_listener` is the
test-friendly entry point: it accepts a `tokio::net::TcpListener` (so
tests can bind port 0 and read the assigned port back), wires the WS
transport, and spawns the accept loop.

## 7. Stateful pieces and where they live

| State | Owner | Lifetime |
| --- | --- | --- |
| Active sessions | `Hub::sessions` (`SessionRegistry`) | until disconnect or `kick_session` |
| Per-session subscription forwarders | `Session::subscriptions` (DashMap of JoinHandles) | until session drops or unsubscribe |
| Pub/sub channel history | `PubSubChannel` (per-channel ring buffer) | until eviction or process exit |
| CRDT documents | `YjsChannel` / `AutomergeChannel` (per-channel doc) | until process exit |
| Schema registry | `JsonSchemaRegistry` | immutable after `HubBuilder::build` |
| ACL rules | `NamespaceAcl` | immutable after `HubBuilder::build` |
| Cross-node fan-out | `Broker` (memory / redis / nats) | for the broker's lifetime |

Hot-reload of schemas, ACLs, and namespace declarations is deliberately
out of scope for the current milestones — restart the server.

## 8. Where to read next

| If you want to … | Read |
| --- | --- |
| Understand the broad plan and milestones | [`docs/plan.md`](plan.md) |
| Understand a specific design decision | [`docs/adr/`](adr/) |
| See the wire types in code | [`crates/cherenkov-protocol/src/frame.rs`](../crates/cherenkov-protocol/src/frame.rs) |
| See the trait surface | [`crates/cherenkov-core/src/lib.rs`](../crates/cherenkov-core/src/lib.rs) |
| See how it composes at runtime | [`crates/cherenkov-server/src/app.rs`](../crates/cherenkov-server/src/app.rs) |
| Run a working demo | [`examples/echo/`](../examples/echo/), [`examples/price-feed/`](../examples/price-feed/), [`examples/live-cursors/`](../examples/live-cursors/) |
