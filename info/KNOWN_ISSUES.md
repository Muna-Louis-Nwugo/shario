# Known Issues

Bugs that are known/observed but not yet root-caused or fixed. Unlike
[`TODO.md`](TODO.md), not all of these are guaranteed to have a confirmed cause or plan to fix them.


## A local edit racing an already-reported remote sibling can leave the IDE permanently misordered

**Symptom:** confirmed via live testing (2026-11-05) across sustained concurrent typing between two peers -- after a burst of edits from both sides, the two IDEs converge almost everywhere, but a few spots stay permanently wrong (e.g. one side shows `oo` where the other shows `ss`), with no further sync ever correcting it.

**Mechanism: confirmed.** A third peer with no local edits in flight always converges correctly, because every op it receives arrives as a `"network-add"`, and its own shar re-runs the real tie-break scan against its own current tree for every op it applies -- the predecessor it reports is always freshly derived from "what does my tree look like right now." A peer's *own* typed content never goes through that mechanism at all: it renders instantly via a raw local keystroke, bypassing the live-identity-splice entirely. So the failure is specific to this sequence: peer A types a character locally (rendered instantly, `ide-add` delayed in transit); meanwhile a remote op R, parented on the same existing character, reaches peer A's shar first and gets told "insert after X" -- correct, since the shar has no idea A's local edit is coming. Peer A's IDE splices R in after X, landing it *before* A's own already-rendered character. When A's delayed `ide-add` finally arrives and wins the real tie-break against R, peer A's shar correctly reorders its own tree -- but nothing ever tells peer A's IDE that R's reported predecessor is now stale; `IdeAddConfirmed` only ever returns `(tag, id, peer)`. Peer A's IDE is stuck at the wrong order permanently.

Fix direction (agreed 2026-11-05, not yet implemented): extend `IdeAddConfirmed` to also carry the identity of whatever the shar's tie-break scan found already sitting immediately after the newly-confirmed character, if any. The IDE checks whether its own buffer already has that identity positioned right after the new character; if not, it moves it there -- a reposition the IDE executes on instruction, not a tie-break decision it makes itself.

## New files aren't hydrated or synced at all -- only pre-existing ones at a matching path

**Symptom:** a file that exists on one peer but not another (created mid-session, or just never present on the joining peer's disk) never reaches the peer that doesn't have it -- neither as part of one-time catch-up, nor as an ongoing live edit.

**Mechanism: confirmed.** Three separate gaps: `SharDirectory::find_file`/`add_crdt` (`src/shar/core/tree.rs`) only route to files already present in `sub_files`, built once at `SharDirectory::new()` from whatever's on disk at startup -- there's no code path that creates a new `SharFile` entry at runtime. `shario-vscode/extension.js` has no handling for file-creation events at all (no `onDidCreateFiles` listener or equivalent). And when a network op (gossip or hydration's replay) names a file path the receiving replica doesn't have, `add_crdt` returns `Err("File not found")`, which `add_network_operation_inner`'s error handling doesn't distinguish from "parent not arrived yet" -- it blindly backlogs the op under `(parent_id, parent_peer)` regardless of error type. That key can never resolve, since nothing will ever create the missing file, so the content is silently, permanently lost, logged only as a misleading debug line. Not yet designed: both the runtime file-creation path and a real "receiving replica doesn't have this file yet" signal distinct from a missing parent.

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
