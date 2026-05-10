//! Glob-based ACL implementation.
//!
//! Rules are evaluated in declaration order. The first rule whose
//! `(action, channel)` glob matches the request decides — `Allow` returns
//! [`AclDecision::Allow`], `Deny` returns [`AclDecision::Deny`]. If no
//! rule matches the request is denied (deny by default).
//!
//! Matchers may also restrict by subject pattern. The empty rule list
//! denies everything; if you want allow-by-default semantics, register
//! [`cherenkov_core::AllowAllAcl`] instead.

use async_trait::async_trait;
use cherenkov_core::{AclAction, AclChecker, AclDecision, AclError, SessionClaims};
use globset::{Glob, GlobMatcher};
use serde::Deserialize;

/// What action(s) a rule applies to.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AclMatch {
    /// Match `Subscribe` only.
    Subscribe,
    /// Match `Publish` only.
    Publish,
    /// Match both.
    #[default]
    Any,
}

impl AclMatch {
    /// True if this matcher covers `action`.
    #[must_use]
    pub fn covers(self, action: AclAction) -> bool {
        matches!(
            (self, action),
            (Self::Any, _)
                | (Self::Subscribe, AclAction::Subscribe)
                | (Self::Publish, AclAction::Publish)
        )
    }

    /// Convenience constructor.
    #[must_use]
    pub fn any() -> Self {
        Self::Any
    }
}

/// Rule effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Allow,
    Deny,
}

/// One ACL rule.
#[derive(Clone, Debug)]
pub struct AclRule {
    effect: Effect,
    channel: GlobMatcher,
    subject: Option<GlobMatcher>,
    action: AclMatch,
}

impl AclRule {
    fn new(
        effect: Effect,
        channel_pattern: &str,
        action: AclMatch,
    ) -> Result<Self, globset::Error> {
        let channel = Glob::new(channel_pattern)?.compile_matcher();
        Ok(Self {
            effect,
            channel,
            subject: None,
            action,
        })
    }

    /// Build an `Allow` rule for the given `channel` glob.
    ///
    /// # Panics
    ///
    /// Panics if `channel_pattern` is not a valid glob. For runtime-built
    /// configs, use [`AclRule::try_allow`].
    #[must_use]
    pub fn allow(channel_pattern: &str, action: AclMatch) -> Self {
        Self::new(Effect::Allow, channel_pattern, action).expect("static glob pattern must compile")
    }

    /// Build a `Deny` rule for the given `channel` glob.
    ///
    /// # Panics
    ///
    /// Panics if `channel_pattern` is not a valid glob. For runtime-built
    /// configs, use [`AclRule::try_deny`].
    #[must_use]
    pub fn deny(channel_pattern: &str, action: AclMatch) -> Self {
        Self::new(Effect::Deny, channel_pattern, action).expect("static glob pattern must compile")
    }

    /// Fallible counterpart of [`AclRule::allow`].
    ///
    /// # Errors
    ///
    /// Returns the underlying [`globset::Error`] if `channel_pattern` is
    /// not a valid glob.
    pub fn try_allow(channel_pattern: &str, action: AclMatch) -> Result<Self, globset::Error> {
        Self::new(Effect::Allow, channel_pattern, action)
    }

    /// Fallible counterpart of [`AclRule::deny`].
    ///
    /// # Errors
    ///
    /// Returns the underlying [`globset::Error`] if `channel_pattern` is
    /// not a valid glob.
    pub fn try_deny(channel_pattern: &str, action: AclMatch) -> Result<Self, globset::Error> {
        Self::new(Effect::Deny, channel_pattern, action)
    }

    /// Restrict this rule to subjects matching `subject_pattern`.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`globset::Error`] if `subject_pattern` is
    /// not a valid glob.
    pub fn with_subject(mut self, subject_pattern: &str) -> Result<Self, globset::Error> {
        self.subject = Some(Glob::new(subject_pattern)?.compile_matcher());
        Ok(self)
    }

    fn matches(&self, claims: &SessionClaims, action: AclAction, channel: &str) -> bool {
        if !self.action.covers(action) {
            return false;
        }
        if let Some(subj) = &self.subject {
            if !subj.is_match(&claims.subject) {
                return false;
            }
        }
        self.channel.is_match(channel)
    }
}

/// Glob-based [`AclChecker`].
#[derive(Clone, Debug, Default)]
pub struct NamespaceAcl {
    rules: Vec<AclRule>,
    default_allow: bool,
}

/// Builder for [`NamespaceAcl`].
#[derive(Default)]
pub struct NamespaceAclBuilder {
    rules: Vec<AclRule>,
    default_allow: bool,
}

