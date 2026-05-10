## ADDED Requirements

### Requirement: cherenkov-server binary wires hub, broker, channel kind, transport
The `cherenkov-server` binary SHALL load a YAML config (figment), construct
a `Hub`, register `MemoryBroker` and `PubSubChannel`, mount `WsTransport`
on the configured path and port, and run until interrupted.

#### Scenario: Boot is one command
- **WHEN** a developer runs `cargo run -p cherenkov-server -- --config examples/echo/config.yaml`
- **THEN** the server starts, logs "listening on <addr>", and accepts
  WebSocket connections on the configured path

### Requirement: Single-node only at this milestone
The binary SHALL run as a single process with `MemoryBroker`. Multi-node
fan-out via Redis or NATS is explicitly out of scope and SHALL be added in
a later change.

#### Scenario: Two server instances do not share state
- **WHEN** two separate server processes are started
- **THEN** a publication to instance A is not delivered to subscribers on
  instance B; this is acceptable at M1

### Requirement: Echo example demonstrates end-to-end pub/sub
The repository SHALL ship `examples/echo/` containing an `index.html` with
two iframes, each opening a WebSocket directly (no SDK), exchanging
messages through the running server. The example SHALL include a README
with the exact run command.

#### Scenario: Browser demo exchanges messages
- **WHEN** a developer follows `examples/echo/README.md`
- **THEN** typing in one iframe causes the other to display the message
  within one second on a localhost run

### Requirement: Integration test proves WebSocket fan-out
`cherenkov-server/tests/echo.rs` SHALL spin up a hub on an ephemeral port,
open two WebSocket clients, have one publish to a channel both subscribed
to, and assert the second receives the publication. The test SHALL be
hermetic (no shared state, no external services).

#### Scenario: cargo test exercises the demo
- **WHEN** CI runs `cargo test --workspace --all-features`
- **THEN** the `echo` integration test passes deterministically and
  finishes within a few seconds

### Requirement: Configuration is YAML, layered, and documented
Configuration SHALL be loaded via `figment` from a YAML file plus
environment overrides (`CHERENKOV_*`). The YAML schema SHALL be documented
in `examples/echo/config.yaml` with comments on every field.

#### Scenario: Environment override beats file value
- **WHEN** a developer runs the binary with `CHERENKOV_LISTEN_ADDR=0.0.0.0:9000`
- **THEN** the server listens on `0.0.0.0:9000` even if the YAML lists a
  different address
