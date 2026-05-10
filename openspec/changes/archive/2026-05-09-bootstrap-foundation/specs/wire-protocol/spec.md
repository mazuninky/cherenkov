## ADDED Requirements

### Requirement: Wire protocol is defined as Protobuf v1 schema
`cherenkov-protocol` SHALL define `ClientFrame` and `ServerFrame` in
`proto/v1.proto` using `proto3` syntax, with `prost-build` generating Rust
types at build time. The protocol path is `/connect/v1`.

#### Scenario: Generated types compile
- **WHEN** a developer runs `cargo build -p cherenkov-protocol`
- **THEN** `prost-build` regenerates Rust types from `proto/v1.proto` and
  the crate compiles without warnings

#### Scenario: Top-level frames are oneofs
- **WHEN** a reviewer reads `proto/v1.proto`
- **THEN** `ClientFrame` and `ServerFrame` each contain a single `oneof`
  payload field so new variants can be added without breaking older clients

### Requirement: Public API exposes ergonomic wrapper types
`cherenkov-protocol` SHALL expose hand-written wrapper types in
`src/frame.rs` that hide `prost`-generated structs from callers. Callers
SHALL NOT need to depend on `prost` directly.

#### Scenario: Public API does not leak prost
- **WHEN** a downstream crate uses only `cherenkov_protocol::frame::*`
- **THEN** the downstream crate compiles without listing `prost` as a
  dependency

### Requirement: Protocol provides encode and decode helpers
The crate SHALL expose `encode(frame) -> Bytes` and
`decode(bytes) -> Result<Frame, DecodeError>` helpers for both client and
server frames, returning typed errors rather than panicking on malformed
input.

#### Scenario: Malformed bytes return DecodeError
- **WHEN** a caller invokes `decode` with random bytes
- **THEN** the call returns `Err(DecodeError::*)` rather than panicking

### Requirement: Round-trip property test guards encoding correctness
The crate SHALL include a `proptest` test asserting that for any
generated frame `f`, `decode(encode(f)) == f`.

#### Scenario: Round-trip succeeds for arbitrary frames
- **WHEN** `cargo test -p cherenkov-protocol` runs
- **THEN** the round-trip property test executes at least 256 cases and
  passes

### Requirement: Protocol changes are guarded by snapshot tests
The crate SHALL include `insta` snapshot tests for canonical encoded forms
of each variant, so that any wire-format change shows up as a reviewable
diff.

#### Scenario: Snapshot review surfaces wire changes
- **WHEN** a developer changes encoding behavior
- **THEN** `cargo test -p cherenkov-protocol` fails until the developer runs
  `cargo insta review` and approves the new snapshots
