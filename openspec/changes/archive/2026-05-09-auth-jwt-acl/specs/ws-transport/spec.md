## ADDED Requirements

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
