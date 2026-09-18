# Shar — Benchmark log

Socket: `node bench/trace-runner.js <trace.json.gz>` — real socket.io client against the release binary. Internal: `./target/release/shario --bench-internal <trace.json>` — same trace replayed directly against `SharQueue`, no socket.

## 2026-09-18

Fixed: `IdeAdd` parent resolution (identity/tag-based instead of position-based), a `tree.rs` panic on a stale remove hint, and `main.rs` silently dropping `ide-add-confirmed` under `InternalChannelFull` (now retries instead of dropping). An earlier automerge-paper socket run showed multi-second latencies/160MB RSS -- traced to the dev machine's own memory pressure (swap in use), not shario: gone after a restart, and the internal (no-socket) benchmark never reproduced it even during the bad run.

### Socket

| trace | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | peak RSS |
|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 468.9 | 55,610 | 1.5 | 6.8 | 35.8 | 53.6 | 11.1 MB |
| automerge-paper | 259,778 | PASS | 3,486.0 | 74,519 | 2.2 | 6.0 | 39.3 | 59.9 | 27.7 MB |

### Internal

| trace | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | peak RSS |
|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 57.2 | 455,686 | 0.00065 | 0.0012 | 0.0035 | 1.2 | 12.6 MB |
| automerge-paper | 259,778 | PASS | 736.2 | 352,850 | 0.00065 | 0.0015 | 0.0048 | 8.3 | 87.9 MB |
