# hub-core Specification

## Purpose
TBD - created by archiving change bootstrap-foundation. Update Purpose after archive.
## Requirements
### Requirement: Core defines three pluggable extension traits
`cherenkov-core` SHALL define the traits `ChannelKind`, `Transport`, and
`Broker`, all `Send + Sync + 'static`. The crate SHALL NOT import any
concrete kind, transport, or broker implementation.

#### Scenario: Core has no concrete-impl dependencies
- **WHEN** a reviewer runs `cargo tree -p cherenkov-core`
- **THEN** the output does not list `redis`, `tokio-tungstenite`, `wtransport`,
  `yrs`, `automerge`, or any other concrete kind/transport/broker library

#### Scenario: Trait objects are usable through Arc<dyn Trait>
- **WHEN** a binary constructs `Arc<dyn ChannelKind>`,
  `Arc<dyn Transport>`, and `Arc<dyn Broker>`
- **THEN** the binary compiles, because each trait is object-safe

### Requirement: Hub provides subscribe, unsubscribe, and publish entry points
The crate SHALL expose a `Hub` type with `async fn handle_subscribe`,
`async fn handle_unsubscribe`, and `async fn handle_publish` that dispatch
to the registered `ChannelKind` for the target channel and update the
`SessionRegistry` accordingly. Auth and schema validation SHALL be stubbed
to always-allow at this milestone.

#### Scenario: Subscribe registers the session for fan-out
- **WHEN** a session calls `handle_subscribe(channel)`
- **THEN** subsequent `handle_publish(channel, payload)` deliveries reach
  that session via the channel kind's `on_publication` path

#### Scenario: Unsubscribe removes the session from fan-out
- **WHEN** a session calls `handle_unsubscribe(channel)` and then
  `handle_publish(channel, payload)` runs
- **THEN** the session does not receive the publication

### Requirement: SessionRegistry maintains sharded reverse index
The crate SHALL provide `Session` and `SessionRegistry` types backed by
sharded `DashMap`s, with a reverse index `channel → Vec<SessionId>` so that
fan-out is O(subscribers) and lock contention is bounded.

#### Scenario: Reverse index returns subscribers for a channel
- **WHEN** N sessions have subscribed to a channel and the hub fans out
  a publication
- **THEN** the reverse index returns exactly those N session ids and no
  others, even under concurrent subscribe/unsubscribe traffic

### Requirement: Errors are typed and contextful
The crate SHALL define a `HubError` enum via `thiserror`, with variants
that carry enough context to diagnose without a stack trace (per
`docs/plan.md` §4.3). The crate SHALL NOT use `anyhow`.

#### Scenario: Error variants carry context
- **WHEN** a reviewer reads `HubError`
- **THEN** every variant carries either a structured field or a `#[from]`
  source, never a bare unit variant for an operational failure

### Requirement: Public items are documented
Every `pub` item in `cherenkov-core` SHALL have a rustdoc comment.
`#![warn(missing_docs)]` SHALL be enabled at the crate root and CI rejects
warnings.

#### Scenario: cargo doc emits no warnings
- **WHEN** CI runs `cargo doc --workspace --no-deps`
- **THEN** the build completes with zero warnings

### Requirement: HubBuilder accepts an Authenticator and AclChecker
`HubBuilder` SHALL expose `with_authenticator(Arc<dyn Authenticator>)`
and `with_acl_checker(Arc<dyn AclChecker>)`. When either method is
omitted, the corresponding default (`AllowAllAuthenticator` /
`AllowAllAcl`) SHALL be used so M1 / M2 binaries continue to compile and
behave unchanged.

#### Scenario: Both extension points are optional
- **WHEN** a binary calls `HubBuilder::new().with_channel_kind(...).
  with_broker(...).build()` without registering an authenticator or ACL
- **THEN** the hub builds successfully with `AllowAllAuthenticator` and
  `AllowAllAcl`

### Requirement: Hub::handle_connect stores claims on the session
`Hub` SHALL provide an `async fn handle_connect(&self, session:
&Session, token: &str) -> Result<ConnectOk, HubError>` that calls
`Authenticator::authenticate`, stores the resulting `SessionClaims` on
the session via an `ArcSwap<Option<Arc<SessionClaims>>>`, and returns a
`ConnectOk { subject, expires_at }` echoing the claims.

#### Scenario: Successful connect mutates session state once
- **WHEN** a session calls `handle_connect` with a valid token
- **THEN** the session's claims slot transitions from `None` to
  `Some(_)`; subsequent reads observe the same `Arc<SessionClaims>`

#### Scenario: Bad token leaves the session unauthenticated
- **WHEN** the authenticator returns `Err(AuthError::InvalidToken { .. })`
- **THEN** `handle_connect` returns `HubError::Auth(_)` and the
  session's claims slot remains `None`

### Requirement: Subscribe / Publish gate on the ACL after auth
`Hub::handle_subscribe` and `Hub::handle_publish` SHALL evaluate the
configured `AclChecker` against the session's stored claims before
invoking the channel kind or broker. A denial SHALL short-circuit the
operation: the channel kind SHALL NOT see the request, the broker
SHALL NOT see the publication, and the function SHALL return
`HubError::Acl(_)`.

#### Scenario: Denied subscribe never registers the session
- **WHEN** the configured `AclChecker` returns
  `Err(AclError::Denied { .. })` for a `handle_subscribe` call
- **THEN** the channel kind's `on_subscribe` is not invoked, the reverse
  index is unchanged, and the caller receives `HubError::Acl(_)`

#### Scenario: Denied publish never reaches the broker
- **WHEN** the configured `AclChecker` denies a `handle_publish` call
- **THEN** the channel kind's `on_publish` is not invoked, the broker
  records no publication, and the caller receives `HubError::Acl(_)`

### Requirement: Pre-Connect frames are rejected when auth is required
The `Hub` SHALL return `HubError::NotConnected` from
`handle_subscribe` and `handle_publish` whenever the configured
`Authenticator::allow_anonymous()` returns `false` and the session has
no stored claims, without consulting the channel kind, broker, or ACL
checker.

#### Scenario: Subscribe before Connect is gated
- **GIVEN** a `Hub` configured with a real `Authenticator`
  (`allow_anonymous() == false`) and a fresh session
- **WHEN** the session calls `handle_subscribe(channel)` before any
  `handle_connect`
- **THEN** the call returns `HubError::NotConnected` and no channel-kind
  or broker side effects occur

### Requirement: HubError covers Auth, Acl and connection-state failures
`HubError` SHALL include `Auth(AuthError)`, `Acl(AclError)`,
`NotConnected`, and `AlreadyConnected` variants. `Auth` and `Acl` SHALL
be reachable via `#[from]` so transports can pattern-match without
threading boilerplate.

#### Scenario: Variants are reachable via #[from]
- **WHEN** a transport handles an authenticator error
- **THEN** `?` propagation converts `AuthError` into `HubError::Auth`
  without manual conversion

### Requirement: Session stores claims via ArcSwap
`Session` SHALL hold `claims: ArcSwap<Option<Arc<SessionClaims>>>` so
the hub can publish a new authentication snapshot atomically without a
mutex. Reads SHALL be lock-free.

#### Scenario: Concurrent reads observe a consistent snapshot
- **GIVEN** a session whose claims have been set via `handle_connect`
- **WHEN** two tasks read `session.claims()` concurrently
- **THEN** both observe the same `Arc<SessionClaims>` value without
  blocking

