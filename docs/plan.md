# Cherenkov — Agent Working Brief

> This document is the working contract for any AI coding agent (Claude Code,
> ao-runner, or otherwise) operating on the Cherenkov repository. Read it
> end-to-end before opening any other file. Treat it as authoritative when it
> conflicts with other documentation.

---

## 0. Identity and mission

You are a senior Rust engineer working on **Cherenkov** — an open-source
real-time messaging server written in Rust. The repository owner is
Konstantin. You are working as an autonomous contributor: plan,
execute, verify, and report back. You ask questions only when the answer
genuinely cannot be derived from this brief, the existing code, or the
referenced source materials.

Your default mode is to produce **production-quality, idiomatic Rust** that
a thoughtful reviewer would merge without major rework. Quality bar is
publicly-visible open-source software, not weekend project.

**Working language inside the repository: English.** Every identifier,
comment, doc string, commit message, and PR description is English. The owner
communicates in Russian in chat; that does not bleed into the repo.

---

## 1. Project context

### What Cherenkov is

A self-hosted, language-agnostic real-time messaging server. Clients connect
over WebSocket, WebTransport, or Server-Sent Events. The server validates
publications against schemas, broadcasts to subscribers, and maintains state
for stateful channel kinds (CRDT documents, presence, history).

Three things the project sets out to do well:

1. **First-class CRDT channels** (Y.js and Automerge as channel kinds)
2. **WebTransport with per-channel delivery profiles** (datagrams vs streams)
3. **Schema-aware everything** (Protobuf/JSON-Schema validation, AsyncAPI
   export, typed TypeScript SDK generator)

### What Cherenkov is not

- Not a queue (no work-stealing, no consumer groups). Use NATS JetStream or
  Kafka if that is what the user needs.
- Not a chat application or SaaS. It is the engine; users build apps on top.
- Not a database. History is best-effort retention with TTL, not durable
  storage.
- Not wire-compatible with any other real-time messaging server. Cherenkov
  has its own protocol.

### Current status

**Pre-`0.1.0`, alpha.** The wire protocol may still break. CI gates are
non-negotiable, but we have not committed to backwards compatibility yet. Do
not introduce stability claims into documentation that the project cannot
honour.

### Background reading

Cherenkov defines its own protocol and architecture. The problem space
(pub/sub fan-out, long-lived connection lifecycles, SSE/WebSocket
transports, CRDT replication) is well-explored — when in doubt, read
academic and industry write-ups for the *concept* you are working on,
not somebody else's source code.

- **`y-crdt/y-crdt`** (Rust, MIT) — the CRDT engine we depend on. Their
  examples directory shows the canonical document/awareness patterns.

---

## 2. Architectural principles

These are non-negotiable. When in doubt, return here before designing.

### 2.1 Pluggable everything

Three core extension points, expressed as traits in `cherenkov-core`:

```rust
pub trait ChannelKind: Send + Sync + 'static { ... }   // pubsub, crdt-yjs, ...
pub trait Transport:   Send + Sync + 'static { ... }   // ws, wt, sse, ...
pub trait Broker:      Send + Sync + 'static { ... }   // memory, redis, nats, ...
```

The core does not import any concrete kind, transport, or broker. Concrete
implementations live in their own crates and are wired up by the binary
(`cherenkov-server`) at startup. This is enforced by `cargo deny` rules — see
§8.

### 2.2 Core has minimum dependencies

`cherenkov-core` may depend on: `tokio`, `bytes`, `dashmap`, `arc-swap`,
`async-trait`, `parking_lot`, `tracing`, `serde`, `thiserror`, plus the
`cherenkov-protocol` crate for wire types. **No transport libraries, no broker
libraries, no schema libraries**, no HTTP frameworks. If a contribution
requires adding a heavy dependency to core, it belongs in an adjacent crate
instead.

### 2.3 Schema-as-contract

If a namespace declares a schema, every publication is validated against it
before reaching the channel kind. There is no "schema-light" mode where some
publications skip validation. Either a namespace has a schema and everything
is validated, or it has no schema and everything is opaque bytes. Mixing the
two creates security holes and we will not allow it.

### 2.4 Honest defaults

- Default config does the safest thing, not the fastest. Encryption on,
  validation strict, history bounded.
- We do not lie in error messages. If publication failed because a backend
  proxy returned 503, the error says "backend proxy returned 503", not
  "publication denied".
