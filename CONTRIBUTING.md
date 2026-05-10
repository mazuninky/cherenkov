# Contributing to Cherenkov

Thank you for considering a contribution.

Cherenkov is a real-time messaging server written in Rust. The repository is
publicly visible from day one and the quality bar is "open-source software a
thoughtful reviewer would merge without major rework". Most of the conventions
that govern day-to-day work live in [`docs/plan.md`](docs/plan.md); this file
is the short orientation for new contributors.

## Before you start

1. Read [`docs/plan.md`](docs/plan.md) end-to-end. It is the working contract.
   Sections that matter most to first-time contributors:
   * §2 — architectural principles (pluggable everything, schema-as-contract).
   * §4 — code style.
   * §6 — testing.
   * §8 — guardrails (what *not* to do).
   * §9 — workflow (planning, commits, PR template).
2. Skim the relevant ADRs in [`docs/adr/`](docs/adr/) before changing
   anything that touches them.

## Set-up

* Install the Rust toolchain via [rustup](https://rustup.rs). `rust-toolchain.toml`
  pins the channel; on first `cargo` invocation the matching version is
  fetched automatically.
* Run the canonical local checks before pushing:

  ```sh
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --all-features -- -D warnings
  cargo test --workspace --all-features
  cargo doc --workspace --no-deps
  cargo deny check
  ```

  CI runs the same six gates plus the [RustSec audit GitHub Action]
  (`rustsec/audit-check`) — `cargo audit` locally if you want the same
  check before pushing. A red CI is a hard merge blocker (see
  `docs/plan.md` §8.11).

  [RustSec audit GitHub Action]: https://github.com/rustsec/audit-check

## Working on a change

* For anything larger than a typo, write a short plan first. See
  `docs/plan.md` §9.1.
* Architectural changes — anything that touches the wire protocol, the three
  extension traits, or cross-cutting concerns — require an ADR before code.
* Keep commits small and conventional. The format is described in
  `docs/plan.md` §9.2.
* Branch naming: `feat/<short-name>`, `fix/<short-name>`, `docs/<short-name>`.
* Rebase on `main` before merging. Linear history.

## Pull requests

PR descriptions follow the template in `docs/plan.md` §9.4: **What**,
**Why**, **How**, **Testing**, **Risks**. Performance claims need numbers
attached; security-relevant changes need a brief threat model.

## Reporting bugs

Open an issue with a minimum reproducer, your platform, the commit hash, and
the exact failure mode. Security-sensitive issues go to the address in
[`SECURITY.md`](SECURITY.md), not the public tracker.

## Code of conduct

Participation is governed by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).