impl NamespaceAclBuilder {
    /// Append a rule. Rules are evaluated in registration order.
    #[must_use]
    pub fn with_rule(mut self, rule: AclRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Set the default decision when no rule matches. Default is `false`
    /// (deny).
    #[must_use]
    pub fn default_allow(mut self, allow: bool) -> Self {
        self.default_allow = allow;
        self
    }

    /// Finalize.
    #[must_use]
    pub fn build(self) -> NamespaceAcl {
        NamespaceAcl {
            rules: self.rules,
            default_allow: self.default_allow,
        }
    }
}

impl NamespaceAcl {
    /// Construct a builder.
    #[must_use]
    pub fn builder() -> NamespaceAclBuilder {
        NamespaceAclBuilder::default()
    }
}

#[async_trait]
impl AclChecker for NamespaceAcl {
    fn name(&self) -> &'static str {
        "namespace"
    }

    async fn check(
        &self,
        claims: &SessionClaims,
        action: AclAction,
        channel: &str,
    ) -> Result<AclDecision, AclError> {
        for rule in &self.rules {
            if rule.matches(claims, action, channel) {
                return Ok(match rule.effect {
                    Effect::Allow => AclDecision::Allow,
                    Effect::Deny => AclDecision::Deny,
                });
            }
        }
        Ok(if self.default_allow {
            AclDecision::Allow
        } else {
            AclDecision::Deny
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> SessionClaims {
        SessionClaims {
            subject: "alice".to_owned(),
            permissions: vec![],
            expires_at: 0,
        }
    }

    #[tokio::test]
    async fn first_match_wins() {
        let acl = NamespaceAcl::builder()
            .with_rule(AclRule::allow("rooms.*", AclMatch::Any))
            .with_rule(AclRule::deny("rooms.lobby", AclMatch::Any))
            .build();
        let d = acl
            .check(&alice(), AclAction::Subscribe, "rooms.lobby")
            .await
            .unwrap();
        assert_eq!(d, AclDecision::Allow);
    }

    #[tokio::test]
    async fn deny_overrides_when_listed_first() {
        let acl = NamespaceAcl::builder()
            .with_rule(AclRule::deny("admin.*", AclMatch::Any))
            .with_rule(AclRule::allow("*", AclMatch::Any))
            .build();
        let allow = acl
            .check(&alice(), AclAction::Publish, "rooms.lobby")
            .await
            .unwrap();
        assert_eq!(allow, AclDecision::Allow);
        let deny = acl
            .check(&alice(), AclAction::Publish, "admin.users")
            .await
            .unwrap();
        assert_eq!(deny, AclDecision::Deny);
    }

    #[tokio::test]
    async fn empty_rules_deny_by_default() {
        let acl = NamespaceAcl::builder().build();
        let d = acl
            .check(&alice(), AclAction::Subscribe, "rooms.lobby")
            .await
            .unwrap();
        assert_eq!(d, AclDecision::Deny);
    }

    #[tokio::test]
    async fn default_allow_flag_inverts_fall_through() {
        let acl = NamespaceAcl::builder().default_allow(true).build();
        let d = acl
            .check(&alice(), AclAction::Publish, "anything")
            .await
            .unwrap();
        assert_eq!(d, AclDecision::Allow);
    }

    #[tokio::test]
    async fn subject_filter_narrows_the_match() {
        let acl = NamespaceAcl::builder()
            .with_rule(
                AclRule::allow("rooms.*", AclMatch::Subscribe)
                    .with_subject("admin*")
                    .unwrap(),
            )
            .default_allow(false)
            .build();
        let alice = alice();
        let mut admin = alice.clone();
        admin.subject = "admin-1".to_owned();

        let denied = acl
            .check(&alice, AclAction::Subscribe, "rooms.lobby")
            .await
            .unwrap();
        assert_eq!(denied, AclDecision::Deny);
        let allowed = acl
            .check(&admin, AclAction::Subscribe, "rooms.lobby")
            .await
            .unwrap();
        assert_eq!(allowed, AclDecision::Allow);
    }

    #[tokio::test]
    async fn action_match_narrows_to_publish_only() {
        let acl = NamespaceAcl::builder()
            .with_rule(AclRule::allow("rooms.*", AclMatch::Publish))
            .build();
        let pub_d = acl
            .check(&alice(), AclAction::Publish, "rooms.lobby")
            .await
            .unwrap();
        assert_eq!(pub_d, AclDecision::Allow);
        let sub_d = acl
            .check(&alice(), AclAction::Subscribe, "rooms.lobby")
            .await
            .unwrap();
        assert_eq!(sub_d, AclDecision::Deny);
    }
}
