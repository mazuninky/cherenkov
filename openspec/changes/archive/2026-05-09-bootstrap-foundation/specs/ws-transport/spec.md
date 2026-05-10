## ADDED Requirements

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