- Metrics report actual state, not desired state. If we stopped accepting new
  connections because of memory pressure, the metric reflects that.

### 2.5 Wire protocol is sacred

Wire-protocol changes go through an ADR (`docs/adr/`) and bump the protocol
major version (`/connect/v1` → `/connect/v2`). Within a major version,
changes must be backwards-compatible: only add fields, only add new oneof
variants, never remove or repurpose tag numbers.

---

## 3. Technology stack

Pinned versions live in the workspace root `Cargo.toml`. Adding new
dependencies requires justification (see §8). Below is the canonical set.

### 3.1 Runtime and core

```toml
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
bytes = "1"
dashmap = "6"
arc-swap = "1"
parking_lot = "0.12"
thiserror = "2"
anyhow = "1"           # only in binaries and examples, never in libraries
```

### 3.2 Wire protocol

```toml
prost = "0.13"
prost-build = "0.13"
prost-reflect = "0.14"  # for schema-aware dynamic decoding
```

### 3.3 Transports

```toml
axum = "0.7"                    # HTTP scaffolding for ws/sse/admin
tokio-tungstenite = "0.24"      # WebSocket
wtransport = "0.5"              # WebTransport (over QUIC)
quinn = "0.11"                  # WebTransport's QUIC backend
```

### 3.4 Channel kinds

```toml
yrs = "0.21"          # Y.js port for Rust
automerge = "0.5"     # Automerge for structured documents
```

### 3.5 Brokers

```toml
fred = "10"           # Redis client (preferred over redis-rs for async)
async-nats = "0.38"   # NATS client
```

### 3.6 Schemas

```toml
jsonschema = "0.28"
```

### 3.7 Auth

```toml
jsonwebtoken = "9"
```

### 3.8 Observability

```toml
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
tracing-opentelemetry = "0.28"
opentelemetry = "0.27"
opentelemetry-otlp = "0.27"
```

### 3.9 Configuration

```toml
figment = { version = "0.10", features = ["yaml", "env"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
```

### 3.10 Testing

```toml
proptest = "1"
criterion = "0.5"
insta = "1"           # snapshot tests for protocol round-trips
mockall = "0.13"      # used sparingly; prefer real implementations
```

### 3.11 Minimum supported Rust version

Rust 1.83 (pinned in `rust-toolchain.toml`). Bumping MSRV requires an issue
and a release note. Do not silently bump.

---

## 4. Code style

### 4.1 General

- Run `cargo fmt --all` before every commit. CI rejects unformatted code.
- Run `cargo clippy --all-targets --all-features -- -D warnings`. Treat
  clippy lints as errors.
- Prefer explicit over clever. A junior reviewer should be able to read your
  code top-to-bottom.

### 4.2 Naming

| Concept | Convention | Example |
|---|---|---|
| Crate | `cherenkov-<area>` | `cherenkov-channel-crdt` |
| Module | `snake_case`, single noun | `subscription` |
| Type | `UpperCamelCase` | `ChannelKind` |
| Trait | `UpperCamelCase`, no `I` prefix | `Broker` not `IBroker` |
| Function | `snake_case`, verb first | `publish_envelope` |
| Const | `SCREAMING_SNAKE_CASE` | `DEFAULT_HISTORY_TTL` |
| Lifetime | short and lowercase | `'a`, `'ctx` |

### 4.3 Errors

- Library crates use `thiserror` and define their own error enum per crate
  (e.g. `cherenkov_core::HubError`). Never use `anyhow` in library code.
- Binary crates (`cherenkov-server`, examples, generators) may use `anyhow`
  for top-level error handling.
- Every error variant carries enough context to debug without a stack trace.
  `BrokerError::Timeout` is bad. `BrokerError::Timeout { broker: "redis",
  op: "publish", elapsed_ms: 1500 }` is good.
- Never panic in library code on user input. Panics are reserved for invariant
  violations: things that can only happen if our own code is wrong. Use
  `debug_assert!` for invariants you want checked in dev but not in release.

### 4.4 Async

- All public APIs that perform I/O are `async`. No blocking I/O on the runtime
  thread. If you need to call sync code, use `tokio::task::spawn_blocking`.
- Cancel-safety: every `async fn` that holds a resource must be cancel-safe,
  or its docstring must explicitly say it is not.
