//! End-to-end recovery test: a client publishes three frames, then a
//! second client subscribes with `since_offset = 1` and observes the
//! retained frame at offset 2 before the live stream takes over.

use std::time::Duration;

use bytes::Bytes;
use cherenkov_protocol::{
    ClientFrame, Publish, ServerFrame, Subscribe, decode_server, encode_client,
};
use cherenkov_server::{ServerConfig, run_with_listener};
use futures::{SinkExt as _, StreamExt as _};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn next_frame(c: &mut WsClient) -> ServerFrame {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recovery_replays_retained_history() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let handle = run_with_listener(ServerConfig::default(), listener)
        .await
        .expect("server starts");
    let url = format!("ws://{}/connect/v1", handle.local_addr);

    // Publisher posts three messages without subscribing — the channel
    // kind retains them in its history layer regardless of broker
    // subscribers.
    let (mut publisher, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("publisher connects");
    for body in [b"m1", b"m2", b"m3"] {
        publisher
            .send(Message::Binary(encode_client(&ClientFrame::Publish(
                Publish {
                    request_id: 9,
                    channel: "rooms.lobby".to_owned(),
                    data: Bytes::copy_from_slice(body),
                },
            ))))
            .await
            .unwrap();
    }
    // Give the hub a moment to apply each publish through the channel
    // kind before the latecomer's subscribe asks for replay.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // New subscriber asks to replay everything strictly after offset 0
    // — i.e. offsets 1 and 2.
    let (mut latecomer, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("latecomer connects");
    latecomer
        .send(Message::Binary(encode_client(&ClientFrame::Subscribe(
            Subscribe {
                request_id: 2,
                channel: "rooms.lobby".to_owned(),
                since_offset: 1,
            },
        ))))
        .await
        .unwrap();

    // Pull at most three frames; we expect the replayed Publication
    // (offset 2) plus the SubscribeOk. Their relative ordering is
    // implementation-defined: current code pushes replay first.
    let mut got_offsets = Vec::new();
    let mut got_subscribe_ok = false;
    for _ in 0..3 {
        match next_frame(&mut latecomer).await {
            ServerFrame::Publication(p) => {
                assert_eq!(p.channel, "rooms.lobby");
                got_offsets.push(p.offset);
            }
            ServerFrame::SubscribeOk(ok) => {
                assert_eq!(ok.channel, "rooms.lobby");
                got_subscribe_ok = true;
            }
            ServerFrame::Error(e) => panic!("got Error frame: code={} msg={}", e.code, e.message),
            other => panic!("unexpected frame {other:?}"),
        }
        if got_subscribe_ok && !got_offsets.is_empty() {
            break;
        }
    }
    assert_eq!(got_offsets, vec![2u64]);
    assert!(got_subscribe_ok, "no SubscribeOk delivered");

    let _ = publisher.close(None).await;
    let _ = latecomer.close(None).await;
    handle.shutdown();
}
