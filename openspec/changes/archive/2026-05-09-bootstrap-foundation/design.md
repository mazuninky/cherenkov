## Context

Cherenkov is a Rust-native, self-hosted, language-agnostic real-time
messaging server. The repository is empty save for the working brief at
`docs/plan.md` and OpenSpec scaffolding. Before any headline feature work
(CRDT channels, WebTransport, schema-aware validation) can begin, the
project needs a credible workspace skeleton with green CI plus a
minimum-lovable-product demo that proves the pluggable architecture from
`docs/plan.md` §2 actually composes end-to-end.

Stakeholders: the maintainer plus future open-source contributors. The
project is publicly visible from day one, so workspace hygiene and
documentation matter as much as the runtime code.

Constraints:

- Pre-`0.1.0`. The wire protocol is allowed to change later, but every
  change after this milestone must go through an ADR (`docs/plan.md` §2.5).
- MIT-licensed; `cargo deny` rejects GPL/AGPL/SSPL transitive deps
  (`docs/plan.md` §8.1).
- MSRV 1.83 (`docs/plan.md` §3.11).
- Core crate is forbidden from depending on concrete kinds, transports,
  brokers, schema libraries, or HTTP frameworks (`docs/plan.md` §2.2).
- No `unsafe`, no `unwrap()`/`expect()` in library code, no global mutable
  state, no payload logging (`docs/plan.md` §8).

## Goals / Non-Goals

**Goals:**

- A 14-crate workspace that compiles cleanly with `cargo build --workspace`.
- Six green CI gates on the first commit: `fmt`, `clippy`, `test`, `deny`,
  `audit`, `doc`.
- A working `v1` Protobuf wire protocol with round-trip and snapshot tests.
- Three core extension traits (`ChannelKind`, `Transport`, `Broker`) with no
  concrete-impl leakage into `cherenkov-core`.
- A `Hub` that routes subscribe/unsubscribe/publish through pluggable
  components, with a sharded `SessionRegistry` and a reverse channel index.
- One concrete implementation of each trait (`PubSubChannel`,
  `MemoryBroker`, `WsTransport`) sufficient to back a browser-to-browser
  echo demo.
- An end-to-end integration test in `cherenkov-server/tests/echo.rs` that
  proves WebSocket fan-out without external services.
- ADR `0001-pluggable-architecture.md` capturing the trait-based extension
  design.

**Non-Goals:**

- WebTransport or SSE transports (later milestones).
- Schema validation, AsyncAPI export, TS SDK generation (M2+).
- CRDT channel kinds — `yrs` and `automerge` are not pulled in (M4).
- Real auth, JWT verification, ACL, backend proxy events (M3+).
- Reconnect-time recovery / history replay on resume.
- Redis or NATS brokers (M6).
- Admin HTTP API or UI (M7).
- macOS / Windows CI matrix (deferred to a follow-up PR).
- Performance benchmarks against the §7 targets — we set the framework with
  `criterion` benches but do not commit to numbers at this milestone.

## Decisions

### D1. Bundle M0 and M1 into a single OpenSpec change

**Rationale:** M0 alone produces a repo that compiles nothing useful, and
M1 alone cannot land without M0's CI and lint guardrails. Reviewing them
together gives one coherent "first green build" story. The change is large
but most of it is mechanical workspace setup; the truly novel surface is
the seven specs in this change.

**Alternative considered:** Two separate changes (`workspace-bootstrap` and
`foundation-ws-pubsub`). Rejected because the workspace skeleton without a
working demo is unmotivating to review, and because the first round of CI
tuning surfaces during M1 work, not M0.

### D2. Wire protocol uses Protobuf via prost, with hand-written wrappers

**Rationale:** Protobuf gives us schema-aware tooling for free at later
milestones (AsyncAPI export, TS SDK generation, dynamic decode via
`prost-reflect`). Hand-written wrappers in `frame.rs` keep `prost`
generated types out of the public API so we can refactor codegen without
breaking downstream crates.

**Alternative considered:** `bincode` or `rmp-serde` for self-describing
binary frames. Rejected: locks us out of cross-language SDKs.

**Alternative considered:** Capnp. Rejected: ecosystem maturity and
familiarity for open-source contributors.

### D3. Hub holds Arc<dyn Trait>, not generic over the trait