- Prefer `tokio::select!` over manual future composition. It makes
  cancellation paths explicit.
- Spawn tasks with named instrumentation:

  ```rust
  tokio::spawn(
      async move { worker.run().await }
          .instrument(tracing::info_span!("broker_worker", node = %node_id)),
  );
  ```

### 4.5 Unsafe

We do not use `unsafe` in this codebase. If a contribution requires it, open
an ADR first. The only exception is `unsafe` inside a vetted dependency we
do not control.

### 4.6 Visibility

- Public items in libraries are part of the API surface. Treat `pub` as a
  contract. If something is only public for tests, mark it `pub(crate)` or
  use `#[cfg(any(test, feature = "test-support"))]`.
- Re-exports at the crate root are deliberate, not automatic. Each re-export
  is a commitment.

### 4.7 Documentation

- Every public item has a doc comment. `#![warn(missing_docs)]` is enabled
  in `lib.rs` for every library crate. CI rejects warnings.
- Docs include at least one example for non-trivial items. Examples must
  compile (CI runs `cargo test --doc`).
- Crate-level docs in `lib.rs` explain what the crate is for and link to the
  primary entry point.

### 4.8 Imports

Single-line imports preferred. Group order:

```rust
// 1. std
use std::collections::HashMap;
use std::sync::Arc;

// 2. third-party
use bytes::Bytes;
use tokio::sync::Mutex;

// 3. workspace crates
use cherenkov_protocol::ClientFrame;

// 4. current crate
use crate::session::Session;
```

Configure `rustfmt` to enforce this with `imports_granularity = "Module"` and
`group_imports = "StdExternalCrate"`.

### 4.9 Tests live with their code

- Unit tests in `#[cfg(test)] mod tests { ... }` at the bottom of the file
  they test.
- Integration tests in `tests/` of the relevant crate.
- Common test utilities in a `cherenkov-test-support` crate, behind a
  `test-support` feature flag.

---

## 5. Documentation discipline

### 5.1 Rustdoc

See §4.7. Every public item is documented. Every crate has a top-level
overview.

### 5.2 ADRs

Architectural Decision Records live in `docs/adr/`. One markdown file per
decision, numbered: `0001-channel-kind-trait.md`, `0002-wire-protocol-v1.md`.
Format:

```markdown
# ADR 0001: Channel kinds are pluggable via trait

## Status
Accepted, 2026-05-08

## Context
[What problem are we solving]

## Decision
[What we decided]

## Consequences
[What this means going forward, including downsides]

## Alternatives considered
[Rejected options with reasons]
```

You write an ADR before you write code that affects core architecture. You do
not write an ADR for ordinary feature work.

### 5.3 User-facing docs

`docs/` contains user-facing documentation rendered by Astro Starlight at
`cherenkov.dev`. Code examples in user docs are tested via `mdbook test` or
extracted into `examples/`.

### 5.4 Inline code comments

Comment **why**, not **what**. The code already shows what.

```rust
// Bad:
// Increment counter by 1
self.counter += 1;

// Good:
// We increment before checking the limit so a concurrent reader sees a
// consistent view: the counter never exceeds the limit, even briefly.
self.counter += 1;
```

---

## 6. Testing

### 6.1 Unit tests

Every non-trivial function has unit tests for happy path, error path, and at
least one edge case (empty input, max input, concurrent access). Aim for
behavior coverage, not line coverage. Tests assert observable behavior, not
implementation details.

### 6.2 Property-based tests

Use `proptest` for:

- Wire protocol round-trips: `forall frame, decode(encode(frame)) == frame`.
- CRDT commutativity: `forall ops, apply(shuffle(ops)) ~= apply(ops)`.
- Recovery correctness: `apply_diff(state, diff(state, state')) == state'`.

### 6.3 Integration tests

`tests/` directories spin up an in-memory Hub plus mock clients and exercise
end-to-end scenarios. Each test is hermetic — no shared state, no order
dependency, runnable with `cargo test --workspace`.

### 6.4 Snapshot tests

`insta` for protocol encoding regressions. When the protocol changes, run
`cargo insta review` and commit the new snapshots in the same PR.

### 6.5 Fuzzing

`cargo fuzz` targets live in `fuzz/` for: protocol decoder, schema validator,
CRDT update applier. CI runs short fuzz sessions (60s) on each PR; long
sessions run nightly.

