//! JWT-backed [`Authenticator`] implementation.
//!
//! HS256 is the default, with the secret supplied at builder time. The
//! `permissions` claim from the token is forwarded into
//! [`SessionClaims::permissions`]; the standard `aud`, `iss`, and `exp`
//! claims are validated by `jsonwebtoken`.

use async_trait::async_trait;
use cherenkov_core::{AuthError, Authenticator, SessionClaims};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;

/// JWT signing algorithm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JwtAlgorithm {
    /// HMAC-SHA256, the default.
    #[default]
    Hs256,
    /// HMAC-SHA384.
    Hs384,
    /// HMAC-SHA512.
    Hs512,
}

impl From<JwtAlgorithm> for Algorithm {
    fn from(value: JwtAlgorithm) -> Self {
        match value {
            JwtAlgorithm::Hs256 => Algorithm::HS256,
            JwtAlgorithm::Hs384 => Algorithm::HS384,
            JwtAlgorithm::Hs512 => Algorithm::HS512,
        }
    }
}

/// Claims layout the authenticator expects from the token.
///
/// Standard registered claims (`sub`, `aud`, `iss`, `exp`) plus a free-form
/// `permissions` array. Unknown fields are ignored.
#[derive(Debug, Deserialize)]
struct CherenkovClaims {
    sub: String,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    exp: u64,
}

/// JWT-backed [`Authenticator`].
pub struct JwtAuthenticator {
    key: DecodingKey,
    validation: Validation,
}

/// Builder for [`JwtAuthenticator`].
#[derive(Default)]
pub struct JwtAuthBuilder {
    secret: Option<Vec<u8>>,
    algorithm: JwtAlgorithm,
    audiences: Vec<String>,
    issuer: Option<String>,
}

impl JwtAuthBuilder {
    /// Set the HMAC secret used to verify the token signature.
    #[must_use]
    pub fn with_hmac_secret(mut self, secret: impl Into<Vec<u8>>) -> Self {
        self.secret = Some(secret.into());
        self
    }

    /// Override the algorithm (default: HS256).
    #[must_use]
    pub fn with_algorithm(mut self, alg: JwtAlgorithm) -> Self {
        self.algorithm = alg;
        self
    }

    /// Add an accepted `aud` value. Tokens whose `aud` is not in this set
    /// are rejected. If no audience is configured, audience verification
    /// is disabled.
    #[must_use]
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audiences.push(audience.into());
        self
    }

    /// Restrict accepted tokens to those issued by `issuer`.
    #[must_use]
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Build the authenticator.
    ///
    /// # Panics
    ///
    /// Panics if no HMAC secret was supplied. Asymmetric keys are reserved
    /// for a follow-up change.
    #[must_use]
    pub fn build(self) -> JwtAuthenticator {
        let secret = self
            .secret
            .expect("JwtAuthBuilder requires an HMAC secret via with_hmac_secret");
        let mut validation = Validation::new(self.algorithm.into());
        if !self.audiences.is_empty() {
            validation.set_audience(&self.audiences);
        } else {
            validation.validate_aud = false;
        }
        if let Some(iss) = self.issuer {
            validation.set_issuer(&[iss]);
        }
        JwtAuthenticator {
            key: DecodingKey::from_secret(&secret),
            validation,
        }
    }
}

impl JwtAuthenticator {
    /// Construct a builder.
    #[must_use]
    pub fn builder() -> JwtAuthBuilder {
        JwtAuthBuilder::default()
    }
}

#[async_trait]
impl Authenticator for JwtAuthenticator {
    fn name(&self) -> &'static str {
        "jwt"
    }

    fn allow_anonymous(&self) -> bool {
        false
    }

    async fn authenticate(&self, token: &str) -> Result<SessionClaims, AuthError> {
        let data = decode::<CherenkovClaims>(token, &self.key, &self.validation).map_err(|e| {
            AuthError::InvalidToken {
                reason: e.to_string(),
            }
        })?;
        Ok(SessionClaims {
            subject: data.claims.sub,
            permissions: data.claims.permissions,
            expires_at: data.claims.exp,
        })
    }
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::json;

    use super::*;

    fn sign(secret: &[u8], body: serde_json::Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &body,
            &EncodingKey::from_secret(secret),
        )
        .expect("sign")
    }

    #[tokio::test]
    async fn hs256_round_trip() {
        let secret = b"super-secret-key";
        let auth = JwtAuthenticator::builder()
            .with_hmac_secret(secret.to_vec())
            .with_audience("cherenkov")
            .build();
        let token = sign(
            secret,
            json!({
                "sub": "alice",
                "aud": "cherenkov",
                "exp": 9_999_999_999u64,
                "permissions": ["publish", "subscribe"],
            }),
        );
        let claims = auth.authenticate(&token).await.expect("verify ok");
        assert_eq!(claims.subject, "alice");
        assert!(claims.permissions.iter().any(|p| p == "publish"));
        assert_eq!(auth.name(), "jwt");
        assert!(!auth.allow_anonymous());
    }

    #[tokio::test]
    async fn invalid_signature_rejected() {
        let auth = JwtAuthenticator::builder()
            .with_hmac_secret(b"correct-key".to_vec())
            .build();
        let token = sign(
            b"wrong-key",
            json!({
                "sub": "alice",
                "exp": 9_999_999_999u64,
            }),
        );
        let err = auth.authenticate(&token).await.expect_err("must reject");
        assert!(matches!(err, AuthError::InvalidToken { .. }));
    }

    #[tokio::test]
    async fn audience_mismatch_rejected() {
        let secret = b"abc";
        let auth = JwtAuthenticator::builder()
            .with_hmac_secret(secret.to_vec())
            .with_audience("cherenkov")
            .build();
        let token = sign(
            secret,
            json!({
                "sub": "alice",
                "aud": "other",
                "exp": 9_999_999_999u64,
            }),
        );
        let err = auth.authenticate(&token).await.expect_err("must reject");
        assert!(matches!(err, AuthError::InvalidToken { .. }));
    }

    #[tokio::test]
    async fn expired_token_rejected() {
        let secret = b"abc";
        let auth = JwtAuthenticator::builder()
            .with_hmac_secret(secret.to_vec())
            .build();
        let token = sign(
            secret,
            json!({
                "sub": "alice",
                "exp": 1u64,
            }),
        );
        let err = auth.authenticate(&token).await.expect_err("must reject");
        assert!(matches!(err, AuthError::InvalidToken { .. }));
    }
}
