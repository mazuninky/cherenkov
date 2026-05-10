# ADR 0003: Schema validation is a per-namespace contract, not per-channel

## Status

Accepted, 2026-05-09.

## Context

`docs/plan.md` §2.3 ("Schema-as-contract") states that either a namespace
declares a schema and every publication is validated, or the namespace is
opaque and every publication passes through unvalidated. Mixing the two
inside a single namespace is forbidden, because a publisher who can opt
out of validation by choosing a sibling channel name can defeat the
contract.

M2 introduces the first concrete validator (`JsonSchemaRegistry` in
`cherenkov-schema`). Before wiring it into `Hub::handle_publish`, three
shape decisions had to be locked in:

1. The granularity of a schema declaration (per-channel vs per-namespace).
2. Where the validation hook fires in the publish pipeline.
3. How the core stays decoupled from any concrete validator backend
   (`docs/plan.md` §2.2).

## Decision

* **Schemas are declared per *namespace*, where the namespace is the part
  of the channel name before the first `.`.** `rooms.lobby` and
  `rooms.support` share the schema for `rooms`; channels with no `.`
  are treated as their own namespace.

* **Validation is a [`SchemaValidator`] trait in `cherenkov-core`.** The
  core depends only on `bytes` and `async-trait` for it; concrete
  implementations live in dedicated crates (`cherenkov-schema` for JSON
  Schema, future `cherenkov-schema-protobuf` for Protobuf descriptor sets).
  `Hub::handle_publish` calls `validator.validate(channel, &data)`
  *before* the channel kind sees the payload, so a rejected publication
  never advances the kind's offset or hits the broker.

* **Default builder behavior is "allow everything".** When the binary
  builds a `Hub` without `with_schema_validator`, the hub is wired with
  `AllowAllValidator`. That preserves the M1 echo demo's behavior and
  matches the schema-as-contract rule (no schema → opaque).

* **Validation failures surface as a typed `HubError::Schema` and a
  dedicated wire-protocol code (`ErrorCode::ValidationFailed = 5`).**
  Clients can therefore distinguish "the server is broken" from "this
  payload was malformed" without parsing error message text.

* **Reasons in `Error.message` are payload-free.** The validator returns
  schema diagnostics (path, expected type, …) but never the offending
  bytes. This honours `docs/plan.md` §8.7 — the protocol must not log or
  echo user payloads.

## Consequences

* Adding a new schema language (Protobuf, Avro, Cap'n Proto) is a new
  crate that implements `SchemaValidator`. The hub does not change.

* Channel-name conventions are now load-bearing. Tooling that creates
  channels (e.g. the future TS SDK generator) must round-trip the
  `<namespace>.<rest>` shape.

* Per-channel schema overrides are explicitly out of scope. If a
  deployment needs them, the answer is to split the channel into its own
  namespace; we will not accept a "but only this one channel" exception.

* Recovery and replay (post-foundation) will need to validate historical
  publications against the *current* schema, not the one in force when
  the publication was originally accepted. The validator API is async
  precisely so a future remote-registry implementation (load from a
  control plane) can plug in without rewriting the call site.

## Alternatives considered

### Per-channel schema declarations

Have each fully-qualified channel name carry its own schema. Rejected:
this defeats the §2.3 invariant — a publisher who controls channel
names can opt out of validation by publishing to an unregistered
sibling, and operators end up writing the same schema dozens of times.

### Validate inside the channel kind

`PubSubChannel::on_publish` already takes `data: Bytes` and returns a
`Publication`. We could call the validator from there. Rejected: every
new channel kind would have to remember to call the validator, the
validator dependency would creep into every kind crate, and
schema-as-contract would silently degrade if a kind author forgot.
Putting the call in `Hub` makes it impossible to bypass.

### Validate after broker fan-out (on the receiver side)

Rejected outright: it lets bad publications enter the broker, multiplies
the cost across N subscribers, and gives clients an inconsistent view
(some receive a malformed message, then later get a validation error on
the same payload). The whole point of the contract is server-side
enforcement.

### Embed the validator instance in `Hub` without a trait

Make `Hub` own `Arc<JsonSchemaRegistry>` directly. Rejected because it
forces every hub to pull in `jsonschema` (and its `icu_*` chain),
including hubs that only carry opaque pubsub. The trait keeps the core
clean and lets a deployment opt out by linking nothing.
