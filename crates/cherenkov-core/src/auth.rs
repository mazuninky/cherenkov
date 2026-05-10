//! The [`Authenticator`] extension trait.
//!
//! An [`Authenticator`] turns the opaque token from a `Connect` frame into
//! [`SessionClaims`]. The hub stores the claims on the session and
//! consults them when evaluating ACLs (see [`crate::AclChecker`]).
//!
//! Concrete implementations live in dedicated crates (`cherenkov-auth`)
//! so the core never pulls in `jsonwebtoken`, `oauth2`, or any other
//! credential backend.

use async_trait::async_trait;
use thiserror::Error;

/// Authenticated principal extracted from a `Connect` token.
///
/// `subject` is the opaque identifier the [`crate::AclChecker`] sees;
/// `permissions` is a free-form set of strings the ACL checker may match
/// against (e.g. JWT scopes, roles). `expires_at` is the Unix timestamp
/// the credential expires at, with `0` meaning "no expiry known".
#[derive(Clone, Debug, Default)]
pub struct SessionClaims {
    /// Authenticated principal (typically the JWT `sub` claim).
    pub subject: String,
    /// Free-form permission strings the ACL checker may consult.
    pub permissions: Vec<String>,
    /// Unix timestamp at which the credential expires; `0` means none.
    pub expires_at: u64,
}

impl SessionClaims {
    /// Construct claims for an anonymous session.
    #[must_use]
    pub fn anonymous() -> Self {
        Self {
            subject: "anonymous".to_owned(),
            permissions: Vec::new(),
            expires_at: 0,
        }
    }
}

/// Errors an [`Authenticator`] may surface.
#[derive(Debug, Error)]
pub enum AuthError {
    /// The token did not pass validation (signature, audience, issuer,
    /// or expiry mismatch). `reason` is human-readable and safe to send
    /// to the client.
    #[error("invalid token: {reason}")]
    InvalidToken {
        /// Human-readable reason — never includes the token bytes.
        reason: String,
    },
    /// An implementation-specific failure (key load failure, IO error).
    #[error("authenticator error: {0}")]
    Other(String),
}

/// Extension trait: validates a bearer token, returns [`SessionClaims`].
///
/// Implementations must be cheap to clone (stored as `Arc<dyn _>` on the
/// hub) and cancel-safe.
#[async_trait]
pub trait Authenticator: Send + Sync + 'static {
    /// Stable identifier for this authenticator backend, used in
    /// metrics and structured logs (e.g. `"jwt-hs256"`, `"allow-all"`).
    fn name(&self) -> &'static str;

    /// Returns `true` if this authenticator may be replaced by an
    /// implicit anonymous session — that is, the hub may skip the
    /// `Connect`-required gate entirely.
    ///
    /// The default no-op authenticator returns `true`; a real
    /// [`Authenticator`] should return `false` so the hub enforces
    /// the connect handshake.
    fn allow_anonymous(&self) -> bool {
        false
    }

    /// Validate `token` and return the corresponding [`SessionClaims`].
    async fn authenticate(&self, token: &str) -> Result<SessionClaims, AuthError>;
}

/// Authenticator that accepts every token (including the empty string)
/// as an anonymous session. Default for hubs built without an explicit
/// authenticator.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAllAuthenticator;

#[async_trait]
impl Authenticator for AllowAllAuthenticator {
    fn name(&self) -> &'static str {
        "allow-all"
    }

    fn allow_anonymous(&self) -> bool {
        true
    }

    async fn authenticate(&self, _token: &str) -> Result<SessionClaims, AuthError> {
        Ok(SessionClaims::anonymous())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allow_all_yields_anonymous_claims() {
        let auth = AllowAllAuthenticator;
        let claims = auth.authenticate("").await.expect("no error");
        assert_eq!(claims.subject, "anonymous");
        assert!(auth.allow_anonymous());
        assert_eq!(auth.name(), "allow-all");
    }
}