**Rationale:** A single binary may host multiple concrete `ChannelKind`s
(pub/sub for one namespace, CRDT for another). Generics cannot express
that without a kind-tag enum. Trait objects do. Hot-path costs of dynamic
dispatch are bounded — fan-out is the dominant cost, not the trait method
indirection.

**Alternative considered:** Static enums of all known kinds. Rejected:
defeats the pluggable-architecture principle (`docs/plan.md` §2.1).

### D4. SessionRegistry uses sharded DashMap with reverse channel index

**Rationale:** Subscribe/unsubscribe and publish-fanout are both hot.
DashMap's per-shard locking keeps tail latency bounded. A reverse index
`channel → Vec<SessionId>` avoids scanning all sessions per publish; per
`docs/plan.md` §7 we explicitly avoid `Mutex<HashMap>` on hot paths.

**Alternative considered:** Single `RwLock<HashMap>`. Rejected on §7
grounds.

### D5. MemoryBroker uses tokio::sync::broadcast per topic, lazy-created

**Rationale:** `tokio::sync::broadcast` already implements bounded fan-out
with explicit `Lagged` semantics, so back pressure surfaces as a typed
error rather than as silent stalls. Lazy creation avoids paying for unused
topics in long-lived servers.

**Trade-off:** `broadcast` drops slow consumers as `Lagged`. We surface
this as a metric and accept the loss at M1 (acceptable for a non-recovery
demo).

### D6. WebSocket transport is built on axum + tokio-tungstenite, not raw hyper

**Rationale:** `axum` gives us routing, extractors, and a familiar middleware
story for the future admin API. Pinning to the workspace's existing
`tokio` and `tokio-tungstenite` versions avoids a parallel HTTP stack.

### D7. Configuration uses figment with YAML + env layering

**Rationale:** Per `docs/plan.md` §3.9. YAML for files, env for overrides
(`CHERENKOV_*`). figment composes the two cleanly and supports profiles
later.

### D8. CI runs Linux-only at this milestone

**Rationale:** macOS and Windows runners are slower and flakier; getting
the Linux gates green and stable first lets us add other platforms behind
a separate change without churning the initial setup.

## Risks / Trade-offs

- **Risk: workspace dependency drift across crates** → Mitigation: declare
  every shared dependency in `[workspace.dependencies]` and inherit via
  `dep.workspace = true` in member crates. CI's `cargo deny check bans`
  catches duplicates.

- **Risk: `prost-build` regenerates code on every clean build, slowing CI**
  → Mitigation: cache `target/` and `~/.cargo` in CI; the regen cost is
  small relative to the workspace compile time.

- **Risk: `tokio::sync::broadcast` `Lagged` errors look like data loss to
  reviewers** → Mitigation: document the back-pressure semantics in the
  `MemoryBroker` rustdoc and emit a `cherenkov_broker_dropped_total`
  metric so the loss is observable.

- **Risk: integration test flake on slow CI runners** → Mitigation: bind
  WebSocket clients to ephemeral ports, await readiness via a hub-emitted
  signal rather than `sleep`, and set a generous (but not unbounded)
  `tokio::time::timeout` per assertion.

- **Risk: scope creep into M2** → Mitigation: the `## Out of scope` list in
  the proposal is explicit; reviewers reject any code that imports
  `jsonschema`, `yrs`, `automerge`, `fred`, `async-nats`, or `wtransport`
  in this change.

- **Risk: ADR template drift** → Mitigation: capture the canonical template
  in `docs/plan.md` §5.2 and reference it from `0001-pluggable-architecture.md`.

## Migration Plan

There is nothing to migrate from — this is the founding change. Rollback
is `git revert` of the merge commit; no production state is mutated.

## Open Questions

- **Q1**: Does `Broker` belong in `cherenkov-core` or in its own
  `cherenkov-broker` crate? `docs/plan.md` §M1.6 leaves this to an ADR.
  Decision deferred to the implementation step; default lean is its own
  crate for symmetry with kinds and transports.
- **Q2**: What is the right default `tokio::sync::broadcast` capacity?
  Plan picks an arbitrary 1024; revisit once we have a benchmark.
- **Q3**: Do we ship `examples/echo/` JS as plain `<script>` or via a
  bundler? Plan picks plain `<script>` to avoid a Node toolchain in this
  change. Revisit when we add the TS SDK in M2+.
