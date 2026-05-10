# Architectural decision records

This directory captures decisions that shape Cherenkov's architecture.
Format and process are defined in [`docs/plan.md`](../plan.md) §5.2.

| Number | Status   | Title                                                          |
| ------ | -------- | -------------------------------------------------------------- |
| 0001   | Accepted | [Pluggable architecture via trait objects](0001-pluggable-architecture.md) |
| 0002   | Accepted | [Broker trait lives in `cherenkov-core`](0002-broker-crate-boundary.md)    |
| 0003   | Accepted | [Schema validation is per-namespace](0003-schema-as-contract.md)           |
| 0004   | Accepted | [Authentication and ACL](0004-auth-and-acl.md)                             |

## Conventions

* One markdown file per decision, numbered sequentially.
* Statuses: `Proposed`, `Accepted`, `Superseded by ADR-N`, `Rejected`.
* Each ADR has Status, Context, Decision, Consequences, and Alternatives
  considered.
* ADRs are immutable once accepted. Course corrections add a new ADR that
  supersedes the old one rather than rewriting it.
