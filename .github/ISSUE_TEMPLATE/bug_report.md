---
name: Bug report
about: Report unexpected or incorrect behaviour
title: "bug: <short summary>"
labels: ["bug", "needs-triage"]
assignees: []
---

## Summary

<!-- One sentence: what is broken? -->

## Reproduction

<!--
Smallest steps that reproduce the bug. Include:
  * commit hash (`git rev-parse HEAD`)
  * platform and Rust version (`rustc --version`)
  * minimal config or code snippet
-->

```sh
# steps go here
```

## Expected behaviour

<!-- What you expected to happen. -->

## Actual behaviour

<!--
What actually happened. Include logs, error messages, and any relevant
metric values. Trim payloads — Cherenkov does not log them and neither
should issue reports.
-->

## Environment

* Cherenkov version / commit:
* Rust version:
* OS / arch:
* Transport (`ws` / `wt` / `sse`):
* Broker (`memory` / `redis` / `nats`):

## Anything else?

<!-- Workarounds, related issues, hypotheses. -->
