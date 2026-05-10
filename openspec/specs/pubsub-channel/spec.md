# pubsub-channel Specification

## Purpose
TBD - created by archiving change bootstrap-foundation. Update Purpose after archive.
## Requirements
### Requirement: PubSubChannel implements ChannelKind
`cherenkov-channel-pubsub` SHALL provide `PubSubChannel` implementing
`ChannelKind`. The crate SHALL be the first concrete `ChannelKind`
implementation, used by the M1 echo demo.

#### Scenario: Subscribers receive subsequent publications
- **WHEN** session A subscribes to channel `room:1` and session B publishes
  to the same channel
- **THEN** session A receives B's publication exactly once

### Requirement: Subscribe response carries epoch and offset
On subscribe, `PubSubChannel` SHALL return the current `epoch` and the
`offset` of the last accepted publication so clients can later detect gaps.

#### Scenario: Empty channel returns offset zero
- **WHEN** the first session ever subscribes to a freshly created channel
- **THEN** the response carries `epoch = 0` and `offset = 0`

#### Scenario: Established channel returns last offset
- **WHEN** N publications have been accepted before a new subscribe
- **THEN** the subscribe response carries `offset = N`

### Requirement: History is bounded by TTL and max-size
`PubSubChannel` SHALL retain a ring of recent publications per channel,
with both a TTL and a max-size bound. Defaults SHALL be safe (small TTL,
small max size); both SHALL be configurable per namespace.

#### Scenario: Oldest entries are evicted when max size is exceeded
- **WHEN** a channel configured with `max_size = 10` receives 12
  publications
- **THEN** the history contains exactly the last 10 publications

#### Scenario: Expired entries are evicted on read
- **WHEN** a publication's TTL has elapsed and a new subscriber requests
  history
- **THEN** the expired publication is not returned

### Requirement: Recovery and durability are out of scope at M1
This milestone SHALL NOT implement reconnect-time recovery, history
replay on resume, or durable history storage. History is best-effort,
in-process only.

#### Scenario: Reconnecting session does not auto-replay
- **WHEN** a session reconnects after disconnecting
- **THEN** it receives only publications produced after its new subscribe;
  prior history is not re-delivered automatically

