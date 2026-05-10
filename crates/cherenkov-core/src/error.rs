//! [`HubError`]: the error type surfaced from [`crate::Hub`] entry points.

use thiserror::Error;

use crate::{AclError, AuthError, BrokerError, ChannelError, SchemaError};

/// Errors that the hub may surface to a transport when handling a frame.
///
/// Each variant carries enough context to debug without reconstructing a
/// stack trace, per `docs/plan.md` §4.3.
#[derive(Debug, Error)]
pub enum HubError {
    /// The session id referenced an unknown or already-closed session.
    #[error("session {session_id} is not registered or has been closed")]
    SessionGone {
        /// The session id that was looked up.
        session_id: u64,
    },
    /// The hub has no [`crate::ChannelKind`] configured to handle this
    /// channel name.
    #[error("no channel kind registered for `{channel}`")]
    UnknownChannel {
        /// The channel name that could not be routed.
        channel: String,
    },
    /// The session is already subscribed to this channel.
    #[error("session {session_id} is already subscribed to `{channel}`")]
    AlreadySubscribed {
        /// The session id.
        session_id: u64,
        /// The channel that was double-subscribed.
        channel: String,
    },
    /// The session was not subscribed to this channel when it tried to
    /// unsubscribe.
    #[error("session {session_id} is not subscribed to `{channel}`")]
    NotSubscribed {
        /// The session id.
        session_id: u64,
        /// The channel that was double-unsubscribed.
        channel: String,
    },
    /// A [`crate::ChannelKind`] surfaced an error.
    #[error("channel kind error: {0}")]
    Channel(#[from] ChannelError),
    /// A [`crate::Broker`] surfaced an error.
    #[error("broker error: {0}")]
    Broker(#[from] BrokerError),
    /// A [`crate::SchemaValidator`] rejected a publication.
    #[error("schema validation failed: {0}")]
    Schema(#[from] SchemaError),
    /// A [`crate::Authenticator`] rejected the connect token.
    #[error("authentication failed: {0}")]
    Auth(#[from] AuthError),
    /// A [`crate::AclChecker`] denied the action.
    #[error("acl check failed: {0}")]
    Acl(#[from] AclError),
    /// The session attempted to subscribe / publish before successfully
    /// completing the `Connect` handshake. Only surfaced when the
    /// configured authenticator is non-anonymous.
    #[error("session {session_id} has not connected")]
    NotConnected {
        /// The session id that lacked claims.
        session_id: u64,
    },
    /// The session sent a second `Connect` frame after the first one
    /// succeeded. Re-authentication is not supported in v1.
    #[error("session {session_id} is already connected")]
    AlreadyConnected {
        /// The session id that double-connected.
        session_id: u64,
    },
}
