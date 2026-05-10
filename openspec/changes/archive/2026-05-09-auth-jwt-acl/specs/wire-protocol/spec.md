## ADDED Requirements

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
