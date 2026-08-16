# Shar CRDT — TODO

## 🟠 CRDT correctness — the convergence core

- [ ] **Global id uniqueness** — advance `char_counter` (or derive ids from `this_id` + counter) for *local* inserts, not just `add_file`. Confirm `(id, peer_id)` is unique across peers.
- [ ] **Parent lookup only matches on bare `id`** (`check_line`, the `parent_col` projection lookup) — both do `element.0 == parent_id` with no peer check. If two peers' local counters collide on the same id, this silently resolves to whichever node comes first and anchors the whole sibling walk to the wrong parent. Needs `parent_id` to travel everywhere as `(IdSize, PeerIdSize)`, not a bare id — `Line`'s parent field needs a `parent_peer` slot too, and `add_crdt`/`Entry::add_crdt` need a `parent_peer` param threaded through. The sibling-walk tie-break itself (id desc, peer asc) is already correct and doesn't need to change.
- [ ] Correctly add new lines to SharFile

## 🟡 Data model — decide before building more on top

- [ ] **Reconsider `HashMap<LineSize, Line>` keyed by ordinal** (`src/shar/core/tree.rs:35`) — newline insert/delete renumbers every later key (O(n)) and concurrent line inserts collide. Decided direction: split into two structures instead of one —
  - `tree`: a single `HashMap<(IdSize, PeerIdSize), Node>` — no ordinal/line keying, no per-line partitioning. This is the actual CRDT state; the shar itself doesn't care about order, so parent lookup by id is O(1) instead of the current line-scoped forward/backward line search. Each `Node` holds `value`/`parent_id`/`parent_peer` plus a child/sibling pointer (ordered list of children, or next-sibling) so that finding a node's siblings for the insert tie-break doesn't require scanning every node for a matching `parent_id`.
  - `projection: Vec<Vec<(IdSize, PeerIdSize)>>`: a line/column → node-id index, purely derived from `tree` by walking child pointers from the root, never merged over the network. This is what the IDE actually consumes, since it works on positions, not ids. `add_crdt` patches it incrementally in the same call that mutates `tree` (no separate eventing layer needed since ownership is already a direct call chain: `SharQueue` → `SharDirectory` → `SharFile`).
  - Column lookup within a line still walks the small `projection` row directly — no fancier index needed at that scale.
- [ ] **Consistent anchor sentinel** (`add_file`, `src/shar/core/tree.rs:41`) — the reserved `(0,0,Atom(0))` anchor exists only on line 0; every other line has no parent for column-0 inserts.
- [ ] **Cut the `CRDT` middleman** (`src/shar/types.rs:34`, `src/shar/core/tree.rs` `add_crdt`) — `add_crdt` currently destructures an incoming `CRDT` into a tuple with the same fields it already has (`id, peer, value, parent, parent_peer`). Once storage moves to an id-keyed `HashMap<(IdSize, PeerIdSize), Node>`, consider making `Node` the single type used both for wire transfer and tree storage, rather than converting `CRDT` → tuple/`Node` on every insert. `Node` would be a superset of `CRDT`'s fields plus local-only bookkeeping (child/sibling pointers for O(1) sibling lookup) that never gets serialized.

## 🟢 Serialization pipeline — pick one layout

- [ ] **Write a matching `from_bytes`** — needed before the network path; `SharBuffer::read` is currently a stub returning `Vec::new()`.

## ⚪ Cleanup — do last, once logic settles

- [ ] **Dead code** (`src/shar/core/tree.rs:171-176`) — the post-loop `if parent_index.is_none()` block is unreachable; also drops a `must_use` Result.
- [ ] **`SharDirectory::add_crdt` stub** (`src/shar/core/tree.rs:280`) — returns `Ok(())` without routing to a file by `file_path`.
- [ ] **`cargo fix` pass** — clear the 32 warnings so real ones stop hiding.
