## ADDED Requirements

### Requirement: Core defines the AclChecker extension trait
`cherenkov-core` SHALL define an async trait
`AclChecker: Send + Sync + 'static` with a `name(&self) ->
&'static str` and an `async fn check(&self, claims: &SessionClaims,
action: AclAction, channel: &str) -> Result<AclDecision, AclError>`.
The crate SHALL NOT depend on `globset` or any other rule-engine
backend.

#### Scenario: Core has no concrete ACL backend
- **WHEN** a reviewer runs `cargo tree -p cherenkov-core`
- **THEN** the output does not list `globset`, `regex` (transitively
  imported only by `tracing-subscriber`), or any concrete rule library

#### Scenario: Trait is object-safe
- **WHEN** a binary constructs `Arc<dyn AclChecker>`
- **THEN** the binary compiles

### Requirement: AclAction enumerates the gated operations
`AclAction` SHALL define `Subscribe` and `Publish` variants with
`as_str(self) -> &'static str` returning `"subscribe"` / `"publish"`
respectively. These string forms are stable for use in config files,
log fields, and metrics.

#### Scenario: as_str is the rule-file vocabulary
- **WHEN** the server config encodes an ACL rule
- **THEN** the `action` token uses `"subscribe"` or `"publish"`,
  matching `AclAction::as_str` exactly

### Requirement: AclError carries the (subject, action, channel) triple
`AclError::Denied` SHALL carry `subject: String`, `action: &'static
str`, `channel: String`, and a human-readable `reason: String`. The
variant SHALL NOT carry payload bytes from the publish request.

#### Scenario: Denial includes enough context for triage
- **WHEN** a reviewer reads `Display for AclError::Denied`
- **THEN** the formatted error names the subject, the action, and the
  channel, but never the publish payload

### Requirement: AllowAllAcl preserves M1 / M2 behaviour
`cherenkov-core` SHALL ship `AllowAllAcl` with `name() == "allow-all"`
returning `Ok(AclDecision::Allow)` for every input. The `Hub` SHALL
default to this checker when none is registered.

#### Scenario: Hub built without ACL allows every action
- **WHEN** a binary builds a `Hub` via `HubBuilder::new()` without
  calling `with_acl_checker`
- **THEN** every `Subscribe` / `Publish` is accepted regardless of
  channel
