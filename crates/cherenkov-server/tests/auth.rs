//! End-to-end auth + ACL test for the WebSocket transport.
//!
//! Boots `cherenkov-server` with a JWT authenticator (HS256 + audience
//! `cherenkov`) and an ACL that allows `rooms.*` and denies `admin.*`,
//! then exercises four scenarios against the same socket pool:
//!
//! * subscribe before connect → `Error{code=NotConnected (8)}`
//! * connect with bad token  → `Error{code=InvalidToken (6)}`
//! * publish to denied chan  → `Error{code=AclDenied (7)}`
//! * publish to allowed chan → round-trips as `Publication`

use std::time::Duration;

use bytes::Bytes;
use cherenkov_protocol::{
    decode_server, encode_client, ClientFrame, Connect, ErrorCode, ProtocolError, Publish,
    ServerFrame, Subscribe,
};
use cherenkov_server::{
    run_with_listener, AclConfig, AclEffectConfig, AclRuleConfig, AuthConfig, ServerConfig,
};
use futures::{SinkExt as _, StreamExt as _};
use jsonwebtoken::{encode as jwt_encode, Algorithm, EncodingKey, Header};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

const SECRET: &[u8] = b"integration-test-secret";

fn signed_token(audience: &str) -> String {
    jwt_encode(
        &Header::new(Algorithm::HS256),
        &json!({
            "sub": "alice",
            "aud": audience,
            "exp": 9_999_999_999u64,
            "permissions": ["publish", "subscribe"],
        }),
        &EncodingKey::from_secret(SECRET),
    )
    .expect("sign")
}

async fn next_server_frame(c: &mut WsClient) -> ServerFrame {
    let msg = timeout(Duration::from_secs(5), c.next())
        .await
        .expect("timeout")
        .expect("not exhausted")
        .expect("read ok");
    let Message::Binary(b) = msg else {
        panic!("expected binary, got {msg:?}");
    };
    decode_server(&b).expect("decode")
}

async fn expect_error(c: &mut WsClient, code: ErrorCode) -> ProtocolError {
    match next_server_frame(c).await {
        ServerFrame::Error(e) => {
            assert_eq!(e.code, u32::from(code), "wrong error code");
            e
        }
        other => panic!("expected Error{{code={}}}, got {other:?}", code as u32),
    }
}

async fn send(c: &mut WsClient, frame: ClientFrame) {
    let bytes = encode_client(&frame);
    c.send(Message::Binary(bytes.to_vec())).await.expect("send");
}

fn build_config() -> ServerConfig {
    let mut config = ServerConfig {
        auth: Some(AuthConfig {
            hmac_secret: String::from_utf8(SECRET.to_vec()).unwrap(),
            audiences: vec!["cherenkov".to_owned()],
            issuer: None,
        }),
        ..ServerConfig::default()
    };
    config.acl = Some(AclConfig {
        rules: vec![
            AclRuleConfig {
                effect: AclEffectConfig::Allow,
                channel: "rooms.*".to_owned(),
                subject: None,
                action: cherenkov_server::AclActionConfig::Any,
            },
            AclRuleConfig {
                effect: AclEffectConfig::Deny,
                channel: "admin.*".to_owned(),
                subject: None,
                action: cherenkov_server::AclActionConfig::Any,
            },
        ],
        default_allow: false,
    });
    config
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auth_and_acl_end_to_end() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let handle = run_with_listener(build_config(), listener)
        .await
        .expect("server starts");
    let url = format!("ws://{}/connect/v1", handle.local_addr);

    // Subscribe before connect → NotConnected.
    let (mut precon, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client connects");
    send(
        &mut precon,
        ClientFrame::Subscribe(Subscribe {
            request_id: 1,
            channel: "rooms.lobby".to_owned(),
            since_offset: 0,
        }),
    )
    .await;
    expect_error(&mut precon, ErrorCode::NotConnected).await;
    let _ = precon.close(None).await;

    // Bad token → InvalidToken.
    let (mut bad, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client connects");
    send(
        &mut bad,
        ClientFrame::Connect(Connect {
            request_id: 7,
            token: "not.a.real.jwt".to_owned(),
        }),
    )
    .await;
    let err = expect_error(&mut bad, ErrorCode::InvalidToken).await;
    assert_eq!(err.request_id, 7);
    let _ = bad.close(None).await;

    // Good token, publish to allowed and denied channels.
    let (mut alice, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client connects");
    send(
        &mut alice,
        ClientFrame::Connect(Connect {
            request_id: 1,
            token: signed_token("cherenkov"),
        }),
    )
    .await;
    match next_server_frame(&mut alice).await {
        ServerFrame::ConnectOk(ok) => assert_eq!(ok.subject, "alice"),
        other => panic!("expected ConnectOk, got {other:?}"),
    }

    // Subscribe to allowed channel.
    send(
        &mut alice,
        ClientFrame::Subscribe(Subscribe {
            request_id: 2,
            channel: "rooms.lobby".to_owned(),
            since_offset: 0,
        }),
    )
    .await;
    match next_server_frame(&mut alice).await {
        ServerFrame::SubscribeOk(ok) => assert_eq!(ok.channel, "rooms.lobby"),
        other => panic!("expected SubscribeOk, got {other:?}"),
    }

    // Publish to denied channel → AclDenied.
    send(
        &mut alice,
        ClientFrame::Publish(Publish {
            request_id: 3,
            channel: "admin.users".to_owned(),
            data: Bytes::from_static(b"x"),
        }),
    )
    .await;
    let err = expect_error(&mut alice, ErrorCode::AclDenied).await;
    assert_eq!(err.request_id, 3);

    // Publish to allowed channel → fan-out as Publication.
    send(
        &mut alice,
        ClientFrame::Publish(Publish {
            request_id: 4,
            channel: "rooms.lobby".to_owned(),
            data: Bytes::from_static(b"hello"),
        }),
    )
    .await;
    match next_server_frame(&mut alice).await {
        ServerFrame::Publication(p) => {
            assert_eq!(p.channel, "rooms.lobby");
            assert_eq!(p.data, Bytes::from_static(b"hello"));
        }
        other => panic!("expected Publication, got {other:?}"),
    }

    let _ = alice.close(None).await;
    handle.shutdown();
}
