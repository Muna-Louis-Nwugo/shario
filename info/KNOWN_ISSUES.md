# Known Issues

Bugs that are known/observed but not yet root-caused or fixed. Unlike
[`TODO.md`](TODO.md), these don't have a confirmed mechanism or a planned fix yet --
just a symptom.

The large-burst crash (client-side position tracking diverging under
load) is fixed -- see [`TODO.md`](TODO.md)'s "1.1" entry and [`BENCHMARK.md`](BENCHMARK.md) for
full-trace verification. `shario-vscode`'s `maybeRestartServer` watchdog
(see `extension.js`) still auto-restarts the server if it ever dies for
an unrelated reason.

## Socket-mode benchmark runs intermittently take multiple seconds instead of milliseconds

**Symptom:** replaying the automerge-paper trace through `bench/trace-runner.js`
occasionally takes 8-10s instead of the usual ~3s, with `p50` latency in
the seconds and RSS growth well above normal. Not every run -- roughly
half, no obvious trigger from the outside.

**Mechanism: confirmed**, not just observed -- see [`TODO.md`](TODO.md)'s matching
entry for the full explanation and fix direction. Not a correctness bug
(every run still converges to the right document); purely a latency/perf
issue under a synthetic full-speed burst. Real typing is paced far below
this, giving the server time to keep processing order close to send
order, so this isn't expected to matter in actual use -- documented here
for the record, not urgent.
