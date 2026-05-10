//! End-to-end hub microbenchmarks.
//!
//! Constructs a `Hub` with `MemoryBroker` + `PubSubChannel`, opens N
//! subscribers on a single channel, and measures the latency of a single
//! `handle_publish` call. The subscribers drain their outboxes on a
//! background task so back-pressure does not skew the measurement.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use cherenkov_broker::MemoryBroker;
use cherenkov_channel_pubsub::PubSubChannel;
use cherenkov_core::{Hub, HubBuilder};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

const SUBSCRIBER_COUNTS: &[usize] = &[1, 10, 100];

fn build_hub() -> Hub {
    let kind = Arc::new(PubSubChannel::with_bounds(64, Duration::from_secs(60)));
    let broker = Arc::new(MemoryBroker::with_capacity(1024));
    let built = HubBuilder::new()
        .with_channel_kind(kind)
        .with_broker(broker)
        .build()
        .expect("hub builds");
    built.hub
}

async fn open_subscribers(hub: &Hub, channel: &str, count: usize) {
    for i in 0..count {
        let (tx, mut rx) = mpsc::channel(64);
        let session = hub.open_session(tx);
        hub.handle_subscribe(session.id(), (i + 1) as u64, channel, 0)
            .await
            .expect("subscribe");
        // Drain in the background so the outbox never back-pressures the hub.
        tokio::spawn(async move { while rx.recv().await.is_some() {} });
    }
}

fn bench_publish_fanout(c: &mut Criterion) {
    let runtime = Runtime::new().expect("tokio runtime");
    let mut group = c.benchmark_group("hub/publish_fanout");
    group.measurement_time(Duration::from_secs(6));

    for &count in SUBSCRIBER_COUNTS {
        let hub = build_hub();
        // Publisher session — also subscribes so it sees its own publications.
        let (pub_tx, mut pub_rx) = mpsc::channel(64);
        let publisher = runtime.block_on(async {
            let s = hub.open_session(pub_tx);
            hub.handle_subscribe(s.id(), 0, "rooms.lobby", 0)
                .await
                .expect("publisher subscribe");
            tokio::spawn(async move { while pub_rx.recv().await.is_some() {} });
            s
        });
        runtime.block_on(open_subscribers(&hub, "rooms.lobby", count));

        let payload = Bytes::from_static(b"hello world");
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.to_async(&runtime).iter(|| {
                let hub = hub.clone();
                let session_id = publisher.id();
                let payload = payload.clone();
                async move {
                    let pubn = hub
                        .handle_publish(session_id, black_box("rooms.lobby"), payload)
                        .await
                        .expect("publish");
                    black_box(pubn);
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_publish_fanout);
criterion_main!(benches);
