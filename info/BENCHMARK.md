# Shar — Benchmark log

Socket: `node bench/trace-runner.js <trace.json.gz>` — real socket.io client against the release binary. Internal: `./target/release/shario --bench-internal <trace.json>` — same trace replayed directly against `SharQueue`, no socket. "doc size" is the final document's size (`endContent`), not the trace file's size (the trace file also encodes every character that got typed *and later deleted*, so it's much larger than what's actually on screen at the end) — this makes it comparable against RSS growth as an amplification ratio. "RSS growth" is peak minus idle RSS for that run (isolates what the run itself added, not what was already resident); idle RSS is captured *after* the trace file itself is loaded and parsed, so RSS growth isn't polluted by that either.

## 2026-10-12

Re-ran after a large stretch of changes since the last entry: the whole peer-to-peer layer was rewritten from scratch onto `iroh`/`iroh-gossip` plus a Redis-backed discovery DB (none of that existed in the last entry at all), `PeerIdSize` widened from `u8` to a randomly-generated u32, and `NetworkAdd`/`Remove`'s file paths now get converted to/from relative on every single op instead of travelling as the raw absolute path.

`clap` now requires a `create`/`participate` subcommand, which broke `bench/trace-runner.js` (it spawned the server with no arguments at all) -- fixed by adding a `--socket-benchmark` flag that skips Redis/iroh entirely and just starts the IDE-facing socket.io server, since socket-mode benchmarking only ever exercised that local path anyway.

Internal-mode numbers below are from a clean run after a laptop restart -- an initial pass actually came in noticeably *slower* than the last recorded entry, and kept creeping up further on repeated back-to-back runs with no code changes in between, which pointed at system load rather than a real regression. Confirmed: after restarting, both traces came in faster than the last recorded entry, not just back to baseline.

Socket-mode was re-checked after the restart too, and it's a mixed result, not a clean "machine load explains everything":
- **friendsforever** came back in line with the original baseline on both time and RSS growth -- that part really was just machine load.
- **automerge-paper**'s total time mostly recovered (4,517.6 ms now vs. 6,882.8 ms before vs. 3,075.0 ms in the original baseline -- better, but still ~47% slower than baseline, not fully back), but its RSS growth did *not* recover: 65.3 MB now, barely different from the earlier 77.3 MB, and still roughly 4x the original baseline's 16.8 MB. Given friendsforever (much smaller, far fewer deletes) looks completely normal while automerge-paper (the largest trace, by far the most deletes) doesn't, this looks like it scales with trace size or delete volume specifically, not a flat per-run overhead. Still not root-caused -- worth profiling rather than guessing further.

### Socket

| trace | doc size | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | RSS growth |
|---|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 21,362 B | 26,078 | PASS | 441.7 | 59,044 | 1.5 | 6.5 | 16.6 | 45.5 | 4.8 MB |
| automerge-paper | 104,852 B | 259,778 | PASS | 4,517.6 | 57,503 | 2.6 | 255.6 | 1,385.7 | 1,414.0 | 65.3 MB |

### Internal

| trace | doc size | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | RSS growth |
|---|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 21,362 B | 26,078 | PASS | 49.7 | 524,943 | 0.0007 | 0.0011 | 0.0032 | 0.6 | 2.3 MB |
| automerge-paper | 104,852 B | 259,778 | PASS | 656.0 | 396,032 | 0.0007 | 0.0014 | 0.0038 | 6.4 | 15.2 MB |

## 2026-09-18

Fixed: `IdeAdd` parent resolution (identity/tag-based instead of position-based), a `tree.rs` panic on a stale remove hint, and `main.rs` silently dropping `ide-add-confirmed` under `InternalChannelFull` (now retries instead of dropping).

Socket-mode automerge-paper runs intermittently showed multi-second latencies. Confirmed cause: `find_crdt`'s ring-search cost scales linearly with hint distance, and socketioxide processes each socket event as an independent tokio task with no ordering guarantee -- under the wrong processing order, hints land far from the real position (measured mean ring-search distance ~281 and climbing vs. ~1.4 on a clean run). See [`TODO.md`](TODO.md)/[`KNOWN_ISSUES.md`](KNOWN_ISSUES.md).

### Socket

| trace | doc size | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | RSS growth |
|---|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 21,362 B | 26,078 | PASS | 403.7 | 64,602 | 2.2 | 7.3 | 37.5 | 52.3 | 4.6 MB |
| automerge-paper | 104,852 B | 259,778 | PASS | 3,075.0 | 84,482 | 2.2 | 5.0 | 29.1 | 42.2 | 16.8 MB |

### Internal

| trace | doc size | ops | result | total ms | ops/sec | min ms | p50 ms | p99 ms | max ms | RSS growth |
|---|---|---|---|---|---|---|---|---|---|---|
| friendsforever_flat | 21,362 B | 26,078 | PASS | 69.3 | 376,118 | 0.00083 | 0.0013 | 0.0029 | 0.7 | 2.0 MB |
| automerge-paper | 104,852 B | 259,778 | PASS | 748.8 | 346,934 | 0.00083 | 0.0017 | 0.0035 | 6.0 | 14.0 MB |
