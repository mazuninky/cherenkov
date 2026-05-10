## ADDED Requirements

### Requirement: Core defines the Authenticator extension trait
`cherenkov-core` SHALL define an async trait
`Authenticator: Send + Sync + 'static` with a `name(&self) ->
&'static str`, an `allow_anonymous(&self) -> bool` (default `false`),
and an `async fn authenticate(&self, token: &str) ->
Result<SessionClaims, AuthError>` method. The crate SHALL NOT depend on
`jsonwebtoken`, `oauth2`, or any other concrete credential backend.

#### Scenario: Core has no concrete authenticator dependencies
- **WHEN** a reviewer runs `cargo tree -p cherenkov-core`
- **THEN** the output does not list `jsonwebtoken`, `oauth2`, or any
  other concrete authentication library

#### Scenario: Trait is object-safe
- **WHEN** a binary constructs `Arc<dyn Authenticator>`
- **THEN** the binary compiles, because `Authenticator` is object-safe

### Requirement: SessionClaims carry subject, permissions and expiry
`cherenkov-core` SHALL expose `SessionClaims { subject: String,
permissions: Vec<String>, expires_at: u64 }` (Unix seconds; `0` means
"no expiry known") plus a `SessionClaims::anonymous()` constructor that
returns `subject = "anonymous"` with no permissions.

#### Scenario: anonymous() returns a stable shape
- **WHEN** a binary calls `SessionClaims::anonymous()`
- **THEN** `subject` is `"anonymous"`, `permissions` is empty, and
  `expires_at` is `0`

### Requirement: AuthError variants carry no token bytes
`cherenkov-core::AuthError` SHALL define `InvalidToken { reason: String }`
and `Other(String)` variants only. Neither variant SHALL store the
incoming token bytes — `reason` MUST be human-readable text safe to
forward to the client.

#### Scenario: Reviewer audits AuthError variants
- **WHEN** a reviewer reads `auth.rs`
- **THEN** every variant's payload is a `String` reason or implementation
  message, never the raw token

### Requirement: AllowAllAuthenticator preserves M1 / M2 behaviour
`cherenkov-core` SHALL ship `AllowAllAuthenticator` with `name() ==
"allow-all"`, `allow_anonymous() == true`, and `authenticate(_)` always
returning `SessionClaims::anonymous()`. The `Hub` SHALL default to this
authenticator when no explicit one is registered.

#### Scenario: Hub built without authenticator stays anonymous
- **WHEN** a binary builds a `Hub` via `HubBuilder::new()` without
  calling `with_authenticator`
- **THEN** every session is treated as anonymous and pre-`Connect`
  frames are accepted, matching the M1 / M2 echo demo
