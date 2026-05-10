## ADDED Requirements

### Requirement: cherenkov-auth ships NamespaceAcl with glob rules
`cherenkov-auth` SHALL implement `NamespaceAcl` evaluating an ordered
list of `AclRule { effect, subjects, actions, channels }` triples,
where `subjects` and `channels` are `globset` patterns and `actions` is
a set of `AclAction` values. The first matching rule SHALL determine
the decision; if no rule matches, the default is `Deny`.

#### Scenario: Allow rule wins on first match
- **GIVEN** rules
  `[ Allow { subjects: ["alice"], actions: [Publish], channels: ["orders.*"] } ]`
- **WHEN** subject `"alice"` publishes to `"orders.created"`
- **THEN** the decision is `AclDecision::Allow`

#### Scenario: Fall-through default-denies
- **GIVEN** an empty rule list
- **WHEN** any subject attempts any action on any channel
- **THEN** the decision is `Err(AclError::Denied { .. })`

#### Scenario: Wildcard subject matches everyone
- **GIVEN** a single rule `Allow { subjects: ["*"], actions:
  [Subscribe], channels: ["public.*"] }`
- **WHEN** any subject (including `"anonymous"`) subscribes to
  `"public.news"`
- **THEN** the decision is `AclDecision::Allow`

#### Scenario: Deny rule shadows a later allow
- **GIVEN** rules
  `[ Deny { subjects: ["*"], actions: [Publish], channels: ["admin.*"] },
     Allow { subjects: ["root"], actions: [Publish], channels: ["admin.*"] } ]`
- **WHEN** subject `"root"` publishes to `"admin.kick"`
- **THEN** the first rule matches and the decision is
  `Err(AclError::Denied { .. })`

### Requirement: NamespaceAcl globs are compiled eagerly
`NamespaceAcl::new(rules)` SHALL compile every glob pattern at
construction time. A malformed pattern SHALL be reported synchronously
as a construction error, never as a per-request runtime failure.

#### Scenario: Bad glob is rejected at construction
- **WHEN** a binary calls `NamespaceAcl::new` with a rule whose
  `channels` contains `"["` (an unclosed glob class)
- **THEN** the call returns an error at startup, before any session
  evaluation
