# Echo demo

The minimum lovable Cherenkov demo: two browser iframes chat in a single
`rooms.lobby` room over WebSocket.

## Running

In one terminal, start the server:

```sh
cargo run -p cherenkov-server -- --config examples/echo/config.yaml
```

The server listens on `127.0.0.1:7000` and exposes the WebSocket transport
at `/connect/v1`.

In a second terminal, serve the static demo files. Any HTTP server works;
the simplest option is Python's built-in:

```sh
cd examples/echo && python3 -m http.server 8080
```

Then open <http://127.0.0.1:8080/index.html> in a browser. The two iframes
will connect, subscribe to `rooms.lobby`, and round-trip messages through
the hub. Type into either pane and press Enter; the other pane sees the
publication arrive.

## What's happening

* `index.html` embeds two `<iframe>`s, each loading `client.html` with a
  different `nick` query parameter.
* `client.html` opens an independent WebSocket, sends a `Subscribe` frame,
  and from then on relays user input as `Publish` frames and renders
  incoming `Publication` frames.
* The wire format is the v1 Protobuf schema in
  [`crates/cherenkov-protocol/proto/v1.proto`](../../crates/cherenkov-protocol/proto/v1.proto).
  This demo hand-rolls a minimal Protobuf encoder/decoder so it has no
  bundler or SDK dependency. The TypeScript SDK lands in M2.

## Verifying

The same flow is exercised hermetically in
[`crates/cherenkov-server/tests/echo.rs`](../../crates/cherenkov-server/tests/echo.rs).
Run it with:

```sh
cargo test -p cherenkov-server --test echo
```
