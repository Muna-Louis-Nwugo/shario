# Shar CRDT — TODO

## 🟠 CRDT correctness — the convergence core

- [x] **Global id uniqueness** — advance `char_counter` (or derive ids from `this_id` + counter) for *local* inserts, not just `add_file`. Confirm `(id, peer_id)` is unique across peers.
- [x] **Parent lookup only matches on bare `id`** (`check_line`, the `parent_col` projection lookup) — both do `element.0 == parent_id` with no peer check. If two peers' local counters collide on the same id, this silently resolves to whichever node comes first and anchors the whole sibling walk to the wrong parent. Needs `parent_id` to travel everywhere as `(IdSize, PeerIdSize)`, not a bare id — `Line`'s parent field needs a `parent_peer` slot too, and `add_crdt`/`Entry::add_crdt` need a `parent_peer` param threaded through. The sibling-walk tie-break itself (id desc, peer asc) is already correct and doesn't need to change.
- [x] Correctly add new lines to SharFile
- [x] **Coordinate staleness across replicas** (`src/shar/core/tree.rs` `add_crdt`) — `coordinates` is only valid relative to the projection state it was computed against. A remote op carries `coordinates` captured on the sender's replica; if a concurrent edit earlier in that line has already landed locally by the time the op is applied, the index has shifted and `coordinates` now points at the wrong element. Local/sequential use (the IDE handing back the position of a parent it just tracked itself) is fine and doesn't need this. This is structural, not cosmetic — needs `parent_id`/`parent_peer` to be resolved to a live position on the receiving replica (id → position lookup) rather than trusting a foreign `(line, col)` directly. Do this after `SharDirectory` and tombstones.
- [x] **Out-of-order delivery: parent not received yet** (network/queue layer, not `SharFile`) — a remote op can arrive whose `parent_id`/`parent_peer` this replica hasn't seen yet (causal delivery isn't guaranteed by the network). Right now `add_crdt`/`find_crdt`/`find_tombstone` all assume the parent already exists locally. Planned fix: a "pending" set — an op whose parent is missing gets held there instead of applied; every time a *new* op is successfully applied, check the pending set for anything waiting on it and re-attempt those. Purely a network/queue concern, not `SharFile`'s — do this once `SharQueue` actually exists and drains.
- [ ] **File paths need to be relative to the shar root, not absolute** (`SharFile::file_path`, `AddOperation::file_path`, `RemoveOperation::file_path` in `tree.rs`/`types.rs`) — every path currently stored/passed around is an absolute `PathBuf` on the local machine. That's meaningless once an `Operation` crosses the network to a peer whose shar root lives at a different absolute location. Needs paths normalized to be relative to the shar's root directory before they're put on the wire (and resolved back to a local absolute path on receipt), not the full local path as-is.

## 🟡 Data model — decide before building more on top

- [x] **Reconsider `HashMap<LineSize, Line>` keyed by ordinal** (`src/shar/core/tree.rs:35`) — newline insert/delete renumbers every later key (O(n)) and concurrent line inserts collide. Decided direction: split into two structures instead of one —
  - `tree`: a single `HashMap<(IdSize, PeerIdSize), Node>` — no ordinal/line keying, no per-line partitioning. This is the actual CRDT state; the shar itself doesn't care about order, so parent lookup by id is O(1) instead of the current line-scoped forward/backward line search. Each `Node` holds `value`/`parent_id`/`parent_peer` plus a child/sibling pointer (ordered list of children, or next-sibling) so that finding a node's siblings for the insert tie-break doesn't require scanning every node for a matching `parent_id`.
  - `projection: Vec<Vec<(IdSize, PeerIdSize)>>`: a line/column → node-id index, purely derived from `tree` by walking child pointers from the root, never merged over the network. This is what the IDE actually consumes, since it works on positions, not ids. `add_crdt` patches it incrementally in the same call that mutates `tree` (no separate eventing layer needed since ownership is already a direct call chain: `SharQueue` → `SharDirectory` → `SharFile`).
  - Column lookup within a line still walks the small `projection` row directly — no fancier index needed at that scale.
- [x] **Cut the `CRDT` middleman** (`src/shar/types.rs:34`, `src/shar/core/tree.rs` `add_crdt`) — `add_crdt` currently destructures an incoming `CRDT` into a tuple with the same fields it already has (`id, peer, value, parent, parent_peer`). Once storage moves to an id-keyed `HashMap<(IdSize, PeerIdSize), Node>`, consider making `Node` the single type used both for wire transfer and tree storage, rather than converting `CRDT` → tuple/`Node` on every insert. `Node` would be a superset of `CRDT`'s fields plus local-only bookkeeping (child/sibling pointers for O(1) sibling lookup) that never gets serialized.

## 🟢 Serialization pipeline — pick one layout

- [ ] **Write a matching `from_bytes`** — needed before the network path; `SharBuffer::read` is currently a stub returning `Vec::new()`.

## ⚪ Cleanup — do last, once logic settles

- [ ] **Dead code** (`src/shar/core/tree.rs:171-176`) — the post-loop `if parent_index.is_none()` block is unreachable; also drops a `must_use` Result.
- [x] **`SharDirectory::add_crdt` stub** (`src/shar/core/tree.rs:280`) — returns `Ok(())` without routing to a file by `file_path`.
- [ ] **`cargo fix` pass** — clear the 32 warnings so real ones stop hiding.
