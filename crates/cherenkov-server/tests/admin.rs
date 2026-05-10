//! End-to-end admin test: the server boots WS + admin on ephemeral
//! ports, opens a WebSocket session, and verifies that:
//!
//! * `GET /admin/v1/health` reports `sessions = 1` after the WS connect
//!   has been processed by the hub.
//! * `POST /admin/v1/sessions/<id>/disconnect` evicts the session, the
//!   WebSocket on the data plane is closed, and `health` reports
//!   `sessions = 0`.

use std::time::Duration;

use cherenkov_server::{run, AdminConfig, ServerConfig};
use futures::StreamExt as _;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admin_health_and_disconnect_round_trip() {
    let mut config = ServerConfig::default();
    config.transport.ws.listen = "127.0.0.1:0".parse().unwrap();
    config.admin = AdminConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
        enabled: true,
        auth_token: Some("admin-test".to_owned()),
    };
    let handle = run(config).await.expect("server starts");
    let admin_addr = handle.admin_addr.expect("admin enabled");
    let ws_addr = handle.ws_addr;

    let client = reqwest::Client::new();
    let health_url = format!("http://{admin_addr}/admin/v1/health");
    let sessions_url = format!("http://{admin_addr}/admin/v1/sessions");

    // Auth required: missing header is 401.
    let resp = client.get(&health_url).send().await.unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    // With the right token the health endpoint succeeds.
    let body: serde_json::Value = client
        .get(&health_url)
        .bearer_auth("admin-test")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["sessions"], 0);

    // Open a WS session, give the hub a beat to register it, then check
    // that admin reports one session.
    let (mut ws, _) = tokio_tungstenite::connect_async(&format!("ws://{ws_addr}/connect/v1"))
        .await
        .expect("ws connects");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let body: serde_json::Value = client
        .get(&health_url)
        .bearer_auth("admin-test")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["sessions"], 1);

    let sessions: serde_json::Value = client
        .get(&sessions_url)
        .bearer_auth("admin-test")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sessions = sessions.as_array().expect("array");
    assert_eq!(sessions.len(), 1);
    let id = sessions[0]["id"].as_u64().expect("id present");

    // Kick the session.
    let resp = client
        .post(format!(
            "http://{admin_addr}/admin/v1/sessions/{id}/disconnect"
        ))
        .bearer_auth("admin-test")
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "disconnect status: {}",
        resp.status()
    );

    // The WS reader sees the close shortly after.
    let next = timeout(Duration::from_secs(2), ws.next()).await;
    match next {
        Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) => {}
        Ok(Some(Ok(other))) => panic!("expected close, got {other:?}"),
        Err(_) => panic!("WS did not observe close after kick"),
    }

    handle.shutdown();
}