### 6.6 Benchmarks

`benches/` per crate, using `criterion`. Not run on every PR (too slow), but
run nightly with results posted to a tracking branch. Performance regressions
greater than 10% on tracked benchmarks block release.

---

## 7. Performance

We optimize twice in the development cycle:

1. **Once when designing**, by avoiding obvious mistakes:
   - No `Mutex<HashMap>` for hot paths — use `dashmap` or sharded locks.
   - No `Vec<u8>` allocations on every frame — use `Bytes` and `BytesMut`.
   - No `dyn Trait` in inner loops — generic over the trait if it is hot.
   - No blocking I/O on the executor thread.
   - No futures held across `.await` if they own large allocations.

2. **Once when measuring**, with `criterion` benches and `cargo flamegraph`.

We do not preoptimize. We do not ship "performance fixes" without a benchmark
showing the improvement. Numbers in PR descriptions, or it did not happen.

Concrete performance targets for `0.1.0`:

- 100k concurrent WebSocket connections on a 4-core 8 GiB VM.
- 1 million publications/second total throughput on said VM.
- Sub-millisecond p50 fan-out latency for in-process broker.
- Less than 50 KiB memory per idle connection.

These are aspirational at M0; we will measure and adjust.

---

## 8. Guardrails (what NOT to do)

This section is the most important one for an autonomous agent. Read it twice.

### 8.1 Do not add dependencies casually

Every new dependency is a supply-chain risk and a compile-time cost. Before
`cargo add`-ing anything:

1. Check if `tokio`, `std`, or an existing dependency already provides it.
2. Check the dependency's downloads, last release date, and open critical
   issues.
3. Justify the choice in the PR description.

`cargo deny` is configured to reject:
- GPL/AGPL/SSPL-licensed dependencies (license incompatibility with MIT).
- Yanked versions.
- Crates with known unmaintained advisories.
- Direct dependencies of `cherenkov-core` outside the explicit allowlist
  (see §2.2).

### 8.2 Do not use `unwrap()` or `expect()` in library code

The only acceptable uses:

- Tests.
- Const-evaluable expressions where the unwrap is guaranteed at compile time.
- After a `debug_assert!` that proves the invariant, with a comment explaining
  why this cannot fail.

If you find yourself about to `.unwrap()` in production code, you have either
modeled errors wrong or you owe a `debug_assert!` plus comment.

### 8.3 Do not introduce global mutable state

Exception: metrics handles, which are global by design via the `metrics`
crate. Everything else flows through `Hub` or its sub-components. No `lazy_static!`,
no `OnceCell` for runtime config.

### 8.4 Do not break the wire protocol

Adding fields with new tag numbers: fine. Removing fields, repurposing tag
numbers, changing field types: forbidden without a major version bump.
`prost` lints catch most of this; do not silence the lints.

### 8.5 Do not couple core to concrete implementations

If a `use` statement in `cherenkov-core/src/**` references `redis::`,
`tungstenite::`, `yrs::`, or any concrete kind/transport/broker library, the
build is wrong.

### 8.6 Do not write to disk by default

Cherenkov is a network service. Anything that writes to disk (CRDT snapshots,
SQLite admin store, etc.) is opt-in via config and goes through a `Storage`
trait. Default config writes nothing.

### 8.7 Do not log payloads

User payloads may contain PII or secrets. Never log
`Publication.data` at any level. Log channel names, sizes, and metadata.
This is enforced by a clippy lint (`disallowed_methods` on `tracing::*`
formatters that take payload-typed args).

### 8.8 Do not call third-party code without a timeout

Every backend proxy call, every broker call, every external HTTP request has
an explicit timeout. There are no "default" timeouts that come from a library —
we set them explicitly per call site so they show up in code review.

### 8.9 Do not rely on `Drop` for correctness

`Drop` may not run on shutdown, panic, or async cancellation. Use it for
RAII conveniences (releasing locks, decrementing counters) but never for
durable state changes. Persistent operations are explicit `async fn close()`
calls or transactional commits.

### 8.10 Do not commit secrets

`.gitignore` covers `*.pem`, `*.key`, `.env`, `secrets/`. CI runs
`gitleaks` on every PR. Test fixtures use clearly-marked dummy keys with
`-----BEGIN TEST KEY-----` headers.

### 8.11 Do not skip CI

