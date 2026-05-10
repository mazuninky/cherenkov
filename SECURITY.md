# Security policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately, **not** via the public
issue tracker.

* Email: **security@cherenkov.dev**
* Optional: encrypt with the maintainer's public key, available at
  <https://cherenkov.dev/.well-known/security-pgp.asc>.

Include a description, a minimal reproducer, the affected commit or
release, and your assessment of the impact. We acknowledge reports within
72 hours and aim to provide a status update within 14 days.

## Supported versions

Cherenkov is pre-`0.1.0`. Until the first stable release, only the most
recent commit on `main` is considered supported. We do not backport fixes.

| Version    | Supported               |
| ---------- | ----------------------- |
| `main`     | Yes                     |
| `< 0.1.0`  | No (no stable releases) |

## Disclosure timeline

Default coordinated-disclosure window: 90 days from the date of the report.
We may publish earlier if a fix lands and is deployed; we may extend the
window for issues that require a coordinated upstream change in
dependencies.

## Out of scope

* Vulnerabilities in third-party services configured to talk to Cherenkov
  (Redis, NATS, JWT issuers). Report those to the upstream project.
* Issues that require an attacker to already be a privileged operator of
  the host running Cherenkov.
