# Live-cursors demo

Multi-user "ghost cursors" over WebSocket. Each browser tab gets a
randomly coloured pointer with an animal nickname, publishes its mouse
position to `cursors.lobby` at ~30 Hz, and renders every other tab's
cursor in real time.

This demo exercises the **pub/sub fan-out** path under steady write
pressure. The `cursors.*` namespace is intentionally **schema-free**:
the payload is a tiny JSON object and the bottleneck is fan-out latency,
not validation.

## Running

In one terminal:

```sh
cargo run -p cherenkov-server -- --config examples/live-cursors/config.yaml
```

In another:

```sh
cd examples/live-cursors && python3 -m http.server 8080
```

Open <http://127.0.0.1:8080/index.html> in two or more browser tabs (or
on two devices on the same LAN). Move the mouse in any tab; the other
tabs see your cursor glide across the page.

Add `?room=<name>` to the URL to join a different room (channel
`cursors.<name>`). Tabs that share a room see each other; tabs in
different rooms do not.

## What's interesting

* Every tab is **both publisher and subscriber** on the same WebSocket
  session — the hub fans the publication out to every other subscriber
  on the channel.
* Mouse-move events are throttled client-side to ~30 Hz so the demo
  works on a laptop trackpad without flooding the broker.
* Stale cursors (no update in 5 s) disappear, so closing a tab quietly
  cleans up the peer view without a goodbye message.
* The wire format is the v1 Protobuf schema in
  [`crates/cherenkov-protocol/proto/v1.proto`](../../crates/cherenkov-protocol/proto/v1.proto);
  the demo hand-encodes frames so there is no bundler step.