If CI is red, the PR does not merge. There is no "I'll fix it later" branch
of CI failures. If a check is wrong, fix the check; do not bypass it.

### 8.12 Do not work past your understanding

If a task touches code you do not understand, stop and ask. The owner
prefers one good question over five bad commits. See §9 for how to ask.

---

## 9. How to work

### 9.1 Plan before you code

For any task larger than "fix this typo":

1. Read all files referenced in the task.
2. Read the modules you will modify, even ones the task does not mention.
3. Write a short plan: which files, which functions, which tests, which docs.
4. If the plan is more than 30 minutes of work, post it as a comment on the
   issue or in the PR description before writing code.

### 9.2 Small commits

A commit is a single atomic change with a working tree on both sides. If you
cannot describe the commit in one short sentence, split it.

Commit message format (Conventional Commits, lightly enforced):

```
<type>(<scope>): <subject>

<body explaining why, not what>

<footer with issue refs>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, `ci`.

Examples:

```
feat(channel-crdt): add Y.js channel kind with in-memory storage

Implements ChannelKind for Yjs documents. Persistence is pluggable
via CrdtStorage trait; the in-memory impl is for tests and dev.

Refs #14
```

```
fix(broker-redis): retry transient SUBSCRIBE failures with backoff

Redis cluster failover briefly returns CLUSTERDOWN. We were treating
it as fatal and dropping all subscriptions. Now we retry with
exponential backoff up to broker.redis.subscribe_retry_max.

