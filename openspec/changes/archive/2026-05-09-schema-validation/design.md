## Context

Phase 1 (`bootstrap-foundation`) deliberately stubbed schema validation
to always-allow so the M1 echo demo could ship without a JSON validator
in core's dependency footprint. The schema-as-contract principle
(`docs/plan.md` §2.3) only becomes load-bearing once a publication can
actually be rejected for failing validation; until then, "schema-aware
everything" is a marketing line, not a feature.

Stakeholders: the maintainer and any future operator who wants to
deploy a Cherenkov instance with a contract their publishers must
honour. The change is publicly visible, so wire-protocol choices made
here are durable.

Constraints:

- `cherenkov-core` is forbidden from depending on `jsonschema`,
  `prost-reflect`, or any other concrete schema backend
  (`docs/plan.md` §2.2 / ADR 0001).
- Wire protocol stays inside v1 — no breaking change, only additive
  variants (`docs/plan.md` §2.5).
- Validator must be cancel-safe and not log payload bytes
  (`docs/plan.md` §4.4 / §8.7).
- MSRV may move only as a deliberate, documented bump
  (`bootstrap-foundation/tasks.md` Notes).

## Goals / Non-Goals

**Goals:**

- A `SchemaValidator` trait in `cherenkov-core` with no concrete-impl
  dependencies, plumbed through `Hub::handle_publish` so a rejection
  short-circuits the channel kind and the broker.
- A `JsonSchemaRegistry` in `cherenkov-schema` that validates payloads
  against per-namespace JSON Schema documents.
- Server YAML config that declares schemas inline or by file path, with
  schemas compiled eagerly at startup.
- A wire-protocol `ErrorCode::ValidationFailed` that lets clients tell
  validation rejections apart from server-internal failures.
- An end-to-end integration test that proves valid publish, invalid
  publish (Error frame), and opaque pass-through all work on a single
  client socket.
- ADR `0003-schema-as-contract.md` capturing the design decisions.

**Non-Goals:**

- AsyncAPI export from the registry (M2 follow-up).
- TypeScript SDK generation from the registry (M2 follow-up).
- Protobuf descriptor sets as a schema language (post-M2).
- Per-channel schema overrides inside a namespace (rejected — see ADR
  0003 alternatives).
- Hot-reload of namespace declarations (post-foundation).
- Multi-kind routing (one channel kind per `<namespace>.*` family).

## Decisions

### D1. Schemas are per-namespace, namespace = prefix-before-first-dot

**Rationale:** §2.3 forbids "schema-light" mixing inside a namespace.
Using the first `.`-delimited prefix as the namespace gives operators a
single coordinate to declare a contract against, follows the well-known
`<namespace>.<channel>` segmentation pattern in pub/sub systems, and is
trivially derivable on the client without registry round-trips.

**Alternative considered:** per-channel schemas. Rejected because
publishers who control the channel name can opt out by publishing to a
sibling channel without a registered schema.

### D2. Validator is a trait in `cherenkov-core`, impl lives in `cherenkov-schema`

**Rationale:** §2.2 forbids core depending on validator backends. A
trait keeps `jsonschema` (and its `icu_*` chain) out of every hub that
only carries opaque pubsub. Symmetrical to the `ChannelKind` /
`Transport` / `Broker` decomposition from ADR 0001.

**Alternative considered:** embed `Arc<JsonSchemaRegistry>` directly in
`Hub`. Rejected — see ADR 0003.

### D3. Validation happens in `Hub::handle_publish`, before the channel kind

**Rationale:** Putting the call in the hub means it cannot be
forgotten by a future channel-kind author, and rejected publications
never reach the broker so the wire-protocol Error code is the *only*
visible side-effect. Order is `validate → kind.on_publish → broker.publish`.

**Alternative considered:** validate inside the channel kind. Rejected —
see ADR 0003.

### D4. Default validator is `AllowAllValidator` so the M1 demo keeps working

