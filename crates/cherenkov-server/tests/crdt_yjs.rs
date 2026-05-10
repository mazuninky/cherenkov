//! End-to-end test: a `crdt-yjs` namespace round-trips Y.js updates
//! through the WebSocket transport.
//!
//! Two clients subscribe to `doc.shared`; one publishes a Y.js update
//! that inserts text; the other observes the same bytes back as a
//! `Publication` and applies them to its local doc.

use std::collections::BTreeMap;
use std::time::Duration;

use bytes::Bytes;
use cherenkov_protocol::{
    decode_server, encode_client, ClientFrame, Publish, ServerFrame, Subscribe,
};
use cherenkov_server::{run_with_listener, ChannelKindName, ChannelKindsConfig, ServerConfig};
use futures::{SinkExt as _, StreamExt as _};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, Text, Transact, Update};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn yjs_namespace_round_trips_updates() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let mut config = ServerConfig::default();
    let mut kinds = BTreeMap::new();
    kinds.insert("doc".to_owned(), ChannelKindName::CrdtYjs);
    config.channel_kinds = ChannelKindsConfig(kinds);
    let handle = run_with_listener(config, listener)
        .await
        .expect("server starts");
    let url = format!("ws://{}/connect/v1", handle.local_addr);

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let sub = ClientFrame::Subscribe(Subscribe {
        request_id: 1,
        channel: "doc.shared".to_owned(),
        since_offset: 0,
    });
    a.send(Message::Binary(encode_client(&sub).to_vec()))
        .await
        .unwrap();
    b.send(Message::Binary(encode_client(&sub).to_vec()))
        .await
        .unwrap();
    assert!(matches!(
        next_server_frame(&mut a).await,
        ServerFrame::SubscribeOk(_)
    ));
    assert!(matches!(
        next_server_frame(&mut b).await,
        ServerFrame::SubscribeOk(_)
    ));

    // Build a Y.js update that inserts "hi" into a "greeting" text type.
    let publisher_doc = Doc::new();
    let text = publisher_doc.get_or_insert_text("greeting");
    let mut txn = publisher_doc.transact_mut();
    text.insert(&mut txn, 0, "hi");
    let update = txn.encode_update_v1();
    drop(txn);

    a.send(Message::Binary(
        encode_client(&ClientFrame::Publish(Publish {
            request_id: 2,
            channel: "doc.shared".to_owned(),
            data: Bytes::from(update.clone()),
        }))
        .to_vec(),
    ))
    .await
    .unwrap();

    // The publisher's own subscribe stream observes the same Publication.
    match next_server_frame(&mut a).await {
        ServerFrame::Publication(p) => assert_eq!(p.data.as_ref(), update.as_slice()),
        other => panic!("expected Publication on publisher socket, got {other:?}"),
    }
    // And so does the second subscriber.
    let observed = match next_server_frame(&mut b).await {
        ServerFrame::Publication(p) => p,
        other => panic!("expected Publication on observer socket, got {other:?}"),
    };
    assert_eq!(observed.data.as_ref(), update.as_slice());

    // Apply the observed update locally and confirm semantic equivalence.
    let local = Doc::new();
    let local_text = local.get_or_insert_text("greeting");
    let mut local_txn = local.transact_mut();
    local_txn
        .apply_update(Update::decode_v1(&observed.data).unwrap())
        .unwrap();
    drop(local_txn);
    let read_txn = local.transact();
    assert_eq!(local_text.get_string(&read_txn), "hi");

    let _ = a.close(None).await;
    let _ = b.close(None).await;
    handle.shutdown();
}
