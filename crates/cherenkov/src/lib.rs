//! Cherenkov: a self-hosted, language-agnostic real-time messaging server in Rust.
//!
//! This is the facade crate. It re-exports the most common types from
//! [`cherenkov_core`] and [`cherenkov_protocol`] under a single namespace
//! so library consumers can pull in one dependency. Additional pieces
//! (transports, brokers, channel kinds, auth, admin) are gated behind
//! cargo features and re-exported from sub-modules.
//!
//! # Default features
//!
//! `pubsub` (in-memory channel kind), `ws` (WebSocket transport), and
//! `schema` (JSON Schema validator). Other backends are opt-in:
//!
//! ```text
//! cherenkov = { version = "*", features = ["full"] }   # everything
//! cherenkov = { version = "*", features = ["sse", "broker-redis"] }
//! ```

pub use cherenkov_core as core;
pub use cherenkov_protocol as protocol;

#[cfg(feature = "crdt")]
pub use cherenkov_channel_crdt as channel_crdt;
#[cfg(feature = "pubsub")]
pub use cherenkov_channel_pubsub as channel_pubsub;

#[cfg(feature = "sse")]
pub use cherenkov_transport_sse as transport_sse;
#[cfg(feature = "ws")]
pub use cherenkov_transport_ws as transport_ws;
#[cfg(feature = "wt")]
pub use cherenkov_transport_wt as transport_wt;

#[cfg(feature = "auth")]
pub use cherenkov_auth as auth;
#[cfg(feature = "schema")]
pub use cherenkov_schema as schema;

#[cfg(feature = "broker-memory")]
pub use cherenkov_broker as broker_memory;
#[cfg(feature = "broker-nats")]
pub use cherenkov_broker_nats as broker_nats;
#[cfg(feature = "broker-redis")]
pub use cherenkov_broker_redis as broker_redis;

#[cfg(feature = "admin")]
pub use cherenkov_admin as admin;

/// Convenience re-exports of the most-frequently-touched core types.
pub mod prelude {
    pub use cherenkov_core::{
        AclAction, AclChecker, AclDecision, AclError, AllowAllAcl, AllowAllAuthenticator,
        AllowAllValidator, AuthError, Authenticator, Broker, BrokerError, ChannelCursor,
        ChannelError, ChannelKind, Hub, HubBuilder, HubError, SchemaError, SchemaValidator,
        Session, SessionClaims, SessionId, SessionRegistry, Transport, TransportError,
    };
    pub use cherenkov_protocol::{
        ClientFrame, Connect, ConnectOk, ErrorCode, ProtocolError, Publication, Publish,
        ServerFrame, Subscribe, SubscribeOk, Unsubscribe, UnsubscribeOk, decode_client,
        decode_server, encode_client, encode_server,
    };
}
