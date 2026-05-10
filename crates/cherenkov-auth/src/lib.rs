//! JWT authentication and per-namespace ACL for Cherenkov.
//!
//! This crate is the production complement to the no-op
//! [`cherenkov_core::AllowAllAuthenticator`] / [`cherenkov_core::AllowAllAcl`]
//! defaults. The hub stays neutral; concrete credential validation and
//! glob-based rule evaluation live here so the core never pulls in a JWT
//! library or a glob matcher.
//!
//! # Quick start
//!
//! ```
//! use std::sync::Arc;
//! use cherenkov_core::{HubBuilder, AllowAllAuthenticator};
//! use cherenkov_auth::{JwtAuthenticator, NamespaceAcl, AclRule, AclMatch};
//!
//! let _auth = JwtAuthenticator::builder()
//!     .with_hmac_secret(b"hunter2")
//!     .with_audience("cherenkov")
//!     .build();
//!
//! let _acl = NamespaceAcl::builder()
//!     .with_rule(AclRule::allow("rooms.*", AclMatch::any()))
//!     .with_rule(AclRule::deny("admin.*", AclMatch::any()))
//!     .build();
//! ```

pub mod jwt;
pub mod namespace_acl;

pub use jwt::{JwtAlgorithm, JwtAuthBuilder, JwtAuthenticator};
pub use namespace_acl::{AclMatch, AclRule, NamespaceAcl, NamespaceAclBuilder};
