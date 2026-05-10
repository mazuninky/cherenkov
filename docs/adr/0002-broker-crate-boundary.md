# ADR 0002: `Broker` trait lives in `cherenkov-core`

## Status

Accepted, 2026-05-09.

## Context

ADR 0001 defines three pluggable extension points: `ChannelKind`,
`Transport`, and `Broker`. The first two trait definitions live in
`cherenkov-core`; concrete implementations live in their own crates. For
the broker, `docs/plan.md` (M1.6) deliberately deferred the question of
where the *trait* itself should live — in `cherenkov-core` alongside its
peers, or in a separate `cherenkov-broker` crate so that
"broker things" are namespaced together.

Either layout is workable; the question is which causes less churn for
downstream crates and clearer surface for reviewers.

## Decision

Put the `Broker` trait (plus `BrokerError` and `BrokerStream`) in
`cherenkov-core`, mirroring `ChannelKind` and `Transport`. The
`cherenkov-broker` crate contains only the in-process `MemoryBroker`
implementation. Future Redis and NATS implementations live in
`cherenkov-broker-redis` and `cherenkov-broker-nats` respectively.

## Consequences

* **Symmetry.** All three extension-point traits are colocated in one
  crate; readers find them in a single place. The
  `cherenkov-broker*` crate names refer unambiguously to *implementations*.
* **Downstream import shape.** A binary composing a hub imports the trait
  from `cherenkov_core` and the implementation from `cherenkov_broker`.
  Identical to the kind / transport pattern.
* **Avoids a circular dependency risk.** If the trait lived in
  `cherenkov-broker` and the hub depended on it for routing, every crate
  that needed the trait would also pull `cherenkov-broker`'s build graph,
  even when running entirely against (say) the Redis broker.
* **Trade-off: `cherenkov-core` carries the broker trait surface.** That
  is consistent with §2.2's allowlist: the trait declaration adds no
  concrete dependencies beyond `futures` (already present) and
  `cherenkov-protocol` (the `Publication` type).

## Alternatives considered

### Trait in `cherenkov-broker`, in-memory impl alongside

Move the trait out of core into a dedicated `cherenkov-broker` crate that
also exports `MemoryBroker`. Future `cherenkov-broker-redis` and
`cherenkov-broker-nats` would depend on `cherenkov-broker` for the trait.

Rejected. Asymmetric with `ChannelKind` / `Transport` (which live in
core), and forces every crate that wants to write against the trait to
also pull in `MemoryBroker`'s dependencies (`tokio-stream`, `dashmap`,
etc.). Users that ship only with a Redis or NATS broker would still
transitively depend on the in-memory broker's deps via the trait crate.

### Trait in its own crate (`cherenkov-broker-trait`)

A leaner middle ground: the trait alone in a tiny new crate, and every
implementation depends on it.

Rejected. Splitting the workspace further for one trait creates more
maintenance overhead than it saves. Three traits in `cherenkov-core` is
already the right level of granularity.
