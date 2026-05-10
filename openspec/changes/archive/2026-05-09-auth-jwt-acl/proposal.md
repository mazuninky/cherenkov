## Why

Phase 1 + 2 (`bootstrap-foundation`, `schema-validation`) shipped M1 pub/sub
and M2 schema enforcement, but every WebSocket session is still anonymous
and unrestricted. Anyone who can reach the listen socket can subscribe to or
publish on any channel. That is fine for the echo demo and not fine for
anything else. M3 closes the gap with token-based authentication and
per-namespace ACLs evaluated server-side before subscribe / publish reach
the channel kind.

## What Changes

- Extend the v1 wire protocol with a `Connect` request and `ConnectOk`
  response (oneof variants 4 and 5 respectively). Both directions remain
  backwards compatible — the variants are appended per `docs/plan.md` §2.5.
- Add three new wire `ErrorCode`s: `InvalidToken = 6`, `AclDenied = 7`,
  `NotConnected = 8`.
- Define two new extension traits in `cherenkov-core`:
  - `Authenticator`: validates a bearer token, returns `SessionClaims`.
  - `AclChecker`: gates subscribe / publish per session + channel + action.
- Default impls `AllowAllAuthenticator` and `AllowAllAcl` keep the M1 / M2
  echo behavior intact when the server is built without explicit auth.
- Hub gains `handle_connect`, stores per-session `SessionClaims`, and gates
  every `handle_subscribe` / `handle_publish` through the `AclChecker`.
  When a non-permissive `Authenticator` is configured, hub rejects all
  pre-connect frames with `HubError::NotConnected`.
- New crate body for `cherenkov-auth`:
  - `JwtAuthenticator` (HS256 default, RS256 opt-in via feature flag).
  - `NamespaceAcl` matcher: glob-style allow/deny rules per
    `(subject_pattern, action, channel_pattern)` triples.
- WebSocket transport handles the new `Connect` frame, responds with
  `ConnectOk` or an `Error` carrying the new error code, and refuses to
  forward anything else until the session has connected (when auth is
  enabled).
- Server config gains an optional `auth:` section (HMAC secret, JWT
  audiences, optional issuer) and an `acl:` rule list. Schemas remain
  per-namespace; ACL rules use the same namespace prefix granularity.
- Add ADR `0004-auth-and-acl.md` capturing the trait placement, why ACLs
  are evaluated in the hub (not the transport), and why we accept JWTs
  rather than session cookies.
- Integration test in `cherenkov-server/tests/auth.rs` asserts:
  - `Connect` with bad token returns `Error{code=InvalidToken}`.
  - `Subscribe` before `Connect` returns `Error{code=NotConnected}`.
  - `Publish` to a denied namespace returns `Error{code=AclDenied}`.
  - `Publish` to an allowed namespace round-trips as before.

## Capabilities

### New Capabilities

- `auth-trait`: `Authenticator`, `SessionClaims`, `AuthError`, plus
  `AllowAllAuthenticator` defaults in `cherenkov-core`.
- `acl-trait`: `AclChecker`, `AclAction`, `AclDecision`, `AclError`, plus
  `AllowAllAcl` defaults in `cherenkov-core`.
- `jwt-authenticator`: `JwtAuthenticator` in `cherenkov-auth` backed by the
  `jsonwebtoken` crate; HS256 today, RS256 reserved for follow-up.
- `namespace-acl`: `NamespaceAcl` in `cherenkov-auth` evaluating glob-style
  `(subject, action, channel)` rules.
- `server-auth-config`: `AuthConfig`, `AclConfig`, `AclRule` types in
  `cherenkov-server` with `deny_unknown_fields`.

### Modified Capabilities

- `wire-protocol`: appends `Connect` (request) and `ConnectOk` (response)
  variants and three new `ErrorCode`s.
- `hub-core`: `HubBuilder::with_authenticator` and `with_acl_checker`;
  `Hub::handle_connect`; per-session `SessionClaims` storage; ACL gates
  inside `handle_subscribe` / `handle_publish`.
- `ws-transport`: dispatches `Connect`, refuses pre-connect frames when
  auth is enabled, maps `HubError::Acl{,Denied,Auth,...}` to the correct
  wire `ErrorCode`.
- `server-binary`: composes `JwtAuthenticator` and `NamespaceAcl` from the
  new config sections; defaults remain anonymous-allow-all.

## Impact

- **Code**: implements `cherenkov-auth` (was a stub); new modules
  `cherenkov-core/src/{auth.rs,acl.rs}`; new server config sections; one
  new ADR; one new integration test.
- **Wire protocol**: appends two new oneof variants and three new error
  codes; no breaking change inside v1.
- **Dependencies**: adds `jsonwebtoken = "9"` and `globset = "0.4"` to
  workspace deps. No MSRV bump.
- **Backwards compat**: with no auth section in config, behavior matches
  M2 — every session is anonymous, every channel is permitted.