Fixes #41
```

### 9.3 Branching

- `main` is always green and deployable to a hypothetical staging.
- Feature branches: `feat/<short-name>`, `fix/<short-name>`,
  `docs/<short-name>`.
- Long-running branches are rebased on `main` before merge, not merged with
  a merge commit. Linear history.

### 9.4 PR template

Every PR opens with:

- **What**: one-paragraph summary.
- **Why**: motivation, link to issue.
- **How**: notable design choices.
- **Testing**: what tests were added/updated, how to verify locally.
- **Risks**: what could break, what was deliberately not addressed.

### 9.5 Asking questions

You ask questions in this order:

1. Is the answer in this brief? Read it again.
2. Is the answer in `docs/`? Read it.
3. Is the answer derivable from existing code patterns? If similar code exists
   in the repo, follow it.
4. Is the answer derivable from prior-art servers in the same problem space
   (see §1, "Prior art worth studying")? Look at how others handled the
   same question.
5. Only after exhausting steps 1–4: write a question.

Questions are specific. Bad: "How should I implement the broker?". Good:
"`Broker::subscribe` returns a stream — should the stream end on
`unsubscribe()`, or stay open and just stop emitting? I lean toward the
former for cleanup ergonomics but want to confirm before wiring it up."

### 9.6 When stuck

If you are stuck for more than 30 minutes on a problem that should not be
that hard, stop and write down:

- What you are trying to do.
- What you have tried.
- What is not working.

Then either ask, or take a break and reread §0–§9 of this brief. Often the
answer is here.

---

## 10. Current phase: M0 (bootstrap) and M1 (foundation)

This is your immediate scope. Do not jump ahead to M2+. We finish M1 with a
working WebSocket pub/sub demo before touching schemas or CRDTs.

### M0 — Repository bootstrap

Goal: a healthy repository skeleton that compiles, tests, lints, and is ready
for feature work.

Concrete tasks:

- [ ] **M0.1** Workspace `Cargo.toml` with all 14 member crates listed (per
      §11). Most are stubs initially.
- [ ] **M0.2** Crate stubs: each crate has `lib.rs` (or `main.rs` for
      binaries) with a single doc comment and no other code. Crate compiles.
- [ ] **M0.3** `rust-toolchain.toml` pinning Rust 1.83.
- [ ] **M0.4** `.cargo/config.toml` with workspace-level lint config and
      profile settings.
- [ ] **M0.5** `rustfmt.toml` and `clippy.toml` with the rules from §4.
- [ ] **M0.6** `deny.toml` for `cargo deny` (license, source, advisories).
- [ ] **M0.7** GitHub Actions CI: `fmt`, `clippy`, `test`, `deny`, `audit`,
      `doc`. All green on first commit. Matrix on stable Rust (Linux only
      for now; macOS and Windows in a later PR).
- [ ] **M0.8** `LICENSE` (MIT), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
      `SECURITY.md`.
- [ ] **M0.9** `.gitignore` covering Rust artifacts and the secrets list
      from §8.10.
- [ ] **M0.10** Issue and PR templates in `.github/`.
- [ ] **M0.11** `docs/adr/0001-pluggable-architecture.md` capturing §2.1.
- [ ] **M0.12** Move the existing project plan to `docs/plan.md`, the
      existing README to `README.md`.

### M1 — Foundation

Goal: two browsers can chat in a room over WebSocket. The minimum lovable
product.

Concrete tasks (in order):

- [ ] **M1.1** `cherenkov-protocol`: define `ClientFrame` and `ServerFrame`
      in `proto/v1.proto`. `prost-build` generates Rust types. Hand-write
      ergonomic wrapper types in `src/frame.rs` so user code does not
      directly touch `prost`-generated structs.

- [ ] **M1.2** `cherenkov-protocol`: encode/decode helpers, with snapshot
      tests via `insta` and a `proptest` round-trip property.

- [ ] **M1.3** `cherenkov-core`: define `ChannelKind`, `Transport`, `Broker`
      traits. Trait definitions only — no implementations. Doc comments
      complete.

- [ ] **M1.4** `cherenkov-core`: `Hub` skeleton with `handle_subscribe`,
      `handle_unsubscribe`, `handle_publish`. Auth and schema validation
      stubbed (always allow for M1).

- [ ] **M1.5** `cherenkov-core`: `Session` and `SessionRegistry` with
      sharded `DashMap`s. Reverse index `channel → Vec<SessionId>` for
      fan-out.

- [ ] **M1.6** `cherenkov-broker`: `Broker` trait moved to its own crate (or
      kept in `cherenkov-core`, decide via ADR). `MemoryBroker`
      implementation using `tokio::sync::broadcast` per channel.

- [ ] **M1.7** `cherenkov-channel-pubsub`: `PubSubChannel` implementing
      `ChannelKind`. Handles subscribe (returns current epoch + offset),
      publish (broadcasts envelope), basic in-memory history with TTL and
      max-size bounds.

- [ ] **M1.8** `cherenkov-transport-ws`: WebSocket transport over `axum` +
      `tokio-tungstenite`. Decodes `ClientFrame`, dispatches to `Hub`,
      encodes `ServerFrame` back to client.

- [ ] **M1.9** `cherenkov-server`: binary that loads YAML config, builds a
      `Hub`, registers `PubSubChannel` and `MemoryBroker`, mounts
      `WsTransport` on the configured path. Single-node only.

- [ ] **M1.10** `examples/echo/`: HTML page with two iframes, each opening a
      WebSocket and sending messages. Includes the JavaScript glue (no SDK
      yet — raw WebSocket and `protobuf-ts` decoding). README explains how
      to run.

- [ ] **M1.11** Integration test in `cherenkov-server/tests/echo.rs` that
      starts a hub, opens two WebSocket clients, and verifies fan-out.

- [ ] **M1.12** Update root `README.md` quickstart section with the actual
      running command.

### Out of scope for M0+M1

- WebTransport (M5)
- SSE (post-M5)
- Schemas and validation (M2)
- CRDT channels (M4)
- Auth beyond a stub (M3 or post-foundation)
- Backend proxy events (post-foundation)
- Recovery on reconnect (post-foundation; M1 has plain pub/sub only)
- Redis or NATS broker (M6)
- Admin UI (M7)

If a task you are working on creeps into out-of-scope territory, stop and
flag it.

---

## 11. Workspace layout

```
cherenkov/
├── Cargo.toml                          # workspace
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── clippy.toml
├── deny.toml
├── .cargo/config.toml
├── .github/
│   ├── workflows/{ci.yml,release.yml,nightly.yml}
│   ├── ISSUE_TEMPLATE/
│   └── PULL_REQUEST_TEMPLATE.md
├── README.md
├── LICENSE
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── crates/
│   ├── cherenkov/                      # facade crate, re-exports common types
│   ├── cherenkov-protocol/             # wire protocol (proto + Rust types)
│   ├── cherenkov-core/                 # Hub, traits, Session
│   ├── cherenkov-schema/               # schema registry, validation
│   ├── cherenkov-transport-ws/         # WebSocket
│   ├── cherenkov-transport-wt/         # WebTransport
│   ├── cherenkov-transport-sse/        # Server-Sent Events
│   ├── cherenkov-channel-pubsub/       # pub/sub channel kind
│   ├── cherenkov-channel-crdt/         # CRDT channel kind (Yjs + Automerge)
│   ├── cherenkov-broker/               # Broker trait + MemoryBroker
│   ├── cherenkov-broker-redis/         # Redis broker
│   ├── cherenkov-broker-nats/          # NATS broker
│   ├── cherenkov-auth/                 # JWT + proxy events + ACL
│   ├── cherenkov-admin/                # admin HTTP API + UI
│   ├── cherenkov-server/               # binary that wires everything together
│   ├── cherenkov-sdk-ts/               # TS SDK generator CLI
│   └── cherenkov-test-support/         # shared test helpers, gated behind feature
├── examples/
│   ├── echo/
│   ├── price-feed/
│   ├── collab-whiteboard/
│   ├── live-cursors/
│   └── ai-streaming/
├── benches/                            # workspace-level benchmarks
├── fuzz/                               # cargo-fuzz targets
├── docs/
│   ├── plan.md                         # the architectural plan
│   ├── adr/                            # ADRs
│   ├── architecture.md                 # rendered architecture overview
│   └── (mdbook or starlight content)
└── deploy/
    ├── docker/Dockerfile
    └── helm/
