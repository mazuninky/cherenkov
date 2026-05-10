## 1. Wire protocol extension

- [x] 1.1 Append `Connect` and `ConnectOk` messages to `proto/v1.proto`,
       wired into `ClientFrame`/`ServerFrame` oneofs as variants 4/5.
- [x] 1.2 Add `ErrorCode::InvalidToken=6`, `AclDenied=7`, `NotConnected=8`
       to `cherenkov-protocol/src/frame.rs`.
- [x] 1.3 Hand-write `Connect` / `ConnectOk` wrappers and From impls.
- [x] 1.4 Add proptest round-trip + insta snapshot for `Connect` and
       `ConnectOk`.

## 2. Hub core auth + ACL traits

- [x] 2.1 Add `cherenkov-core/src/auth.rs` with `Authenticator`,
       `SessionClaims`, `AuthError`, `AllowAllAuthenticator`.
- [x] 2.2 Add `cherenkov-core/src/acl.rs` with `AclChecker`, `AclAction`,
       `AclDecision`, `AclError`, `AllowAllAcl`.
- [x] 2.3 Re-export from `cherenkov-core/src/lib.rs`.
- [x] 2.4 Extend `HubError` with `Auth(AuthError)`, `Acl(AclError)`,
       `NotConnected`, `AlreadyConnected`.
- [x] 2.5 `Session` gains optional `claims: ArcSwap<Option<Arc<SessionClaims>>>`.
- [x] 2.6 `HubBuilder::with_authenticator`, `with_acl_checker`.
- [x] 2.7 `Hub::handle_connect(session, token) -> Result<ConnectOk>`.
- [x] 2.8 `Hub::handle_subscribe` / `handle_publish` consult the ACL
       checker. When auth is non-trivial, both fail with `NotConnected`
       if claims are still unset.
- [x] 2.9 Unit tests: connect happy path, bad token, ACL allow + deny,
       not-connected gating.

## 3. JWT authenticator + ACL impl

- [x] 3.1 Implement `cherenkov-auth/src/jwt.rs` with `JwtAuthenticator`
       (HS256 default), `JwtAuthBuilder`, decode via `jsonwebtoken`.
- [x] 3.2 Implement `cherenkov-auth/src/namespace_acl.rs` with
       `NamespaceAcl`, `AclRule`, `AclMatch`, glob via `globset`.
- [x] 3.3 Re-export from `cherenkov-auth/src/lib.rs`.
- [x] 3.4 Unit tests: HS256 verify, expired token rejected, audience
       mismatch rejected, ACL allow / deny / wildcard / fall-through.

## 4. WS transport wiring

- [x] 4.1 Dispatch `ClientFrame::Connect` to `Hub::handle_connect`.
- [x] 4.2 Map `HubError::Auth` → `ErrorCode::InvalidToken`,
       `HubError::Acl` → `ErrorCode::AclDenied`,
       `HubError::NotConnected` → `ErrorCode::NotConnected`.
- [x] 4.3 ConnectOk delivered as `ServerFrame::ConnectOk`.

## 5. Server config + composition

- [x] 5.1 Add `AuthConfig` (`hmac_secret`, `audiences`, `issuer`) and
       `AclConfig { rules: Vec<AclRule> }` to `cherenkov-server::config`.
- [x] 5.2 In `app::run_with_listener` build `JwtAuthenticator` +
       `NamespaceAcl` from config; wire to hub; default to
       `AllowAllAuthenticator` / `AllowAllAcl` when sections absent.
- [x] 5.3 Re-export new types from `cherenkov-server::lib`.

## 6. Integration test

- [x] 6.1 `cherenkov-server/tests/auth.rs` covering bad token,
       not-connected, ACL deny, ACL allow round-trip.

## 7. Documentation

- [x] 7.1 ADR `0004-auth-and-acl.md`.
- [x] 7.2 Update `docs/adr/README.md` to list ADR 0004.

## 8. Definition of Done

- [x] 8.1 `cargo fmt --all -- --check` exits 0
- [x] 8.2 `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits 0
- [x] 8.3 `cargo test --workspace --all-features` exits 0 (108 tests, 41 suites)
- [x] 8.4 `cargo doc --workspace --no-deps` builds with zero warnings
- [x] 8.5 `cargo deny check` exits 0

## Notes

* **MSRV bump 1.86 → 1.88.** `jsonwebtoken` transitively pulls
  `simple_asn1` → `time 0.3.47`, which requires Rust 1.88. Same precedent
  as M0 (1.83 → 1.85) and M2 (1.85 → 1.86). Captured in the M3-M7
  squash-merge commit body.

* **`deny.toml` allowlist additions.**
  - `CDLA-Permissive-2.0` for `webpki-roots` (CCADB data, transitive via
    `fred` Redis client).
  - Ignore `RUSTSEC-2025-0057` (`fxhash` unmaintained; pulled by
    `automerge`, no upstream fix yet).
