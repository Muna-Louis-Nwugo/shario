# Known Issues

Bugs that are known/observed but not yet root-caused or fixed. Unlike
[`TODO.md`](TODO.md), not all of these are guaranteed to have a confirmed cause or plan to fix them.


## Two peers joining the same session from genuinely different starting documents can silently drop, misattach, or spuriously delete content -- not just "duplicate" it

**Symptom:** peer A creates (or already has) a shar with some real content in it; peer B joins the same session with its own workspace folder already containing different (or just independently-existing) content for the same file path. Peer B joining corrupts the state of the Shar. This is the only known situation where the system does not currently converge. To give an idea of what this looks like, consider the following three examples:

- Peer A has `"Hi"` (ids 1-2); peer B joins with `"Yo"` (also ids 1-2). Hydrating A's content into B: both characters collide with B's own ids 1 and 2, so they're silently dropped. B never sees any of `"Hi"`, and A never sees any of `"Yo"`

- Peer A has `"Hello"` (ids 1-5); peer B joins with  `"Hi"` (ids 1-2). Ids 1-2 collide and drop as above, but id 3 (`'l'`) doesn't collide, so it inserts parented on whatever B's own id 2 is (`'i'`). B ends up with `"Hillo"`, instead of `"Hello"`.

- Peer A backspaces a typo, deleting its own id 2. Peer B's own unrelated id 2 is a real character it actually typed. The remote remove tombstones B's character instead -- something B never touched just disappears.

**Mechanism: confirmed.** Every peer's genesis bootstrap (`SharDirectory::new()`) independently assigns peer `"0"` and sequential ids from 0, with no awareness of any other replica -- so two peers' pre-existing content collides on identical `(id, peer)` keys by coincidence, not because anything's actually shared. `add_crdt`'s deduplication check treats a collision as "already applied" and drops it, even though it's an unrelated character; survivors past the local id range get misattached onto whatever local character shares their claimed parent's key. `remove_crdt` is worse -- it tombstones whatever's at a key with no check it's the right character, so a colliding remote delete can spuriously remove local content. Possibly explains an earlier, unresolved observation in this project of a character being deleted that nobody actually deleted.


## Editing a shar's files outside the shar, between runs, can silently corrupt or lose history

**Symptom:** a peer disconnects (or the process just isn't running), someone edits the directory's files directly -- a plain text editor, a `git pull`, anything that isn't the shar itself -- and then the shar is started again on that same directory. Depending on exactly what changed, this can either silently drop a real incoming edit as a false "already applied" duplicate, or just quietly lose that replica's entire prior networked history (other peers' edits into what used to be this content, tombstones, all of it).

**Mechanism: confirmed.** There is currently no persistence of CRDT state across process restarts at all -- editing through socket.io only ever mutates the in-memory `characters`/`projection` (`src/shar/core/tree.rs`); nothing is ever flushed back to disk. Every time a `shario` process starts, `SharDirectory::new` reads whatever's currently on disk as if this were the very first time this directory had ever been a shar: every character gets assigned `peer = 0` and a sequential id from a counter that always starts at 0 (see Tree, below). That scheme is only correct as a one-time *genesis* bootstrap, which is all it's ever had to be so far -- every restart up to now really has been the first load.

A restart on a directory that's been edited *outside* the shar breaks that assumption. The process has no memory that this directory was ever a shar with real history; it just re-walks the (now different) bytes on disk and reassigns peer-0 ids from scratch, in whatever order the new content happens to be in. Two concrete failure modes follow from that:
- `SharDirectory::add_crdt` already treats a repeated `(id, peer)` key as "already applied" and no-ops it (`if self.characters.contains_key(&(id, peer)) { return Ok(None); }`). Since both this replica's fresh reload and the network's real history use small sequential counters starting at 0, a key collision is likely -- and when it happens, a genuine incoming op for that key gets silently dropped instead of applied, with no error raised anywhere.
- Independent of any collision, this replica's entire previous networked history is simply gone the moment the process restarts and re-derives everything from the new on-disk bytes -- nothing persisted it anywhere.

**Suggested fix:** Fixing this requires devising a way for the shar's content to persist in memory between runs.

## Socket-mode benchmark runs intermittently take multiple seconds instead of milliseconds

**Symptom:** replaying the automerge-paper trace through `bench/trace-runner.js`
occasionally takes 8-10s instead of the usual ~3s, with `p50` latency in
the seconds and RSS growth well above normal. Not every run -- roughly
half, no obvious trigger from the outside.

**Mechanism: confirmed** When the benchmark runs, it provides the shar with a line hint of 0 for every character typed. The ring search depends on accurate line hints, so as the document grows, the ring search gets less and less efficient. Under artificially increased loads, this leads to threads getting "clogged" searching through thousands of characters to find the one with the correct (id, peer) combination

**Suggested fix:**  see [TODO.md](TODO.md)'s entry talking about the ring search. This is only an issue under artificially increased load as would common during a benchmark, it is unlikely to surface during usual use.
