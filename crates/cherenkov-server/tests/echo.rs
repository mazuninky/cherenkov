//! End-to-end fan-out test for the echo demo.
//!
//! Spins up `cherenkov-server` on an ephemeral port via
//! [`cherenkov_server::run_with_listener`], opens two WebSocket clients,
//! has both subscribe to a channel, has one publish a payload, and
//! verifies the other receives the publication.

use std::time::Duration;

use bytes::Bytes;
use cherenkov_protocol::{
    ClientFrame, Publication, Publish, ServerFrame, Subscribe, decode_server, encode_client,
};
use cherenkov_server::{ServerConfig, run_with_listener};
use futures::{SinkExt as _, StreamExt as _};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn expect_subscribe_ok(client: &mut WsClient, channel: &str) {
    let msg = timeout(Duration::from_secs(5), client.next())
        .await
        .expect("subscribe-ok arrives within timeout")
        .expect("stream not exhausted")
        .expect("read ok");
    let bytes = match msg {
        Message::Binary(b) => b,
        other => panic!("expected binary, got {other:?}"),
    };
    let frame = decode_server(&bytes).expect("decode server frame");
    match frame {
        ServerFrame::SubscribeOk(ok) => assert_eq!(ok.channel, channel),
        other => panic!("expected SubscribeOk for {channel}, got {other:?}"),
    }
}

async fn expect_publication(client: &mut WsClient) -> Publication {
    let msg = timeout(Duration::from_secs(5), client.next())
        .await
        .expect("publication arrives within timeout")
        .expect("stream not exhausted")
        .expect("read ok");
    let bytes = match msg {
        Message::Binary(b) => b,
        other => panic!("expected binary, got {other:?}"),
    };
    match decode_server(&bytes).expect("decode server frame") {
        ServerFrame::Publication(p) => p,
        other => panic!("expected Publication, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_clients_observe_fan_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let config = ServerConfig::default();
    let handle = run_with_listener(config, listener)
        .await
        .expect("server starts");
    let url = format!("ws://{}/connect/v1", handle.local_addr);

    let (mut alice, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("alice connects");
    let (mut bob, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("bob connects");

    let sub_alice = encode_client(&ClientFrame::Subscribe(Subscribe {
        request_id: 1,
        channel: "rooms.lobby".to_owned(),
        since_offset: 0,
    }));
    let sub_bob = encode_client(&ClientFrame::Subscribe(Subscribe {
        request_id: 1,
        channel: "rooms.lobby".to_owned(),
        since_offset: 0,
    }));
    alice
        .send(Message::Binary(sub_alice))
        .await
        .expect("alice subscribe");
    bob.send(Message::Binary(sub_bob))
        .await
        .expect("bob subscribe");

    expect_subscribe_ok(&mut alice, "rooms.lobby").await;
    expect_subscribe_ok(&mut bob, "rooms.lobby").await;

    let publish = encode_client(&ClientFrame::Publish(Publish {
        request_id: 2,
        channel: "rooms.lobby".to_owned(),
        data: Bytes::from_static(b"hello"),
    }));
    alice
        .send(Message::Binary(publish))
        .await
        .expect("alice publishes");

    let bob_pub = expect_publication(&mut bob).await;
    assert_eq!(bob_pub.channel, "rooms.lobby");
    assert_eq!(bob_pub.data, Bytes::from_static(b"hello"));

    let alice_pub = expect_publication(&mut alice).await;
    assert_eq!(alice_pub.data, Bytes::from_static(b"hello"));

    let _ = alice.close(None).await;
    let _ = bob.close(None).await;
    handle.shutdown();
}
