# Price-feed demo

A single-page Cherenkov demo that exercises **schema-validated publish/
subscribe**: a synthetic publisher emits BTC/ETH ticks every 250 ms into
the `prices.*` namespace, and a live table renders the latest tick per
symbol with a 32-point sparkline.

The `prices.*` namespace is gated by a JSON Schema declared in
[`config.yaml`](./config.yaml); the "publish invalid" button sends a
tick missing the `price` field so you can see the server reject it with
an `Error` frame carrying `code = 5 (ValidationFailed)`.

## Running

In one terminal:

```sh
cargo run -p cherenkov-server -- --config examples/price-feed/config.yaml
```

In another:

```sh
cd examples/price-feed && python3 -m http.server 8080
```

Open <http://127.0.0.1:8080/index.html> in a browser. Click **start
ticking**; the live table fills in. Click **publish invalid** to watch
the server reject the malformed tick.

Open the page in a second tab to see the same publications fan out
through the hub to multiple subscribers.

## What's interesting

* The publisher and subscriber are the same WebSocket session — the hub
  routes the publication back to the publishing session because it is
  also subscribed.
* Schema validation runs **inside the hub**, before the channel kind or
  broker see the bytes (see
  [`docs/adr/0003-schema-as-contract.md`](../../docs/adr/0003-schema-as-contract.md)).
  An invalid tick produces an `Error` frame and never reaches any other
  subscriber.
* The wire format is the v1 Protobuf schema in
  [`crates/cherenkov-protocol/proto/v1.proto`](../../crates/cherenkov-protocol/proto/v1.proto);
  the demo hand-encodes frames so it has no bundler or SDK dependency.
