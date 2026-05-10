//! Ergonomic wrappers around the `prost`-generated v1 wire types.
//!
//! Wrapper types (this module) are owned, public, and the only things that
//! cross crate boundaries; the underlying `prost`-generated types live in a
//! private `proto` module. Conversions between the two are infallible in
//! the wrapper-to-prost direction and fallible ([`DecodeError`]) only when
//! decoding from raw bytes.

use bytes::Bytes;
use prost::Message;
use thiserror::Error;

use crate::proto;

/// Top-level frame the client sends to the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientFrame {
    /// Subscribe to a channel.
    Subscribe(Subscribe),
    /// Unsubscribe from a channel.
    Unsubscribe(Unsubscribe),
    /// Publish a payload to a channel.
    Publish(Publish),
    /// Authenticate the session.
    Connect(Connect),
}

/// Top-level frame the server sends to the client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerFrame {
    /// Acknowledgement for a successful subscribe.
    SubscribeOk(SubscribeOk),
    /// Acknowledgement for a successful unsubscribe.
    UnsubscribeOk(UnsubscribeOk),
    /// A publication delivered to a subscriber.
    Publication(Publication),
    /// An error reply tied to a request id.
    Error(ProtocolError),
    /// Acknowledgement for a successful connect.
    ConnectOk(ConnectOk),
}

/// Subscribe to `channel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscribe {
    /// Echoed back in the matching [`SubscribeOk`].
    pub request_id: u64,
    /// Target channel name.
    pub channel: String,
    /// Recovery cursor: when non-zero, the server replays publications
    /// with `offset > since_offset` before forwarding live publications.
    /// Implementations whose channel kind / history layer does not
    /// support replay treat this field as informational.
    pub since_offset: u64,
}

/// Unsubscribe from `channel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsubscribe {
    /// Echoed back in the matching [`UnsubscribeOk`].
    pub request_id: u64,
    /// Target channel name.
    pub channel: String,
}

/// Publish opaque `data` into `channel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publish {
    /// Echoed back in the error response, if any.
    pub request_id: u64,
    /// Target channel name.
    pub channel: String,
    /// Opaque payload bytes. The server never inspects or logs these.
    pub data: Bytes,
}

/// Acknowledgement for a successful subscribe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscribeOk {
    /// Mirror of the request id from [`Subscribe`].
    pub request_id: u64,
    /// The channel that was subscribed.
    pub channel: String,
    /// Channel epoch at the moment of subscription.
    pub epoch: u64,
    /// Channel offset cursor at the moment of subscription.
    pub offset: u64,
}

/// Acknowledgement for a successful unsubscribe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsubscribeOk {
    /// Mirror of the request id from [`Unsubscribe`].
    pub request_id: u64,
    /// The channel that was unsubscribed.
    pub channel: String,
}

/// A publication delivered to a subscriber.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    /// Channel the publication originated from.
    pub channel: String,
    /// Monotonic offset assigned by the channel kind.
    pub offset: u64,
    /// Opaque payload bytes.
    pub data: Bytes,
}

/// Authenticate the session with a bearer token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connect {
    /// Echoed back in the matching [`ConnectOk`] or [`ProtocolError`].
    pub request_id: u64,
    /// Opaque bearer token. Validated by the configured authenticator;
    /// never logged.
    pub token: String,
}

/// Acknowledgement for a successful connect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectOk {
    /// Mirror of the request id from [`Connect`].
    pub request_id: u64,
    /// Authenticated principal (typically the JWT `sub` claim).
    pub subject: String,
    /// Unix timestamp (seconds) at which the credential expires; `0`
    /// means "no expiry known to the server".
    pub expires_at: u64,
}

/// An error reply tied to a request id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    /// The request id this error refers to, if any (`0` for unsolicited).
    pub request_id: u64,
    /// Numeric error code; see [`ErrorCode`].
    pub code: u32,
    /// Human-readable message — safe for inclusion in client logs.
    pub message: String,
}

