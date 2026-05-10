## ADDED Requirements

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
