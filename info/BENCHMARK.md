# Shar — Benchmark log

Socket: `node bench/trace-runner.js <trace.json.gz>` — real socket.io client against the release binary. Internal: `./target/release/shario --bench-internal <trace.json>` — same trace replayed directly against `SharQueue`, no socket.

## 2026-09-18

Fixed: `IdeAdd` parent resolution (identity/tag-based instead of position-based), a `tree.rs` panic on a stale remove hint, and `main.rs` silently dropping `ide-add-confirmed` under `InternalChannelFull` (now retries instead of dropping).

### Socket

| trace | ops | result | ops/sec | p50 ms | p99 ms |
|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 57,201 | 5.9 | 23.7 |
| automerge-paper | 259,778 | PASS | 66,076 | 6.2 | 18.7 |

### Internal

| trace | ops | result | ops/sec | p50 ms | p99 ms |
|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 422,458 | 0.0013 | 0.0041 |
| automerge-paper | 259,778 | PASS | 363,238 | 0.0015 | 0.0042 |
