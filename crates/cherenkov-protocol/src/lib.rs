//! Cherenkov wire protocol (v1).
//!
//! The on-the-wire schema is defined in [`proto/v1.proto`] and compiled to
//! Rust by `prost` at build time. We deliberately keep the generated types
//! private and expose hand-written, ergonomic wrappers in [`frame`]. This
//! gives downstream crates a stable, idiomatic API even if we change codegen
//! tooling later.
//!
//! # Encoding
//!
//! Frames are encoded as standard length-prefixed Protobuf messages — but
//! since each WebSocket binary frame already carries an explicit length,
//! [`encode_client`] and [`encode_server`] write the bare Protobuf body
//! without a prefix. The transport is responsible for framing.
//!
//! # Example
//!
//! ```
//! use cherenkov_protocol::{ClientFrame, Subscribe, decode_client, encode_client};
//!
//! let frame = ClientFrame::Subscribe(Subscribe {
//!     request_id: 42,
//!     channel: "rooms.lobby".to_owned(),
//!     since_offset: 0,
//! });
//!
//! let bytes = encode_client(&frame);
//! let round_tripped = decode_client(&bytes).expect("valid frame");
//! assert_eq!(frame, round_tripped);
//! ```
//!
//! [`proto/v1.proto`]: https://github.com/mazuninky/cherenkov/blob/main/crates/cherenkov-protocol/proto/v1.proto

pub mod frame;

pub use frame::{
    decode_client, decode_publication, decode_server, encode_client, encode_publication,
    encode_server, ClientFrame, Connect, ConnectOk, DecodeError, ErrorCode, ProtocolError,
    Publication, Publish, ServerFrame, Subscribe, SubscribeOk, Unsubscribe, UnsubscribeOk,
};

/// `prost`-generated Rust types from `proto/v1.proto`.
///
/// Kept private and re-exported only via [`frame`] wrappers so we can refactor
/// codegen tooling without touching the public API.
#[allow(clippy::pedantic, clippy::nursery, clippy::style, missing_docs)]
mod proto {
    include!(concat!(env!("OUT_DIR"), "/cherenkov.v1.rs"));
}
