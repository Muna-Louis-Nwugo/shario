# Shar — Benchmark log

Socket: `node bench/trace-runner.js <trace.json.gz>` — real socket.io client against the release binary. Internal: `./target/release/shario --bench-internal <trace.json>` — same trace replayed directly against `SharQueue`, no socket.

## 2026-09-18

Fixed: `IdeAdd` parent resolution (identity/tag-based instead of position-based), a `tree.rs` panic on a stale remove hint, and `main.rs` silently dropping `ide-add-confirmed` under `InternalChannelFull` (now retries instead of dropping).

### Socket

| trace | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms |
|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 468.9 | 55,610 | 1.5 | 6.8 | 35.8 | 53.6 |
| automerge-paper | 259,778 | PASS | 3,862.2 | 67,262 | 3.0 | 6.2 | 18.1 | 61.7 |

### Internal

| trace | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms |
|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 57.2 | 455,686 | 0.00065 | 0.0012 | 0.0035 | 1.2 |
| automerge-paper | 259,778 | PASS | 736.2 | 352,850 | 0.00065 | 0.0015 | 0.0048 | 8.3 |