/// Canonical error codes used by the wire protocol.
///
/// Codes are stable within v1; new variants only ever get appended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ErrorCode {
    /// The frame did not match the v1 schema.
    InvalidFrame = 1,
    /// The requested channel name is malformed or empty.
    InvalidChannel = 2,
    /// Requested operation is not authorized for this session.
    Unauthorized = 3,
    /// Internal server error; clients should retry with backoff.
    Internal = 4,
    /// The publish payload was rejected by the namespace's schema. The
    /// `message` field carries the human-readable reason (no payload).
    ValidationFailed = 5,
    /// The token supplied with `Connect` was invalid (expired, malformed,
    /// signature mismatch, audience or issuer mismatch).
    InvalidToken = 6,
    /// The session is not permitted to perform the requested action on
    /// the given channel.
    AclDenied = 7,
    /// The session attempted to subscribe / publish before sending a
    /// successful `Connect`.
    NotConnected = 8,
}

impl From<ErrorCode> for u32 {
    fn from(value: ErrorCode) -> Self {
        value as u32
    }
}

/// Error returned when decoding raw bytes into a [`ClientFrame`] or
/// [`ServerFrame`] fails.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// The buffer did not parse as a valid v1 Protobuf message.
    #[error("protobuf decode failed: {0}")]
    Prost(#[from] prost::DecodeError),
    /// A `ClientFrame` was decoded but its `kind` oneof was unset.
    #[error("client frame missing required `kind` oneof")]
    MissingClientKind,
    /// A `ServerFrame` was decoded but its `kind` oneof was unset.
    #[error("server frame missing required `kind` oneof")]
    MissingServerKind,
}

/// Encode a [`ClientFrame`] to a length-unprefixed Protobuf body.
#[must_use]
pub fn encode_client(frame: &ClientFrame) -> Bytes {
    let proto: proto::ClientFrame = frame.clone().into();
    let mut buf = Vec::with_capacity(proto.encoded_len());
    proto
        .encode(&mut buf)
        .expect("Vec<u8> never fails to grow under prost::encode");
    Bytes::from(buf)
}

/// Encode a [`ServerFrame`] to a length-unprefixed Protobuf body.
#[must_use]
pub fn encode_server(frame: &ServerFrame) -> Bytes {
    let proto: proto::ServerFrame = frame.clone().into();
    let mut buf = Vec::with_capacity(proto.encoded_len());
    proto
        .encode(&mut buf)
        .expect("Vec<u8> never fails to grow under prost::encode");
    Bytes::from(buf)
}

/// Encode a single [`Publication`] as a length-unprefixed Protobuf body.
///
/// Used by cross-node brokers (Redis, NATS) to pass publications between
/// nodes without wrapping them in a `ServerFrame` envelope.
#[must_use]
pub fn encode_publication(publication: &Publication) -> Bytes {
    let proto: proto::Publication = publication.clone().into();
    let mut buf = Vec::with_capacity(proto.encoded_len());
    proto
        .encode(&mut buf)
        .expect("Vec<u8> never fails to grow under prost::encode");
    Bytes::from(buf)
}

/// Decode a length-unprefixed Protobuf body into a [`Publication`].
///
/// # Errors
///
/// Returns [`DecodeError::Prost`] if the buffer is not a valid Protobuf
/// message.
pub fn decode_publication(bytes: &[u8]) -> Result<Publication, DecodeError> {
    let proto = proto::Publication::decode(bytes)?;
    Ok(proto.into())
}

/// Decode a length-unprefixed Protobuf body into a [`ClientFrame`].
///
/// # Errors
///
/// Returns [`DecodeError::Prost`] if the buffer is not a valid Protobuf
/// message, or [`DecodeError::MissingClientKind`] if it is structurally
/// valid but its `kind` oneof was not set.
pub fn decode_client(bytes: &[u8]) -> Result<ClientFrame, DecodeError> {
    let proto = proto::ClientFrame::decode(bytes)?;
    proto.try_into()
}

/// Decode a length-unprefixed Protobuf body into a [`ServerFrame`].
///
/// # Errors
///
/// Returns [`DecodeError::Prost`] if the buffer is not a valid Protobuf
/// message, or [`DecodeError::MissingServerKind`] if it is structurally
/// valid but its `kind` oneof was not set.
pub fn decode_server(bytes: &[u8]) -> Result<ServerFrame, DecodeError> {
    let proto = proto::ServerFrame::decode(bytes)?;
    proto.try_into()
}

