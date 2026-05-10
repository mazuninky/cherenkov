## ADDED Requirements

### Requirement: Core defines the SchemaValidator extension trait
`cherenkov-core` SHALL define an async trait `SchemaValidator: Send +
Sync + 'static` with a `name(&self) -> &'static str` method and an
`async fn validate(&self, channel: &str, data: &Bytes) ->
Result<(), SchemaError>` method. The crate SHALL NOT depend on
`jsonschema`, `prost-reflect`, or any other concrete validator backend.

#### Scenario: Core has no concrete validator dependencies
- **WHEN** a reviewer runs `cargo tree -p cherenkov-core`
- **THEN** the output does not list `jsonschema`, `prost-reflect`, or
  any other concrete schema-validation library

#### Scenario: Trait is object-safe
- **WHEN** a binary constructs `Arc<dyn SchemaValidator>`
- **THEN** the binary compiles, because `SchemaValidator` is
  object-safe

### Requirement: Hub validates payloads before the channel kind sees them
`Hub::handle_publish` SHALL call
`self.validator.validate(channel, &data).await` before invoking
`self.kind.on_publish(channel, data)`. A validator error SHALL
short-circuit the publish: the channel kind SHALL NOT be invoked, the
broker SHALL NOT receive the publication, and the function SHALL return
`HubError::Schema(_)` to the caller.

#### Scenario: Rejected payload does not advance the channel kind
- **WHEN** a `SchemaValidator` returns `Err(SchemaError::PayloadRejected
  { .. })` for a publish call
- **THEN** the channel kind's `on_publish` is not invoked, the broker
  does not record the publish, and the caller receives
  `HubError::Schema(_)`

#### Scenario: Default builder uses AllowAllValidator
- **WHEN** a binary builds a `Hub` via `HubBuilder::new()
  .with_channel_kind(...).with_broker(...).build()` without calling
  `with_schema_validator`
- **THEN** every publish is accepted, matching the M1 behaviour

### Requirement: HubError carries a typed Schema variant
`HubError` SHALL include a `Schema(SchemaError)` variant with `#[from]
SchemaError`, so transports can pattern-match on schema rejections to
emit the correct wire-protocol error code.

#### Scenario: Schema variant is reachable
- **WHEN** a reviewer reads `HubError`
- **THEN** the enum exposes a `Schema` variant whose `#[from]` source is
  `SchemaError`
