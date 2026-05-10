# server-binary Specification

## Purpose
TBD - created by archiving change bootstrap-foundation. Update Purpose after archive.
## Requirements
### Requirement: cherenkov-server binary wires hub, broker, channel kind, transport
The `cherenkov-server` binary SHALL load a YAML config (figment), construct
a `Hub`, register `MemoryBroker` and `PubSubChannel`, mount `WsTransport`
on the configured path and port, and run until interrupted.

#### Scenario: Boot is one command
- **WHEN** a developer runs `cargo run -p cherenkov-server -- --config examples/echo/config.yaml`
- **THEN** the server starts, logs "listening on <addr>", and accepts
  WebSocket connections on the configured path

### Requirement: Single-node only at this milestone
The binary SHALL run as a single process with `MemoryBroker`. Multi-node
fan-out via Redis or NATS is explicitly out of scope and SHALL be added in
a later change.

#### Scenario: Two server instances do not share state
- **WHEN** two separate server processes are started
- **THEN** a publication to instance A is not delivered to subscribers on
  instance B; this is acceptable at M1

### Requirement: Echo example demonstrates end-to-end pub/sub
The repository SHALL ship `examples/echo/` containing an `index.html` with
two iframes, each opening a WebSocket directly (no SDK), exchanging
messages through the running server. The example SHALL include a README
with the exact run command.

#### Scenario: Browser demo exchanges messages
- **WHEN** a developer follows `examples/echo/README.md`
- **THEN** typing in one iframe causes the other to display the message
  within one second on a localhost run

### Requirement: Integration test proves WebSocket fan-out
`cherenkov-server/tests/echo.rs` SHALL spin up a hub on an ephemeral port,
open two WebSocket clients, have one publish to a channel both subscribed
to, and assert the second receives the publication. The test SHALL be
hermetic (no shared state, no external services).

#### Scenario: cargo test exercises the demo
- **WHEN** CI runs `cargo test --workspace --all-features`
- **THEN** the `echo` integration test passes deterministically and
  finishes within a few seconds

### Requirement: Configuration is YAML, layered, and documented
Configuration SHALL be loaded via `figment` from a YAML file plus
environment overrides (`CHERENKOV_*`). The YAML schema SHALL be documented
in `examples/echo/config.yaml` with comments on every field.

#### Scenario: Environment override beats file value
- **WHEN** a developer runs the binary with `CHERENKOV_LISTEN_ADDR=0.0.0.0:9000`
- **THEN** the server listens on `0.0.0.0:9000` even if the YAML lists a
  different address

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

