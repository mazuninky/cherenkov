//! The [`AclChecker`] extension trait.
//!
//! ACL evaluation lives in the hub (not the transport) so every transport
//! shares the same enforcement path. Implementations are consulted on
//! [`crate::Hub::handle_subscribe`] and [`crate::Hub::handle_publish`]
//! after authentication; a denying decision short-circuits the channel
//! kind and broker exactly the way schema rejection does.
//!
//! Concrete implementations live in dedicated crates (`cherenkov-auth`)
//! so the core never pulls in glob matchers or rule engines.

use async_trait::async_trait;
use thiserror::Error;

use crate::SessionClaims;

/// The kind of action a session is attempting on a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AclAction {
    /// `Subscribe` to a channel.
    Subscribe,
    /// `Publish` to a channel.
    Publish,
}

impl AclAction {
    /// Stable string representation, suitable for log fields and rule
    /// configuration files.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Subscribe => "subscribe",
            Self::Publish => "publish",
        }
    }
}

/// Decision returned by an [`AclChecker`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AclDecision {
    /// The action is permitted.
    Allow,
    /// The action is forbidden.
    Deny,
}

/// Errors an [`AclChecker`] may surface.
#[derive(Debug, Error)]
pub enum AclError {
    /// The session is not allowed to perform `action` on `channel`. The
    /// `reason` is human-readable and safe to forward to the client.
    #[error("session `{subject}` denied {action} on `{channel}`: {reason}")]
    Denied {
        /// Authenticated subject.
        subject: String,
        /// Stringified action ("subscribe" or "publish").
        action: &'static str,
        /// Channel name.
        channel: String,
        /// Human-readable reason — payload-free.
        reason: String,
    },
    /// An implementation-specific failure (rule engine crashed, etc.).
    #[error("acl checker error: {0}")]
    Other(String),
}

/// Extension trait: gates `Subscribe` / `Publish` on the configured ACL.
#[async_trait]
pub trait AclChecker: Send + Sync + 'static {
    /// Stable identifier for this ACL backend, used in metrics and logs
    /// (e.g. `"namespace"`, `"allow-all"`).
    fn name(&self) -> &'static str;

    /// Evaluate `claims` for `action` against `channel`.
    ///
    /// Returning `Ok(AclDecision::Deny)` is equivalent to returning
    /// `Err(AclError::Denied { .. })` — both surface as a denial to the
    /// hub. Implementations should prefer the latter so a rich human
    /// reason reaches the client.
    async fn check(
        &self,
        claims: &SessionClaims,
        action: AclAction,
        channel: &str,
    ) -> Result<AclDecision, AclError>;
}

/// ACL checker that allows every action. Default for hubs built without
/// an explicit ACL.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowAllAcl;

#[async_trait]
impl AclChecker for AllowAllAcl {
    fn name(&self) -> &'static str {
        "allow-all"
    }

    async fn check(
        &self,
        _claims: &SessionClaims,
        _action: AclAction,
        _channel: &str,
    ) -> Result<AclDecision, AclError> {
        Ok(AclDecision::Allow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allow_all_returns_allow() {
        let acl = AllowAllAcl;
        let decision = acl
            .check(
                &SessionClaims::anonymous(),
                AclAction::Subscribe,
                "rooms.lobby",
            )
            .await
            .expect("no error");
        assert_eq!(decision, AclDecision::Allow);
        assert_eq!(acl.name(), "allow-all");
    }

    #[test]
    fn action_as_str_is_stable() {
        assert_eq!(AclAction::Subscribe.as_str(), "subscribe");
        assert_eq!(AclAction::Publish.as_str(), "publish");
    }
}