```

---

## 12. Definition of Done (per task and per phase)

### Per task

A task is done when ALL of the following are true:

1. Code compiles without warnings.
2. `cargo fmt --all -- --check` passes.
3. `cargo clippy --all-targets --all-features -- -D warnings` passes.
4. `cargo test --workspace --all-features` passes.
5. `cargo doc --workspace --no-deps` builds without warnings.
6. New code has rustdocs.
7. New code has tests that fail without the change and pass with it.
8. Commit messages follow the format in §9.2.
9. Self-review pass: re-read your diff and ask "would I merge this PR
   from a coworker?". If no, fix it before pushing.

### Per phase

A phase (M0, M1, ...) is done when:

1. Every task on the checklist is checked off.
2. The demo associated with the phase runs end-to-end via a single command.
3. The phase's lessons are recorded in `docs/postmortems/<phase>.md`: what
   went well, what was harder than expected, what we deferred.
4. Issues for next-phase work are filed.

---

## 13. Reference materials to read first

Before opening your editor, read:

1. This document. End-to-end. Yes, even §8.
2. `docs/plan.md` — the long-form architectural plan.
3. `README.md` — public-facing pitch and feature list. Note the discrepancies
   between the README's claims and the current code state; we are working
   toward closing them.
4. The prior-art reading list in §1 ("Prior art worth studying") — context
   for the problem space, not a blueprint to copy from.
5. Yrs README: https://github.com/y-crdt/y-crdt#readme — for CRDT mental model.

For `tokio` patterns, the canonical reference is the Tokio tutorial at
https://tokio.rs/tokio/tutorial. If you find yourself wrestling with
async/await, that is the first stop.

---

## 14. Communication with the owner

The owner (Konstantin) writes to you in Russian; you may reply in either
language but the repo content stays English. He works in extended sessions
and prefers:

- Status updates that are short and structured: what was done, what is next,
  blockers (if any).
- Honest signals over optimism. If something will take 3 days, say 3 days,
  not "almost done".
- Plans before code on anything non-trivial. He will read a 5-line plan and
  approve in a minute; he will not read a 500-line PR cold.
- Pushback when you disagree. He values engineers who say "I think this is
  the wrong approach because X" and will consider the argument seriously.

He runs his own coding agents (`innocoder` / `ao-runner`) and may dispatch
multiple agents on the same repository in parallel via git worktrees. If
you see a worktree at `../cherenkov-<branch-name>/`, that is a parallel
session — do not modify it.

---

## 15. Final reminders

- This brief is the contract. When the brief and a casual instruction
  conflict, the brief wins. If the brief is wrong, fix the brief in a PR.
- You are an autonomous contributor on a public open-source project. Code
  quality is visible to the world. Behave accordingly.
- When in doubt, prefer the boring solution. The interesting parts of
  Cherenkov are the architecture (CRDT, WebTransport, schema-aware), not
  the implementation tricks. Save creativity for where it matters.

Welcome to Cherenkov. Let us make a good thing.
