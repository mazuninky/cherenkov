# wire-protocol Specification

## Purpose
TBD - created by archiving change bootstrap-foundation. Update Purpose after archive.
## Requirements
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

### Requirement: Connect / ConnectOk variants for authentication
`proto/v1.proto` SHALL append a `Connect` request to `ClientFrame` as
oneof variant `4`, and a `ConnectOk` response to `ServerFrame` as oneof
variant `5`. `Connect` SHALL carry `request_id` and an opaque `token`
string. `ConnectOk` SHALL carry `request_id`, the authenticated
`subject`, and `expires_at` (Unix seconds; `0` means "no expiry known").

#### Scenario: Both variants are appended, never reordered
- **WHEN** a reviewer reads `proto/v1.proto`
- **THEN** `Connect` is `ClientFrame.kind = 4`, `ConnectOk` is
  `ServerFrame.kind = 5`, and the M1 variants `1..=3` / `1..=4`
  retain their original tags

### Requirement: Three new wire ErrorCodes for auth and ACL
`cherenkov-protocol::ErrorCode` SHALL include `InvalidToken = 6`,
`AclDenied = 7`, and `NotConnected = 8`. Numeric values are stable for
v1; existing codes `1..=5` SHALL retain their values.

#### Scenario: Codes 6, 7, 8 are reserved exactly
- **WHEN** a transport encodes an `Error` frame for an authentication or
  ACL failure
- **THEN** the wire `code` field is `6` for an invalid token, `7` for an
  ACL denial, and `8` for a pre-`Connect` frame on an authenticated hub

### Requirement: Connect and ConnectOk round-trip through encode/decode
`cherenkov-protocol` SHALL provide hand-written wrappers and
`encode_*` / `decode_*` helpers for `Connect` and `ConnectOk` and SHALL
ship a proptest round-trip plus an `insta` snapshot pinning the
canonical encoding of each variant.

#### Scenario: Round-trip preserves every field
- **WHEN** `proptest` generates an arbitrary `Connect` (or `ConnectOk`),
  encodes it, then decodes the bytes
- **THEN** the decoded frame is bit-for-bit equal to the original

