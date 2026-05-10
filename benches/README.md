# Cherenkov benchmarks

Workspace-level [criterion](https://docs.rs/criterion) benchmarks. The
crate is `publish = false` and lives in `benches/` so that
`cargo bench --workspace` picks it up but published artifacts do not.

## Running

```sh
# all targets
cargo bench -p cherenkov-benches

# one target
cargo bench -p cherenkov-benches --bench protocol_codec
```

Pass `-- --quick` for a fast smoke run, or `-- --save-baseline <name>`
to record a baseline you can later `--baseline <name>` against.

## Targets

| Bench              | Measures                                                           |
|--------------------|--------------------------------------------------------------------|
| `protocol_codec`   | v1 wire-format encode / decode for `Subscribe`, `Publish`, `Publication` across 64&nbsp;B / 1&nbsp;KB / 16&nbsp;KB payloads. |
| `hub_pubsub`       | End-to-end `Hub::handle_publish` latency with `MemoryBroker` + `PubSubChannel`, sweeping subscriber counts (1 / 10 / 100). |
| `schema_validate`  | Per-publish validation cost across `AllowAllValidator`, JSON-Schema unknown-namespace, and JSON-Schema registered. |

## Adding a benchmark

1. Add a new file under `benches/` (e.g. `benches/my_bench.rs`).
2. Add the matching `[[bench]]` block to `benches/Cargo.toml`.
3. Use the existing files as a template — `criterion_group!` /
   `criterion_main!` with `harness = false` in the manifest.

Hot loop work goes through `criterion::black_box` so the optimiser does
not constant-fold it away.
