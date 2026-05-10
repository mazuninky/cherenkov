<!--
Cherenkov PR template. Mirrors `docs/plan.md` §9.4. Keep sections short; if
a section is genuinely empty, replace it with `n/a` rather than deleting it.
-->

## What

<!-- One paragraph: what does this PR change? -->

## Why

<!-- Motivation. Link the issue (`Refs #123` / `Fixes #123`). -->

## How

<!--
Notable design choices: trade-offs, deviations from existing patterns,
any ADRs added or amended. If the change touches the wire protocol or the
core extension traits, link the ADR here.
-->

## Testing

<!--
What did you add or change in the test suite, and how can a reviewer
reproduce it locally? Performance claims need numbers (criterion output,
flamegraphs).
-->

## Risks

<!--
What could break, what was deliberately *not* addressed, and what should
the reviewer pay extra attention to.
-->

## Checklist

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo doc --workspace --no-deps` (no warnings under `RUSTDOCFLAGS="-D warnings"`)
- [ ] `cargo deny check`
- [ ] New public items have rustdoc; new behaviour has tests
- [ ] PR title follows the conventional-commits format (`docs/plan.md` §9.2)
