# ws-transport Specification

## Purpose
TBD - created by archiving change bootstrap-foundation. Update Purpose after archive.
## Requirements
### Requirement: WsTransport implements Transport over axum + tokio-tungstenite
`cherenkov-transport-ws` SHALL provide `WsTransport` implementing
`Transport`, mounted as an `axum` route that upgrades incoming HTTP
requests to WebSocket via `tokio-tungstenite`.

#### Scenario: HTTP upgrade succeeds on the configured path
- **WHEN** a browser connects to `ws://localhost:<port>/connect/v1`
- **THEN** the server completes the WebSocket handshake and the connection
  is registered as a new `Session`

### Requirement: Frames are decoded into ClientFrame and dispatched to Hub
On every incoming binary message, the transport SHALL decode the bytes
into `ClientFrame` via `cherenkov-protocol::decode` and dispatch to the
appropriate `Hub` entry point (`handle_subscribe`, `handle_unsubscribe`,
`handle_publish`).

#### Scenario: Malformed frame closes the connection
- **WHEN** a client sends a binary message that fails to decode
- **THEN** the transport closes the WebSocket with a 1002 protocol error
  and does not propagate the bytes to the hub

### Requirement: Outbound publications are encoded as ServerFrame
Publications fanned out by the hub SHALL be encoded into `ServerFrame`
via `cherenkov-protocol::encode` and written to the client as a single
binary WebSocket message.

#### Scenario: Each publication maps to one binary message
- **WHEN** the hub fans out publication P to a connected session
- **THEN** the transport sends exactly one binary WebSocket message whose
  payload is `encode(ServerFrame::Publication(P))`

### Requirement: Connection lifecycle is cancel-safe and explicit
The transport SHALL run cleanup through an explicit `async fn close()` path on
client disconnect, server shutdown, or connection error, unsubscribing the
session from all channels and freeing its `SessionRegistry` entry. The
cleanup path MUST NOT rely on `Drop` for correctness (per `docs/plan.md` §8.9).

#### Scenario: Disconnect releases session resources
- **WHEN** a connected client closes the WebSocket
- **THEN** the session is removed from `SessionRegistry` and from every
  channel's reverse index before the connection task returns

### Requirement: WS transport dispatches Connect to the hub
`cherenkov-transport-ws` SHALL recognise `ClientFrame::Connect` and
forward the token to `Hub::handle_connect`. On success the transport
SHALL emit `ServerFrame::ConnectOk` carrying the same `request_id`,
`subject`, and `expires_at` returned by the hub.

#### Scenario: Successful connect produces ConnectOk
- **WHEN** a client sends `ClientFrame::Connect { request_id: 42,
  token: "<valid-jwt>" }`
- **THEN** the server replies with `ServerFrame::ConnectOk { request_id:
  42, subject, expires_at }` and no `Error` frame is emitted

### Requirement: WS transport maps auth and ACL errors to wire codes
On hub failure, `cherenkov-transport-ws` SHALL map:
- `HubError::Auth(_)` → `Error{code = ErrorCode::InvalidToken (6)}`
- `HubError::Acl(_)`  → `Error{code = ErrorCode::AclDenied (7)}`
- `HubError::NotConnected` →
  `Error{code = ErrorCode::NotConnected (8)}`

The `request_id` of the originating frame SHALL be echoed in the
`Error` frame.

#### Scenario: Bad token returns InvalidToken
- **WHEN** the hub returns `HubError::Auth(_)` for a `Connect` frame
  with `request_id = 7`
- **THEN** the client receives `Error { request_id: 7, code: 6,
  message }` and the WebSocket closes only after the error frame is
  flushed

#### Scenario: Pre-Connect Subscribe returns NotConnected
- **WHEN** an authenticated hub rejects a `Subscribe` frame because the
  session has not yet sent `Connect`
- **THEN** the client receives `Error { code: 8, .. }`

### Requirement: ACL denial preserves the connection
A `HubError::Acl(_)` denial SHALL emit an `Error` frame and leave the
WebSocket open so the client can attempt other channels. Only
authentication failures and decode errors SHALL terminate the session.

#### Scenario: Subscriber denied to one channel can subscribe to another
- **GIVEN** an authenticated session denied `Subscribe` to `"private.x"`
  by the ACL
- **WHEN** the same session sends `Subscribe { channel: "public.y" }`
  next
- **THEN** the second subscribe is processed normally; the WebSocket was
  not closed by the prior denial

