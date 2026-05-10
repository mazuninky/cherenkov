# memory-broker Specification

## Purpose
TBD - created by archiving change bootstrap-foundation. Update Purpose after archive.
## Requirements
### Requirement: MemoryBroker implements Broker
`cherenkov-broker` SHALL provide `MemoryBroker` implementing `Broker` over
`tokio::sync::broadcast`, with one broadcast channel per topic created on
demand and reaped when the last subscriber leaves.

#### Scenario: First subscriber creates the underlying broadcast
- **WHEN** a hub calls `subscribe("room:1")` for the first time
- **THEN** `MemoryBroker` lazily creates a `tokio::sync::broadcast` channel
  of the configured capacity for `room:1`

#### Scenario: Last unsubscribe reaps the broadcast
- **WHEN** the final subscriber to a topic unsubscribes
- **THEN** `MemoryBroker` drops the underlying broadcast so memory is
  reclaimed, and a future subscribe creates a fresh one

### Requirement: Publish is non-blocking and lossy under back pressure
Publish SHALL never block the caller. Under broadcast back pressure, slow
subscribers SHALL be dropped (the `RecvError::Lagged` semantics of
`tokio::sync::broadcast`) and the loss SHALL be reflected in metrics.

#### Scenario: Slow subscriber is dropped, fast subscribers continue
- **WHEN** subscriber A reads slowly while subscriber B reads in real time
- **THEN** A's stream eventually returns `Lagged` and is removed; B
  continues receiving every publication

### Requirement: Broker is the only single-node fan-out path at M1
This milestone SHALL ship `MemoryBroker` only. Redis and NATS brokers are
out of scope for this change and SHALL be added in later changes.

#### Scenario: Workspace builds without Redis or NATS dependencies
- **WHEN** CI builds the default feature set
- **THEN** neither `fred` nor `async-nats` appears in the dependency
  closure of `cherenkov-server`

