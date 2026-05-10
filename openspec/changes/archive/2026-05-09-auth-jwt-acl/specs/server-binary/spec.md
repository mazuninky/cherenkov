## ADDED Requirements

### Requirement: Server config exposes optional auth and acl sections
`cherenkov-server::config::Config` SHALL expose:
- `auth: Option<AuthConfig>` with fields `hmac_secret: String`,
  `audiences: Vec<String>`, `issuer: Option<String>`.
- `acl: Option<AclConfig>` with fields `rules: Vec<AclRuleConfig>` and
  `default_allow: bool` (defaulting to `false`).
- Each `AclRuleConfig` carries `effect ("allow" | "deny")`, `channel`
  (glob), `subject` (optional glob), and `action ("subscribe" |
  "publish" | "any")`.

All structs SHALL use `serde(deny_unknown_fields)` so typos in YAML
fail fast.

#### Scenario: Unknown field is rejected
- **WHEN** the loader reads a YAML file whose `auth:` block contains
  `secret: ...` (instead of `hmac_secret`)
- **THEN** loading fails with a serde error pointing at the offending
  field

#### Scenario: Empty hmac_secret is rejected
- **WHEN** the loader reads `auth: { hmac_secret: "" }`
- **THEN** loading fails before the hub is built; the error mentions
  the empty secret

### Requirement: Server composes JwtAuthenticator and NamespaceAcl from config
`app::run_with_listener` SHALL:
- When `auth:` is present, build a `JwtAuthenticator` and register it
  via `HubBuilder::with_authenticator`.
- When `acl:` is present, build a `NamespaceAcl` and register it via
  `HubBuilder::with_acl_checker`.
- When either section is absent, fall back to `AllowAllAuthenticator`
  / `AllowAllAcl` so the M1 / M2 echo demo continues to work.

#### Scenario: Missing auth + acl sections preserve M1 behaviour
- **GIVEN** a config with neither `auth:` nor `acl:` declared
- **WHEN** the server boots
- **THEN** every WebSocket session is anonymous and every channel is
  reachable, identical to the M1 echo demo

#### Scenario: Both sections wire through to the hub
- **GIVEN** a config with `auth.hmac_secret = "<secret>"` and one allow
  rule
- **WHEN** the server boots and a client sends a valid `Connect` token
- **THEN** the hub uses `JwtAuthenticator` for the connect call and
  `NamespaceAcl` for every subsequent subscribe / publish

### Requirement: Server lib re-exports config types for integration tests
`cherenkov-server::lib` SHALL re-export `AuthConfig`, `AclConfig`,
`AclRuleConfig`, `AclEffectConfig`, and `AclActionConfig` so that
integration tests in `cherenkov-server/tests/` can build configs
programmatically without depending on YAML parsing.

#### Scenario: Test crate builds a Config in code
- **WHEN** an integration test constructs a `Config` literal with
  `auth: Some(AuthConfig { .. })`
- **THEN** the code compiles using only the public re-exports
