# schema-registry Specification

## Purpose
TBD - created by archiving change schema-validation. Update Purpose after archive.
## Requirements
### Requirement: cherenkov-schema implements SchemaValidator over JSON Schema
`cherenkov-schema` SHALL provide a `JsonSchemaRegistry` type that
implements `cherenkov_core::SchemaValidator` and validates publication
payloads against per-namespace JSON Schema documents. The crate's
`name()` SHALL return `"json-schema"`.

#### Scenario: Validator name is stable
- **WHEN** code calls `JsonSchemaRegistry::empty().name()`
- **THEN** the return value is `"json-schema"`

### Requirement: Channels resolve to namespaces via the first-dot rule
The crate SHALL expose a `namespace_of(channel: &str) -> &str` helper
that returns the part of `channel` before the first `.`. If `channel`
contains no `.`, the entire string SHALL be returned. This rule SHALL
be consistent with the rule applied inside
`SchemaValidator::validate`.

#### Scenario: Multi-segment channel resolves to the leading segment
- **WHEN** code calls `namespace_of("rooms.lobby.subroom")`
- **THEN** the return value is `"rooms"`

#### Scenario: Single-segment channel is its own namespace
- **WHEN** code calls `namespace_of("standalone")`
- **THEN** the return value is `"standalone"`

### Requirement: Schemas compile eagerly; invalid schemas fail at build time
`JsonSchemaRegistryBuilder::with_namespace` SHALL compile the supplied
`serde_json::Value` immediately and return
`RegistryError::InvalidSchema { namespace, reason }` if compilation
fails. The error SHALL include the namespace name and the validator's
own diagnostic.

#### Scenario: Invalid schema document is reported with namespace
- **WHEN** a builder receives a schema document that the JSON Schema
  compiler rejects, declared under namespace `"orders"`
- **THEN** `with_namespace` returns
  `Err(RegistryError::InvalidSchema { namespace: "orders", reason })`
  where `reason` is non-empty

### Requirement: Unknown namespaces are pass-through
The `validate(channel, data)` method SHALL return `Ok(())` without
parsing `data` whenever `channel`'s namespace has no registered schema.

#### Scenario: Unknown namespace skips JSON parsing
- **WHEN** the registry has only `"orders"` registered and a caller
  validates an arbitrary byte sequence against `"rooms.lobby"`
- **THEN** the call returns `Ok(())` — the byte sequence is not parsed
  as JSON and not validated against any schema

### Requirement: Validation errors are payload-free
`SchemaError::PayloadRejected { channel, reason }` SHALL carry
diagnostic text suitable for a wire-protocol `Error.message`. The
`reason` SHALL describe the schema violation (path, expected type) but
SHALL NOT include the rejected payload bytes.

#### Scenario: Invalid payload yields a non-empty payload-free reason
- **WHEN** a payload fails JSON Schema validation
- **THEN** the resulting `SchemaError::PayloadRejected.reason` is
  non-empty and does not include the raw payload contents

