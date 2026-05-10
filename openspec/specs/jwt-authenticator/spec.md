# jwt-authenticator Specification

## Purpose
TBD - created by archiving change auth-jwt-acl. Update Purpose after archive.
## Requirements
### Requirement: cherenkov-auth ships JwtAuthenticator on jsonwebtoken
`cherenkov-auth` SHALL implement `JwtAuthenticator` (and a
`JwtAuthBuilder`) backed by the `jsonwebtoken` crate. HS256 SHALL be the
default algorithm; HS384 and HS512 SHALL be opt-in via builder methods.
Implementations SHALL surface failures as `AuthError::InvalidToken`
with a human-readable reason that does not include the token bytes.

#### Scenario: Default algorithm is HS256
- **WHEN** a binary calls `JwtAuthBuilder::new(secret).build()` without
  selecting an algorithm
- **THEN** the resulting `JwtAuthenticator` validates HS256-signed JWTs
  successfully and rejects HS384 / HS512 tokens with
  `AuthError::InvalidToken`

### Requirement: JwtAuthenticator enforces audience and issuer when configured
`JwtAuthBuilder` SHALL accept an optional `audiences: Vec<String>` and
an optional `issuer: String`. When set, validation SHALL reject tokens
whose `aud` / `iss` claims do not match exactly, with `reason` naming
the failing claim.

#### Scenario: Audience mismatch is rejected
- **GIVEN** a `JwtAuthenticator` configured with
  `audiences = ["cherenkov-prod"]`
- **WHEN** a token whose `aud` claim is `"cherenkov-staging"` is
  validated
- **THEN** `authenticate` returns
  `Err(AuthError::InvalidToken { reason })` and `reason` mentions the
  audience

### Requirement: Expired tokens are rejected
`JwtAuthenticator` SHALL reject any token whose `exp` claim is in the
past relative to `SystemTime::now()`. The `expires_at` field of the
returned `SessionClaims` (on success) SHALL be the JWT `exp` claim.

#### Scenario: Expired token returns InvalidToken
- **WHEN** a token with `exp` 60 seconds in the past is validated
- **THEN** `authenticate` returns `Err(AuthError::InvalidToken { .. })`,
  not a successful claims object

