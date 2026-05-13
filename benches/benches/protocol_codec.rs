//! Wire-protocol encode / decode microbenchmarks.
//!
//! Measures the round-trip cost of the v1 framing for the three frames that
//! dominate steady-state traffic: `Subscribe`, `Publish`, and `Publication`.
//! Payload sizes sweep through the bands we expect in production
//! (chat-message-size, JSON-document-size, large-binary).

use bytes::Bytes;
use cherenkov_protocol::{
    ClientFrame, Publication, Publish, ServerFrame, Subscribe, decode_client, decode_server,
    encode_client, encode_server,
};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

const PAYLOAD_SIZES: &[usize] = &[64, 1_024, 16_384];

fn bench_encode_subscribe(c: &mut Criterion) {
    let frame = ClientFrame::Subscribe(Subscribe {
        request_id: 42,
        channel: "rooms.lobby".to_owned(),
        since_offset: 0,
    });
    c.bench_function("encode_client/subscribe", |b| {
        b.iter(|| {
            let bytes = encode_client(black_box(&frame));
            black_box(bytes);
        });
    });
}

fn bench_publish_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_decode_client/publish");
    for &size in PAYLOAD_SIZES {
        let payload = Bytes::from(vec![0xA5_u8; size]);
        let frame = ClientFrame::Publish(Publish {
            request_id: 7,
            channel: "rooms.lobby".to_owned(),
            data: payload,
        });
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &frame, |b, frame| {
            b.iter(|| {
                let bytes = encode_client(black_box(frame));
                let decoded = decode_client(&bytes).expect("round-trip");
                black_box(decoded);
            });
        });
    }
    group.finish();
}

fn bench_publication_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_decode_server/publication");
    for &size in PAYLOAD_SIZES {
        let payload = Bytes::from(vec![0xC3_u8; size]);
        let frame = ServerFrame::Publication(Publication {
            channel: "rooms.lobby".to_owned(),
            offset: 1_000,
            data: payload,
        });
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &frame, |b, frame| {
            b.iter(|| {
                let bytes = encode_server(black_box(frame));
                let decoded = decode_server(&bytes).expect("round-trip");
                black_box(decoded);
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_encode_subscribe,
    bench_publish_round_trip,
    bench_publication_round_trip
);
criterion_main!(benches);