// ---------------------------------------------------------------------------
// Wrapper -> proto
// ---------------------------------------------------------------------------

impl From<ClientFrame> for proto::ClientFrame {
    fn from(value: ClientFrame) -> Self {
        let kind = match value {
            ClientFrame::Subscribe(s) => proto::client_frame::Kind::Subscribe(s.into()),
            ClientFrame::Unsubscribe(u) => proto::client_frame::Kind::Unsubscribe(u.into()),
            ClientFrame::Publish(p) => proto::client_frame::Kind::Publish(p.into()),
            ClientFrame::Connect(c) => proto::client_frame::Kind::Connect(c.into()),
        };
        Self { kind: Some(kind) }
    }
}

impl From<ServerFrame> for proto::ServerFrame {
    fn from(value: ServerFrame) -> Self {
        let kind = match value {
            ServerFrame::SubscribeOk(s) => proto::server_frame::Kind::SubscribeOk(s.into()),
            ServerFrame::UnsubscribeOk(u) => proto::server_frame::Kind::UnsubscribeOk(u.into()),
            ServerFrame::Publication(p) => proto::server_frame::Kind::Publication(p.into()),
            ServerFrame::Error(e) => proto::server_frame::Kind::Error(e.into()),
            ServerFrame::ConnectOk(c) => proto::server_frame::Kind::ConnectOk(c.into()),
        };
        Self { kind: Some(kind) }
    }
}

impl From<Connect> for proto::Connect {
    fn from(value: Connect) -> Self {
        Self {
            request_id: value.request_id,
            token: value.token,
        }
    }
}

impl From<ConnectOk> for proto::ConnectOk {
    fn from(value: ConnectOk) -> Self {
        Self {
            request_id: value.request_id,
            subject: value.subject,
            expires_at: value.expires_at,
        }
    }
}

impl From<Subscribe> for proto::Subscribe {
    fn from(value: Subscribe) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
            since_offset: value.since_offset,
        }
    }
}

impl From<Unsubscribe> for proto::Unsubscribe {
    fn from(value: Unsubscribe) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
        }
    }
}

impl From<Publish> for proto::Publish {
    fn from(value: Publish) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
            data: value.data.to_vec(),
        }
    }
}

impl From<SubscribeOk> for proto::SubscribeOk {
    fn from(value: SubscribeOk) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
            epoch: value.epoch,
            offset: value.offset,
        }
    }
}

impl From<UnsubscribeOk> for proto::UnsubscribeOk {
    fn from(value: UnsubscribeOk) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
        }
    }
}

impl From<Publication> for proto::Publication {
    fn from(value: Publication) -> Self {
        Self {
            channel: value.channel,
            offset: value.offset,
            data: value.data.to_vec(),
        }
    }
}

impl From<ProtocolError> for proto::Error {
    fn from(value: ProtocolError) -> Self {
        Self {
            request_id: value.request_id,
            code: value.code,
            message: value.message,
        }
    }
}

// ---------------------------------------------------------------------------
// proto -> Wrapper
// ---------------------------------------------------------------------------

impl TryFrom<proto::ClientFrame> for ClientFrame {
    type Error = DecodeError;

    fn try_from(value: proto::ClientFrame) -> Result<Self, Self::Error> {
        let kind = value.kind.ok_or(DecodeError::MissingClientKind)?;
        Ok(match kind {
            proto::client_frame::Kind::Subscribe(s) => ClientFrame::Subscribe(s.into()),
            proto::client_frame::Kind::Unsubscribe(u) => ClientFrame::Unsubscribe(u.into()),
            proto::client_frame::Kind::Publish(p) => ClientFrame::Publish(p.into()),
            proto::client_frame::Kind::Connect(c) => ClientFrame::Connect(c.into()),
        })
    }
}

impl TryFrom<proto::ServerFrame> for ServerFrame {
    type Error = DecodeError;

