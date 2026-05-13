//! Core Cherenkov primitives: the [`Hub`], the [`Session`] registry, and the
//! three pluggable extension traits ([`ChannelKind`], [`Transport`], [`Broker`]).
//!
//! The crate is intentionally minimal: it has no concrete kind, transport, or
//! broker implementations. Concrete pieces live in their own crates and are
//! plugged in by [`HubBuilder`] at startup.

pub mod acl;
pub mod auth;
pub mod broker;
pub mod channel;
mod error;
pub mod hub;
pub mod schema;
pub mod session;
pub mod transport;

pub use acl::{AclAction, AclChecker, AclDecision, AclError, AllowAllAcl};
pub use auth::{AllowAllAuthenticator, AuthError, Authenticator, SessionClaims};
pub use broker::{Broker, BrokerError, BrokerStream};
pub use channel::{ChannelCursor, ChannelError, ChannelKind};
pub use error::HubError;
pub use hub::{Hub, HubBuilder, HubBuilt, namespace_of};
pub use schema::{AllowAllValidator, SchemaError, SchemaValidator};
pub use session::{Session, SessionId, SessionRegistry};
pub use transport::{Transport, TransportError};
