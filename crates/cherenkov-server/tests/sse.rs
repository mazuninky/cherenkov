//! End-to-end smoke test for the SSE transport.
//!
//! Spawns the full server with WS + SSE listeners on ephemeral ports,
//! then opens an SSE subscriber via `reqwest`, posts a publication
//! through the SSE publish endpoint, and confirms the event arrives.

use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use cherenkov_server::{run, ServerConfig, SseConfig};
use futures::StreamExt as _;
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sse_publish_and_subscribe_round_trip() {
    let mut config = ServerConfig::default();
    config.transport.ws.listen = "127.0.0.1:0".parse().unwrap();
    config.transport.sse = Some(SseConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        path_prefix: "/sse/v1".to_owned(),
    });
    let handle = run(config).await.expect("server starts");
    let sse_addr = handle.sse_addr.expect("sse enabled");

    let subscribe_url = format!("http://{sse_addr}/sse/v1/subscribe?channel=rooms.lobby");
    let publish_url = format!("http://{sse_addr}/sse/v1/publish?channel=rooms.lobby");

    let client = reqwest::Client::new();

    // Open the subscriber stream first; otherwise our publish is delivered
    // into a topic with no in-process listener (broker is in-process here)
    // and the event would be lost.
    let resp = client
        .get(&subscribe_url)
        .send()
        .await
        .expect("subscribe response");
    assert!(
        resp.status().is_success(),
        "subscribe status: {}",
        resp.status()
    );
    let mut stream = resp.bytes_stream();

    // Give the GET handler a moment to register the subscription with the
    // hub before we POST. 50ms is generous on localhost.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let publish = client
        .post(&publish_url)
        .body(b"hello-sse".to_vec())
        .send()
        .await
        .expect("publish response");
    assert!(
        publish.status().is_success(),
        "publish status: {}",
        publish.status()
    );

    let mut buf = String::new();
    let needle_event = "event: publication";
    let needle_payload = BASE64.encode(b"hello-sse");
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => panic!("timed out waiting for SSE event; got so far: {buf}"),
            chunk = timeout(Duration::from_secs(1), stream.next()) => {
                let chunk = match chunk {
                    Ok(Some(Ok(bytes))) => bytes,
                    Ok(Some(Err(e))) => panic!("stream error: {e}"),
                    Ok(None) => panic!("stream ended without event; buffer: {buf}"),
                    Err(_) => continue,
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));
                if buf.contains(needle_event) && buf.contains(&needle_payload) {
                    break;
                }
            }
        }
    }

    handle.shutdown();
}
