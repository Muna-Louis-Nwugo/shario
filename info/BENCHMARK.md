# Shar — Benchmark log

Socket: `node bench/trace-runner.js <trace.json.gz>` — real socket.io client against the release binary. Internal: `./target/release/shario --bench-internal <trace.json>` — same trace replayed directly against `SharQueue`, no socket. "RSS growth" is peak minus idle RSS for that run (isolates what the run itself added, not what was already resident).

## 2026-09-18

Fixed: `IdeAdd` parent resolution (identity/tag-based instead of position-based), a `tree.rs` panic on a stale remove hint, and `main.rs` silently dropping `ide-add-confirmed` under `InternalChannelFull` (now retries instead of dropping).

Socket-mode automerge-paper runs intermittently showed multi-second latencies. Confirmed cause: `find_crdt`'s ring-search cost scales linearly with hint distance, and socketioxide processes each socket event as an independent tokio task with no ordering guarantee -- under the wrong processing order, hints land far from the real position (measured mean ring-search distance ~281 and climbing vs. ~1.4 on a clean run). See [`TODO.md`](TODO.md)/[`KNOWN_ISSUES.md`](KNOWN_ISSUES.md).

### Socket

| trace | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | RSS growth |
|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 403.7 | 64,602 | 2.2 | 7.3 | 37.5 | 52.3 | 4.6 MB |
| automerge-paper | 259,778 | PASS | 3,075.0 | 84,482 | 2.2 | 5.0 | 29.1 | 42.2 | 16.8 MB |

### Internal

| trace | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | RSS growth |
|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 26,078 | PASS | 69.6 | 374,801 | 0.00064 | 0.0015 | 0.0029 | 1.0 | 2.0 MB |
| automerge-paper | 259,778 | PASS | 569.7 | 456,011 | 0.00055 | 0.0012 | 0.0043 | 20.2 | 13.9 MB |
