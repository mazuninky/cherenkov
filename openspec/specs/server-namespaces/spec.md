# server-namespaces Specification

## Purpose
TBD - created by archiving change schema-validation. Update Purpose after archive.
## Requirements
### Requirement: Server config exposes a namespaces map
`cherenkov-server`'s `ServerConfig` SHALL include a `namespaces`
section that maps a namespace name to a `NamespaceConfig`. Each
`NamespaceConfig` SHALL declare a `kind` (default `"json-schema"`) and
exactly one schema source: an inline `schema` JSON value OR a
`schema_path` filesystem path.

#### Scenario: YAML inline schema is parsed
- **WHEN** the server loads a YAML config with
  `namespaces.orders.schema` set to a JSON-Schema-shaped mapping
- **THEN** `config.namespaces.as_map().get("orders")` returns a
  `NamespaceConfig` whose `schema` field is `Some(_)` and `schema_path`
  is `None`

#### Scenario: deny_unknown_fields is enforced
- **WHEN** a YAML config under `namespaces.<name>` includes a key the
  schema does not declare (e.g. `schemaa: ...`)
- **THEN** `ServerConfig::load` returns an error

### Requirement: Misconfigured namespaces fail at startup
`run_with_listener` SHALL return `ServerError::Schema` if any
namespace declares both `schema` and `schema_path`, neither, or a
`schema_path` that does not exist or is not valid JSON.

#### Scenario: Both schema and schema_path are set
- **WHEN** `run_with_listener` is called with a namespace whose
  `NamespaceConfig` has both `schema` and `schema_path` populated
- **THEN** the call returns
  `Err(ServerError::Schema(reason))` where `reason` mentions the
  namespace name

#### Scenario: schema_path missing
- **WHEN** `run_with_listener` is called with a namespace whose
  `schema_path` points to a non-existent file
- **THEN** the call returns
  `Err(ServerError::Schema(reason))` where `reason` mentions the
  filesystem path

### Requirement: Server composes a JsonSchemaRegistry into the hub
`run_with_listener` SHALL build a `JsonSchemaRegistry` from
`config.namespaces` and pass it to
`HubBuilder::with_schema_validator`. When `config.namespaces` is
empty, the registry SHALL be empty and behave as a no-op.

#### Scenario: Empty namespaces map yields a permissive hub
- **WHEN** `run_with_listener` is called with the default
  `ServerConfig` (no namespaces declared)
- **THEN** every WebSocket publish to any channel succeeds, regardless
  of payload content

#### Scenario: Declared namespace enforces its schema end-to-end
- **WHEN** `run_with_listener` is called with one declared namespace
  (`orders` with a JSON Schema requiring `sku` and `qty`) and a
  WebSocket client publishes `{}` to `orders.created`
- **THEN** the client receives a `ServerFrame::Error` with
  `code = ErrorCode::ValidationFailed (5)` and the publishing frame's
  `request_id`

#### Scenario: Channels outside declared namespaces remain opaque
- **WHEN** the server runs with `orders` declared and a client
  publishes arbitrary bytes to `rooms.lobby`
- **THEN** the publication is delivered to subscribers as-is (no
  validation, no Error frame)

