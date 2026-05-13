//! End-to-end schema validation test.
//!
//! Spins up `cherenkov-server` with one declared namespace (`orders`),
//! connects a WebSocket client, and verifies:
//!
//! 1. A valid payload is broadcast back as a `Publication`.
//! 2. A malformed payload is rejected with an `Error` frame whose code is
//!    [`ErrorCode::ValidationFailed`].
//! 3. A channel outside the declared namespace is opaque pass-through.

use std::collections::BTreeMap;
use std::time::Duration;

use bytes::Bytes;
use cherenkov_protocol::{
    ClientFrame, ErrorCode, Publish, ServerFrame, Subscribe, decode_server, encode_client,
};
use cherenkov_server::{
    NamespaceConfig, NamespacesConfig, SchemaKind, ServerConfig, run_with_listener,
};
use futures::{SinkExt as _, StreamExt as _};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn next_frame(client: &mut WsClient) -> ServerFrame {
    let msg = timeout(Duration::from_secs(5), client.next())
        .await
        .expect("frame arrives within timeout")
        .expect("stream not exhausted")
        .expect("read ok");
    let bytes = match msg {
        Message::Binary(b) => b,
        other => panic!("expected binary, got {other:?}"),
    };
    decode_server(&bytes).expect("decode server frame")
}

fn config_with_orders_schema() -> ServerConfig {
    let mut namespaces = BTreeMap::new();
    namespaces.insert(
        "orders".to_owned(),
        NamespaceConfig {
            kind: SchemaKind::JsonSchema,
            schema: Some(json!({
                "type": "object",
                "required": ["sku", "qty"],
                "properties": {
                    "sku": { "type": "string", "minLength": 1 },
                    "qty": { "type": "integer", "minimum": 1 }
                }
            })),
            schema_path: None,
        },
    );
    ServerConfig {
        namespaces: NamespacesConfig(namespaces),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn schema_validation_end_to_end() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let handle = run_with_listener(config_with_orders_schema(), listener)
        .await
        .expect("server starts");
    let url = format!("ws://{}/connect/v1", handle.local_addr);

    let (mut client, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("client connects");

    // Subscribe to the validated namespace.
    client
        .send(Message::Binary(encode_client(&ClientFrame::Subscribe(
            Subscribe {
                request_id: 1,
                channel: "orders.created".to_owned(),
                since_offset: 0,
            },
        ))))
        .await
        .expect("subscribe send");
    match next_frame(&mut client).await {
        ServerFrame::SubscribeOk(ok) => assert_eq!(ok.channel, "orders.created"),
        other => panic!("expected SubscribeOk, got {other:?}"),
    }

    // 1. Valid payload — broadcast back to the same client (it is a
    // subscriber, so the publication should round-trip).
    client
        .send(Message::Binary(encode_client(&ClientFrame::Publish(
            Publish {
                request_id: 2,
                channel: "orders.created".to_owned(),
                data: Bytes::from_static(br#"{"sku":"abc","qty":3}"#),
            },
        ))))
        .await
        .expect("publish send (valid)");
    match next_frame(&mut client).await {
        ServerFrame::Publication(p) => {
            assert_eq!(p.channel, "orders.created");
            assert_eq!(p.data, Bytes::from_static(br#"{"sku":"abc","qty":3}"#));
        }
        other => panic!("expected Publication, got {other:?}"),
    }

    // 2. Malformed payload — Error frame with ValidationFailed code,
    // request_id echoed.
    client
        .send(Message::Binary(encode_client(&ClientFrame::Publish(
            Publish {
                request_id: 7,
                channel: "orders.created".to_owned(),
                data: Bytes::from_static(br#"{"sku":""}"#),
            },
        ))))
        .await
        .expect("publish send (invalid)");
    match next_frame(&mut client).await {
        ServerFrame::Error(err) => {
            assert_eq!(err.request_id, 7);
            assert_eq!(err.code, u32::from(ErrorCode::ValidationFailed));
            assert!(!err.message.is_empty());
        }
        other => panic!("expected Error, got {other:?}"),
    }

    // 3. Namespace without a declared schema is opaque pass-through.
    client
        .send(Message::Binary(encode_client(&ClientFrame::Subscribe(
            Subscribe {
                request_id: 11,
                channel: "rooms.lobby".to_owned(),
                since_offset: 0,
            },
        ))))
        .await
        .expect("subscribe send (opaque)");
    match next_frame(&mut client).await {
        ServerFrame::SubscribeOk(ok) => assert_eq!(ok.channel, "rooms.lobby"),
        other => panic!("expected SubscribeOk for rooms.lobby, got {other:?}"),
    }
    client
        .send(Message::Binary(encode_client(&ClientFrame::Publish(
            Publish {
                request_id: 12,
                channel: "rooms.lobby".to_owned(),
                // Deliberately not JSON: the registry must pass it through
                // because rooms has no schema.
                data: Bytes::from_static(b"\x00\x01\x02"),
            },
        ))))
        .await
        .expect("publish send (opaque)");
    match next_frame(&mut client).await {
        ServerFrame::Publication(p) => {
            assert_eq!(p.channel, "rooms.lobby");
            assert_eq!(p.data, Bytes::from_static(b"\x00\x01\x02"));
        }
        other => panic!("expected Publication on opaque namespace, got {other:?}"),
    }

    let _ = client.close(None).await;
    handle.shutdown();
}
