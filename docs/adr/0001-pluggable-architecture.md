# ADR 0001: Pluggable architecture via trait objects

## Status

Accepted, 2026-05-09.

## Context

Cherenkov needs to support multiple, independently-evolving extension points:

* **Channel kinds** — pub/sub, CRDT (Y.js, Automerge), presence, history;
  more will follow.
* **Transports** — WebSocket today; WebTransport and Server-Sent Events on
  the roadmap; potentially others (e.g. raw TCP) in private deployments.
* **Brokers** — single-node memory broker today; Redis and NATS for
  cross-node fan-out; Kafka and proprietary brokers later.

A single binary may need to host more than one of each: pub/sub for
`rooms.*` plus CRDT for `docs.*`, WebSocket plus WebTransport, and so on.

We must pick a representation of these extension points that:

1. Lets concrete implementations live in their own crates without forcing
   the core to depend on their dependencies (`docs/plan.md` §2.2).
2. Lets a runtime-configured server compose any combination at startup.
3. Keeps the public API stable across the addition of new kinds, transports,
   or brokers — adding a new one must not require touching `cherenkov-core`.

## Decision

`cherenkov-core` defines three Rust traits — `ChannelKind`, `Transport`,
and `Broker` — and the hub holds them as `Arc<dyn _>` (or `Box<dyn _>` in
the case of one-shot transports).

Implementations live in dedicated `cherenkov-*` crates. The
`cherenkov-server` binary wires concrete implementations into the hub
through a builder pattern (`HubBuilder::with_channel_kind`,
`with_broker`, `with_transport`).

`cherenkov-core` is forbidden from depending on any concrete kind,
transport, broker, schema library, or HTTP framework. `cargo deny` will
enforce this at the dependency level once the `bans.deny` list is populated
in a follow-up change.

## Consequences

* **Open extension surface.** Anyone can publish a `cherenkov-channel-foo`
  crate that satisfies `ChannelKind` without touching the core. We can
  ship Redis and NATS brokers without rewriting plumbing.
* **Trait-object indirection on the hot path.** Every publish goes through
  one virtual method call into the channel kind and one into the broker.
  That cost is bounded — fan-out work and (de)serialization dominate — and
  is acceptable for the flexibility we get. If profiling later shows
  measurable overhead in a hot path we can specialize generically for
  known kinds without breaking the public API.
* **Builder ergonomics, not a registry.** We pick a builder over a
  named-registry pattern (e.g. "register `pubsub` as a name") because the
  channel-name-to-kind mapping is a deployment policy decision and belongs
  in the binary's wiring, not in `cherenkov-core`.
* **Disciplined deps in core.** The dependency allowlist in
  `docs/plan.md` §2.2 is now load-bearing. Any contribution that pulls a
  concrete-kind dependency into `cherenkov-core/src/**` is rejected.

## Alternatives considered

### Static enum of all known kinds / transports / brokers

```rust
pub enum ChannelKind {
    PubSub(PubSubChannel),
    YjsCrdt(YjsChannel),
    Automerge(AutomergeChannel),
    // ...
}
```

Rejected. The core would have to import every concrete crate, defeating
§2.2 of the plan. Adding a new kind would require a `cherenkov-core`
release. External implementations (third-party CRDTs, custom protocols)
would be impossible.

### Conditional compilation (Cargo features)

Have `cherenkov-core` depend on every implementation crate behind a feature
flag, and let the binary opt in to the ones it wants.

Rejected. Feature flags do not compose well across the dependency graph
(they tend to leak), they make dependency review harder, and they still
require `cherenkov-core` to *know about* every implementation. The whole
point is to keep the core ignorant.

### Generic over the trait (`Hub<K, B>`)

Make the hub generic so the trait method calls are monomorphised away.

Rejected. A single hub may legitimately host multiple channel kinds at
once (one for `rooms.*`, another for `docs.*`). Generics cannot express
that without a kind-tag enum, which puts us back in alternative #1.