**Rationale:** The echo demo broadcasts plain UTF-8 strings between
two iframes, which are not JSON. Forcing every hub to declare a schema
or break the M1 contract is hostile to existing users. Builder semantics
are: explicit `with_schema_validator(...)` overrides; otherwise the
hub gets a no-op validator and behaves exactly as it did at M1.

### D5. Error reasons are payload-free

**Rationale:** `docs/plan.md` §8.7 forbids logging or echoing user
payloads. The validator emits diagnostics that include schema paths
("/required/0", "/properties/qty/type") but never the offending bytes.
`serde_json::Error` carries position-only context.

### D6. Schemas compile eagerly at startup

**Rationale:** A malformed schema is an operator error; it should fail
the boot, not the first publish hours into a deployment. The builder's
`with_namespace` returns `Result<Self, RegistryError>` so the failure
surfaces at config-load time.

### D7. Wire protocol gets a new appended `ErrorCode` variant

**Rationale:** §2.5 allows additive changes inside v1 — appending a
variant to the existing `ErrorCode` enum is forward- and
backward-compatible (the `code` field is `uint32` on the wire). Older
clients that hard-code the four pre-existing codes will see "unknown
code 5" and fall through to a generic error path; new clients can
distinguish validation failures cleanly.

### D8. MSRV bump from 1.85 → 1.86 is acceptable

**Rationale:** `jsonschema 0.28` transitively requires `icu 2.x`, which
needs Rust 1.86. The same precedent applies as the M0 1.83 → 1.85
bump (documented in `bootstrap-foundation/tasks.md` Notes): we declare
the bump in `rust-toolchain.toml`, in `[workspace.package].rust-version`,
and in this design doc, and we capture it in the merge-commit message.

## Risks / Trade-offs

- **Risk: jsonschema's diagnostic output is verbose** → Mitigation: we
  format only the first error via `Display`, not the full
  `iter_errors`. Reviewers can read the message; clients should not
  pattern-match on it (the `ErrorCode` is the stable contract).

- **Risk: declaring a namespace schema breaks an existing channel
  inside it** → Mitigation: this is the correct behavior — the
  schema-as-contract principle says a namespace either is validated or
  is not. Operators are warned by ADR 0003 and the README quickstart.

- **Risk: `icu_*` MSRV bump propagates to consumers** → Mitigation: we
  document the 1.85 → 1.86 bump in this design and the merge-commit
  body, mirroring the earlier 1.83 → 1.85 precedent.

- **Risk: schema files referenced by `schema_path` create implicit
  filesystem dependencies in tests** → Mitigation: the integration test
  uses inline `schema:` only; `schema_path:` is exercised by unit tests
  with `tempfile`.

- **Risk: scope creep into AsyncAPI / TS SDK** → Mitigation: the
  proposal calls those out as follow-ups and the change adds no
  AsyncAPI- or TS-related files.

## Migration Plan

There is no migration: M1 hubs that built without
`with_schema_validator` continue to behave identically (default
validator is `AllowAllValidator`). Operators who want enforcement
declare `namespaces:` in YAML; the server builds a registry on next
boot. Rollback is `git revert` of the merge commit; no persistent
state is touched.

## Open Questions

- **Q1**: Should `JsonSchemaRegistry` cache the parsed `serde_json::Value`
  per publication, or accept the parse cost on each publish? Current
  answer: parse on each publish (simpler, no per-channel buffer); revisit
  with a benchmark when M2 lands AsyncAPI export.

- **Q2**: Should validation diagnostics include a stable JSON-pointer
  path that clients can render (e.g. `/properties/qty/minimum`)? Current
  answer: not in this change — `Error.message` is a free-form string.
  We will revisit when the TS SDK generator lands and clients have a
  reason to consume structured paths.

- **Q3**: Do we want a metric `cherenkov_schema_rejected_total{namespace=...}`?
  Current answer: yes, but in a follow-up that adds metrics holistically;
  the `metrics` crate is already a workspace dep.
