# Known Issues

Bugs that are known/observed but not yet root-caused or fixed. Unlike
[`TODO.md`](TODO.md), these don't have a confirmed mechanism or a planned fix yet --
just a symptom.

The large-burst crash (client-side position tracking diverging under
load) is fixed -- see [`TODO.md`](TODO.md)'s "1.1" entry and [`BENCHMARK.md`](BENCHMARK.md) for
full-trace verification. `shario-vscode`'s `maybeRestartServer` watchdog
(see `extension.js`) still auto-restarts the server if it ever dies for
an unrelated reason.

## Editing a shar's files outside the shar, between runs, can silently corrupt or lose history

**Symptom:** a peer disconnects (or the process just isn't running), someone edits the directory's files directly -- a plain text editor, a `git pull`, anything that isn't the shar itself -- and then the shar is started again on that same directory. Depending on exactly what changed, this can either silently drop a real incoming edit as a false "already applied" duplicate, or just quietly lose that replica's entire prior networked history (other peers' edits into what used to be this content, tombstones, all of it).

**Mechanism: confirmed.** There is currently no persistence of CRDT state across process restarts at all -- editing through socket.io only ever mutates the in-memory `characters`/`projection` (`src/shar/core/tree.rs`); nothing is ever flushed back to disk. Every time a `shario` process starts, `SharDirectory::new` reads whatever's currently on disk as if this were the very first time this directory had ever been a shar: every character gets assigned `peer = 0` and a sequential id from a counter that always starts at 0 (see Tree, below). That scheme is only correct as a one-time *genesis* bootstrap, which is all it's ever had to be so far -- every restart up to now really has been the first load.

A restart on a directory that's been edited *outside* the shar breaks that assumption. The process has no memory that this directory was ever a shar with real history; it just re-walks the (now different) bytes on disk and reassigns peer-0 ids from scratch, in whatever order the new content happens to be in. Two concrete failure modes follow from that:
- `SharDirectory::add_crdt` already treats a repeated `(id, peer)` key as "already applied" and no-ops it (`if self.characters.contains_key(&(id, peer)) { return Ok(None); }`). Since both this replica's fresh reload and the network's real history use small sequential counters starting at 0, a key collision is likely -- and when it happens, a genuine incoming op for that key gets silently dropped instead of applied, with no error raised anywhere.
- Independent of any collision, this replica's entire previous networked history is simply gone the moment the process restarts and re-derives everything from the new on-disk bytes -- nothing persisted it anywhere.

**Not a quick fix.** Actually fixing this needs two separate pieces of new machinery, not a tweak to the counter: real persistence of CRDT state across restarts (so the peer-0 genesis path only ever fires for a directory that's never been a shar before, not on every restart), and a reconciliation path for content that changed on disk by something that wasn't CRDT-aware (diffing current bytes against the last-known rendered text and synthesizing insert/delete ops for just the delta, anchored on whatever identities are still shared between old and new). See `ARCHITECTURE.md`'s Tree section for how the genesis bootstrap works today.

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
