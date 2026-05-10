## ADDED Requirements

### Requirement: Cargo workspace lists all 14 member crates
The repository root SHALL define a Cargo workspace whose `members` array
lists all 14 crates from `docs/plan.md` §11, with shared dependency versions
declared in `[workspace.dependencies]` and resolved by workspace inheritance.

#### Scenario: Fresh checkout compiles
- **WHEN** a developer clones the repository and runs `cargo build --workspace`
- **THEN** every member crate compiles without errors and without warnings

#### Scenario: Stub crates are real targets
- **WHEN** a developer runs `cargo metadata --format-version 1`
- **THEN** all 14 crates from `docs/plan.md` §11 appear as workspace members

### Requirement: Minimum supported Rust version is pinned to 1.83
The workspace SHALL pin its toolchain to Rust 1.83 via `rust-toolchain.toml`,
and bumping the MSRV SHALL require an explicit issue and release note.

#### Scenario: Toolchain file present
- **WHEN** a developer runs `rustc --version` inside the repository
- **THEN** the version reported is 1.83.x because `rust-toolchain.toml` pins it

### Requirement: Lint, format, and supply-chain configuration are checked in
The workspace SHALL include `rustfmt.toml`, `clippy.toml`, `deny.toml`, and
`.cargo/config.toml` matching the rules in `docs/plan.md` §4 and §8.1, with
imports grouped via `imports_granularity = "Module"` and
`group_imports = "StdExternalCrate"`.

#### Scenario: Format check passes
- **WHEN** CI runs `cargo fmt --all -- --check`
- **THEN** the command exits 0 on a clean tree

#### Scenario: Clippy treats warnings as errors
- **WHEN** CI runs `cargo clippy --all-targets --all-features -- -D warnings`
- **THEN** the command exits 0 on a clean tree

#### Scenario: cargo-deny rejects forbidden licenses
- **WHEN** a contributor adds a GPL/AGPL/SSPL-licensed dependency
- **THEN** `cargo deny check licenses` exits non-zero and CI fails

### Requirement: GitHub Actions CI gates every pull request
A CI workflow at `.github/workflows/ci.yml` SHALL run, on every push and
pull request to `main`, the jobs `fmt`, `clippy`, `test`, `deny`, `audit`,
and `doc`. All jobs SHALL be required for merge.

#### Scenario: Red CI blocks merge
- **WHEN** any of the six jobs fails on a pull request
- **THEN** the pull request cannot be merged via the GitHub UI

#### Scenario: First commit is green
- **WHEN** the bootstrap commit lands on `main`
- **THEN** the CI workflow succeeds without any job being skipped or
  configured to allow failure

### Requirement: Repository ships license, governance, and contribution files
The repository root SHALL contain `LICENSE` (MIT), `CONTRIBUTING.md`,
`CODE_OF_CONDUCT.md`, `SECURITY.md`, and `.github/` with at least one
issue template and one pull request template.

#### Scenario: License is MIT
- **WHEN** a reader opens `LICENSE`
- **THEN** the file contains the SPDX MIT license text and the current year

### Requirement: gitignore covers Rust artifacts and secrets
The `.gitignore` SHALL exclude Rust build artifacts (`target/`, `Cargo.lock`
in libraries — kept in this workspace because there is a binary), and the
secret patterns from `docs/plan.md` §8.10 (`*.pem`, `*.key`, `.env`,
`secrets/`).

#### Scenario: Secret patterns are ignored
- **WHEN** a developer creates a file matching one of the secret patterns
- **THEN** `git status` does not list it as untracked

### Requirement: First ADR captures the pluggable architecture decision
The repository SHALL contain `docs/adr/0001-pluggable-architecture.md`
following the ADR template from `docs/plan.md` §5.2 and recording the
decisions in §2.1.

#### Scenario: ADR uses the canonical template
- **WHEN** a reader opens `docs/adr/0001-pluggable-architecture.md`
- **THEN** the file has Status, Context, Decision, Consequences, and
  Alternatives Considered sections in that order