    fn try_from(value: proto::ServerFrame) -> Result<Self, DecodeError> {
        let kind = value.kind.ok_or(DecodeError::MissingServerKind)?;
        Ok(match kind {
            proto::server_frame::Kind::SubscribeOk(s) => ServerFrame::SubscribeOk(s.into()),
            proto::server_frame::Kind::UnsubscribeOk(u) => ServerFrame::UnsubscribeOk(u.into()),
            proto::server_frame::Kind::Publication(p) => ServerFrame::Publication(p.into()),
            proto::server_frame::Kind::Error(e) => ServerFrame::Error(e.into()),
            proto::server_frame::Kind::ConnectOk(c) => ServerFrame::ConnectOk(c.into()),
        })
    }
}

impl From<proto::Connect> for Connect {
    fn from(value: proto::Connect) -> Self {
        Self {
            request_id: value.request_id,
            token: value.token,
        }
    }
}

impl From<proto::ConnectOk> for ConnectOk {
    fn from(value: proto::ConnectOk) -> Self {
        Self {
            request_id: value.request_id,
            subject: value.subject,
            expires_at: value.expires_at,
        }
    }
}

impl From<proto::Subscribe> for Subscribe {
    fn from(value: proto::Subscribe) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
            since_offset: value.since_offset,
        }
    }
}

impl From<proto::Unsubscribe> for Unsubscribe {
    fn from(value: proto::Unsubscribe) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
        }
    }
}

impl From<proto::Publish> for Publish {
    fn from(value: proto::Publish) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
            data: Bytes::from(value.data),
        }
    }
}

impl From<proto::SubscribeOk> for SubscribeOk {
    fn from(value: proto::SubscribeOk) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
            epoch: value.epoch,
            offset: value.offset,
        }
    }
}

impl From<proto::UnsubscribeOk> for UnsubscribeOk {
    fn from(value: proto::UnsubscribeOk) -> Self {
        Self {
            request_id: value.request_id,
            channel: value.channel,
        }
    }
}

impl From<proto::Publication> for Publication {
    fn from(value: proto::Publication) -> Self {
        Self {
            channel: value.channel,
            offset: value.offset,
            data: Bytes::from(value.data),
        }
    }
}

