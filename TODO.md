# Shar CRDT — TODO

## 🔴 Blockers — will panic at runtime

- [x] **`check_line` panics on missing line** (`src/shar/core/tree.rs:74`) — `self.tree[&line_number]` must become `self.tree.get(&line_number).ok_or(...)` so out-of-bounds returns `Err` instead of crashing. The whole `num_errors` loop depends on this.
- [x] **Backward underflow** (`src/shar/core/tree.rs:153`) — `line_number - distance_from_og` underflows `u16` past line 0. Use checked/saturating sub and stop at 0.
- [x] **Last-in-line append overflows** (`src/shar/core/tree.rs:181`) — `[parent_index + 1]` is out of bounds when the parent is the last element (the common append case). Treat "no successor" as a plain push.
- [x] **`to_bytes` id→char panic** (`src/shar/types.rs:68`) — `char::from_u32(self.id).unwrap()` panics for ids in the surrogate range or `> 0x10FFFF`. Serialize the `u32` directly to 4 bytes.

## 🟠 CRDT correctness — the convergence core

- [x] **Walk the sibling run** (`src/shar/core/tree.rs:181-204`) — replace the single-neighbor compare with: step over every child of the parent that outranks the new node by `(id desc, peer_id asc)`, insert before the first it beats.
- [x] **Add a parent id to each element** (`Line` in `src/shar/core/tree.rs:16`) — currently `(id, peer_id, atom)` can't tell "child of X" from "descendant of X", so the walk can't know where the run ends. Add the parent field.
- [x] **Tombstones for deletes** — `RemoveChar`/`ChangeChar` need mark-not-remove handling in `add_crdt`, or concurrent "insert after deleted node" breaks.
- [ ] **Global id uniqueness** — advance `char_counter` (or derive ids from `this_id` + counter) for *local* inserts, not just `add_file`. Confirm `(id, peer_id)` is unique across peers.
- [ ] **Parent lookup only matches on bare `id`** (`check_line`, the `parent_col` projection lookup) — both do `element.0 == parent_id` with no peer check. If two peers' local counters collide on the same id, this silently resolves to whichever node comes first and anchors the whole sibling walk to the wrong parent. Needs `parent_id` to travel everywhere as `(IdSize, PeerIdSize)`, not a bare id — `Line`'s parent field needs a `parent_peer` slot too, and `add_crdt`/`Entry::add_crdt` need a `parent_peer` param threaded through. The sibling-walk tie-break itself (id desc, peer asc) is already correct and doesn't need to change.
- [ ] Correctly add new lines to SharFile

## 🟡 Data model — decide before building more on top

- [ ] **Reconsider `HashMap<LineSize, Line>` keyed by ordinal** (`src/shar/core/tree.rs:35`) — newline insert/delete renumbers every later key (O(n)) and concurrent line inserts collide. Decided direction: split into two structures instead of one —
  - `tree`: a single flat `Line` (just `Vec<(...)>`, no per-line partitioning at all) covering the whole file, `\n`/`\r` are ordinary elements in it. This is the actual CRDT state — the only thing that needs to satisfy merge/convergence.
  - `projection: Vec<Vec<(IdSize, PeerIdSize)>>`: a line/column → node-id index, purely derived from `tree`, never merged over the network. `add_crdt` patches it incrementally in the same call that mutates `tree` (no separate eventing layer needed since ownership is already a direct call chain: `SharQueue` → `SharDirectory` → `SharFile`).
  - Column lookup within a line still walks the small `projection` row / `tree` slice directly — no fancier index needed at that scale.
- [ ] **Consistent anchor sentinel** (`add_file`, `src/shar/core/tree.rs:41`) — the reserved `(0,0,Atom(0))` anchor exists only on line 0; every other line has no parent for column-0 inserts.

## 🟢 Serialization pipeline — pick one layout

- [x] **Unify the byte sizes** — `CRDT::to_bytes` → `[Atom; 9]`, `Operation::to_bytes` → `[Atom; 11]`, `SharBuffer::write` → `[u8; 14]`. Three different sizes. Nail down one concrete layout.
- [x] **Actually serialize all fields** (`src/shar/types.rs:58`) — `parent_id`, `anchor_id`, `peer_id` are never written.
- [ ] **Write a matching `from_bytes`** — needed before the network path; `SharBuffer::read` is currently a stub returning `Vec::new()`.

## ⚪ Cleanup — do last, once logic settles

- [ ] **Dead code** (`src/shar/core/tree.rs:171-176`) — the post-loop `if parent_index.is_none()` block is unreachable; also drops a `must_use` Result.
- [ ] **`SharDirectory::add_crdt` stub** (`src/shar/core/tree.rs:280`) — returns `Ok(())` without routing to a file by `file_path`.
- [x] **Unused imports** — `core::num`, `axum::extract::Path`, redundant `Error` import in `tree.rs`.
- [ ] **`cargo fix` pass** — clear the 32 warnings so real ones stop hiding.
