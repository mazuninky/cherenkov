# ADR 0004: Authentication and ACL

## Status

Accepted, 2026-05-09

## Context

Phases 1 and 2 shipped a working pub/sub server with schema validation, but
every WebSocket connection was anonymous and unrestricted. Anyone with
network access could subscribe to or publish on any channel. M3 closes
the gap: sessions authenticate themselves, and the hub enforces an ACL
before frames reach the channel kind or broker.

We considered three places to put authentication:

1. In the transport layer (HTTP `Authorization` header on the WebSocket
   upgrade request).
2. As a cross-cutting Tower middleware.
3. As an explicit `Connect` frame at the start of the wire protocol.

Real-time messaging servers in this space split between all three options
depending on their transport assumptions. We picked #3 because:

- WebTransport / SSE / future raw-TCP transports do not all share the
  HTTP upgrade affordance #1 relies on.
- A `Connect` frame keeps auth state explicit, debuggable on the wire,
  and cleanly tied to a `request_id` clients can correlate.
- It lets the same auth path drive both human users (with short-lived
  JWTs) and service-to-service callers without per-transport plumbing.

## Decision

- The wire protocol gains `Connect` (request) and `ConnectOk` (response)
  oneof variants, plus three new error codes (`InvalidToken=6`,
  `AclDenied=7`, `NotConnected=8`).
- `cherenkov-core` defines two new extension traits — `Authenticator` and
  `AclChecker` — alongside `ChannelKind`, `Transport`, `Broker`, and
  `SchemaValidator`. Default no-op impls (`AllowAllAuthenticator`,
  `AllowAllAcl`) keep the M1 / M2 echo demo intact.
- `Hub::handle_connect` validates the token, stores `SessionClaims` on
  the session, and short-circuits subsequent `Subscribe` / `Publish`
  through the ACL checker. When a non-permissive `Authenticator` is
  configured, pre-connect frames fail with `HubError::NotConnected`.
- `cherenkov-auth` ships `JwtAuthenticator` (HS256 default) and
  `NamespaceAcl` (glob-matching `(subject, action, channel)` rules).
- ACL evaluation lives in the hub, not the transport. Every transport
  shares the same enforcement path, so we cannot accidentally grant
  WebSocket clients more access than WebTransport clients.

## Consequences

- **API surface grows**: two new traits, two new builder methods,
  `HubError::Auth/Acl/NotConnected/AlreadyConnected`, two new wire
  variants, three new error codes. All additive within v1.
- **MSRV bump 1.86 → 1.88** to satisfy `time 0.3.47` (transitive via
  `jsonwebtoken → simple_asn1 → time`). Documented in the merge commit
  alongside the change.
- **Backwards compatible defaults**: server config without an `auth:` or
  `acl:` section behaves like M2 — anonymous sessions, every action
  permitted.
- **Future asymmetric keys** are reserved for a follow-up change; the
  `JwtAlgorithm` enum already lists `Hs384` / `Hs512` so the migration
  path is mechanical.

## Alternatives considered

- **`Authorization` header at WebSocket upgrade.** Rejected because it
  doesn't generalize to WebTransport / SSE / raw-TCP transports without
  duplicate plumbing.
- **ACL evaluated by the channel kind.** Rejected because each channel
  kind would need to repeat the same boilerplate; CRDT and pub/sub
  must enforce the same security policy.
- **Re-authentication via second `Connect`.** Rejected for v1: it
  introduces race conditions with in-flight forwarders. A future ADR
  may revisit if needed.