impl From<proto::Error> for ProtocolError {
    fn from(value: proto::Error) -> Self {
        Self {
            request_id: value.request_id,
            code: value.code,
            message: value.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn arb_bytes() -> impl Strategy<Value = Bytes> {
        proptest::collection::vec(any::<u8>(), 0..64).prop_map(Bytes::from)
    }

    fn arb_channel() -> impl Strategy<Value = String> {
        // Keep names sane and printable to avoid quadratic shrink cost.
        "[a-z0-9._:-]{1,32}".prop_map(String::from)
    }

    fn arb_subscribe() -> impl Strategy<Value = Subscribe> {
        (any::<u64>(), arb_channel(), any::<u64>()).prop_map(
            |(request_id, channel, since_offset)| Subscribe {
                request_id,
                channel,
                since_offset,
            },
        )
    }

    fn arb_unsubscribe() -> impl Strategy<Value = Unsubscribe> {
        (any::<u64>(), arb_channel()).prop_map(|(request_id, channel)| Unsubscribe {
            request_id,
            channel,
        })
    }

    fn arb_publish() -> impl Strategy<Value = Publish> {
        (any::<u64>(), arb_channel(), arb_bytes()).prop_map(|(request_id, channel, data)| Publish {
            request_id,
            channel,
            data,
        })
    }

    fn arb_connect() -> impl Strategy<Value = Connect> {
        (any::<u64>(), "[a-zA-Z0-9._-]{0,64}").prop_map(|(request_id, token)| Connect {
            request_id,
            token: token.to_owned(),
        })
    }

    fn arb_client_frame() -> impl Strategy<Value = ClientFrame> {
        prop_oneof![
            arb_subscribe().prop_map(ClientFrame::Subscribe),
            arb_unsubscribe().prop_map(ClientFrame::Unsubscribe),
            arb_publish().prop_map(ClientFrame::Publish),
            arb_connect().prop_map(ClientFrame::Connect),
        ]
    }

    fn arb_server_frame() -> impl Strategy<Value = ServerFrame> {
        prop_oneof![
            (any::<u64>(), arb_channel(), any::<u64>(), any::<u64>()).prop_map(
                |(request_id, channel, epoch, offset)| ServerFrame::SubscribeOk(SubscribeOk {
                    request_id,
                    channel,
                    epoch,
                    offset,
                })
            ),
            (any::<u64>(), arb_channel()).prop_map(|(request_id, channel)| {
                ServerFrame::UnsubscribeOk(UnsubscribeOk {
                    request_id,
                    channel,
                })
            }),
            (arb_channel(), any::<u64>(), arb_bytes()).prop_map(|(channel, offset, data)| {
                ServerFrame::Publication(Publication {
                    channel,
                    offset,
                    data,
                })
            }),
            (any::<u64>(), any::<u32>(), "[a-zA-Z0-9 ._:-]{0,64}").prop_map(
                |(request_id, code, message)| {
                    ServerFrame::Error(ProtocolError {
                        request_id,
                        code,
                        message: message.to_owned(),
                    })
                }
            ),
            (any::<u64>(), "[a-zA-Z0-9._:-]{0,32}", any::<u64>()).prop_map(
                |(request_id, subject, expires_at)| {
                    ServerFrame::ConnectOk(ConnectOk {
                        request_id,
                        subject: subject.to_owned(),
                        expires_at,
                    })
                }
            ),
        ]
    }

    proptest! {
        #[test]
        fn client_frame_round_trip(frame in arb_client_frame()) {
            let bytes = encode_client(&frame);
            let decoded = decode_client(&bytes).expect("encode produces decodable bytes");
            prop_assert_eq!(decoded, frame);
        }

        #[test]
        fn server_frame_round_trip(frame in arb_server_frame()) {
            let bytes = encode_server(&frame);
            let decoded = decode_server(&bytes).expect("encode produces decodable bytes");
            prop_assert_eq!(decoded, frame);
        }
    }

    #[test]
    fn decode_rejects_empty_client_frame() {
        let err = decode_client(&[]).expect_err("empty buffer must not decode");
        assert!(matches!(err, DecodeError::MissingClientKind));
    }

    #[test]
    fn decode_rejects_empty_server_frame() {
        let err = decode_server(&[]).expect_err("empty buffer must not decode");
        assert!(matches!(err, DecodeError::MissingServerKind));
    }

    #[test]
    fn decode_rejects_garbage() {
        // 0xff bytes do not parse as a valid prost message.
        let err = decode_client(&[0xff, 0xff, 0xff]).expect_err("garbage must not decode");
        assert!(matches!(err, DecodeError::Prost(_)));
    }

    #[test]
    fn snapshot_subscribe() {
        let bytes = encode_client(&ClientFrame::Subscribe(Subscribe {
            request_id: 7,
            channel: "rooms.lobby".to_owned(),
            since_offset: 0,
        }));
        insta::assert_debug_snapshot!("subscribe", bytes.as_ref());
    }

    #[test]
    fn snapshot_publish() {
        let bytes = encode_client(&ClientFrame::Publish(Publish {
            request_id: 11,
            channel: "rooms.lobby".to_owned(),
            data: Bytes::from_static(b"hello"),
        }));
        insta::assert_debug_snapshot!("publish", bytes.as_ref());
    }

    #[test]
    fn snapshot_publication() {
        let bytes = encode_server(&ServerFrame::Publication(Publication {
            channel: "rooms.lobby".to_owned(),
            offset: 42,
            data: Bytes::from_static(b"hi"),
        }));
        insta::assert_debug_snapshot!("publication", bytes.as_ref());
    }

    #[test]
    fn snapshot_connect() {
        let bytes = encode_client(&ClientFrame::Connect(Connect {
            request_id: 13,
            token: "tok-abc".to_owned(),
        }));
        insta::assert_debug_snapshot!("connect", bytes.as_ref());
    }

    #[test]
    fn snapshot_connect_ok() {
        let bytes = encode_server(&ServerFrame::ConnectOk(ConnectOk {
            request_id: 13,
            subject: "alice".to_owned(),
            expires_at: 1_700_000_000,
        }));
        insta::assert_debug_snapshot!("connect_ok", bytes.as_ref());
    }

    #[test]
    fn snapshot_error() {
        let bytes = encode_server(&ServerFrame::Error(ProtocolError {
            request_id: 9,
            code: ErrorCode::InvalidChannel.into(),
            message: "channel name must not be empty".to_owned(),
        }));
        insta::assert_debug_snapshot!("error", bytes.as_ref());
    }
}
