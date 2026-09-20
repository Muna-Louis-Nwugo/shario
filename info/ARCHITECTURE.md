# Shar Architecture

```
  ┌─────┐                                                                 ┌─────────┐
  │ IDE │                                                                 │ Network │
  └──┬──┘                                                                 └────┬────┘
     │                                                                         │
     │  websocket (socket.io)                       websocket (socket.io)      │
     │  "local" room                                 "network" room            │
     │                                                                         │
     │                       ┌──────────────────────────┐                      │
     └──────────────────────►│           Main           │◄─────────────────────┘
                             │   (axum + socketioxide)  │
                             │                          │
                             │  ┌──────────────────────┐│
                             │  │      SharQueue       ││
                             │  │  ┌────────────────┐  ││
                             │  │  │      Tree      │  ││
                             │  │  │ (SharFile /    │  ││
                             │  │  │  SharDirectory)│  ││
                             │  │  └────────────────┘  ││
                             │  └──────────────────────┘│
                             └──────────────────────────┘
```

Both connections are the same kind of link (a websocket, both ways) — the IDE just joins a `"local"` room and a peer joins a `"network"` room, so Main can route events to the right audience.

Both the IDE and (eventually) other Shar instances over the network connect to **Main** the same way: a websocket (socket.io) connection, joined into either the `"local"` room (an IDE) or the `"network"` room (another peer). Main owns one `SharQueue`, which owns the `Tree` — everything routes through that single mediator, so there's one place that decides what's actually been applied.

## IDE (client)

Currently a VS Code extension (`shario-vscode/extension.js`). Send-only in the sense that it never renders remote edits by walking the tree itself — it maintains its own local mirror (`docState`: one entry per line, each with an `anchor` and an array of `cells`) that always reflects what the editor already shows, and reconciles that mirror's identities against the server over the wire.

**The full contract any IDE plugin needs to implement — every event, every field, every invariant — is [`PLUGIN_SPECIFICATIONS.md`](PLUGIN_SPECIFICATIONS.md).** Summary of the key ideas:

- **Every character gets a disposable local `tag`** the moment it's typed — just a correlation id (like a request id), not real CRDT identity. It's replaced with the server's real `(id, peer)` once confirmed.
- **Inserts reference their parent by identity or tag, never by position** — `parent_id`/`parent_peer` if the parent is already confirmed, `parent_tag` if the parent is this same client's own not-yet-confirmed character. A `line_hint` (the current row) is sent alongside purely as a performance hint for the server's search — it has no bearing on correctness.
- **Deletes reference their target the same way**: by real `(id, peer)` if known, or deferred until a pending `tag` resolves.
- **Pre-existing content** (loaded from disk before the client connects) gets its real identities filled in by the server's one-time `initial-state` event, matched by position — safe there specifically because nothing's been edited since the client saved and the server loaded the same bytes.
- **`maybeRestartServer`** watches for the server dying and restarts it automatically, so a crash recovers on its own instead of leaving the session stuck.

## Main (`src/main.rs`)

An `axum` HTTP server with a `socketioxide` websocket layer mounted on it, listening on `127.0.0.1:3000`. Owns a single `QueueWrap` (an `Arc<RwLock<SharQueue>>`) shared across every connection — **one shar server currently manages one directory**; there's no multi-tenancy yet.

Socket events handled:
- `join` — a connecting client declares itself `local` (an IDE) or not (a network peer) and which directory path it wants. The first `local` join initializes the real `SharQueue` for that path (loading existing files off disk) and sends back `initial-state`.
- `ide-add` / `remove` — from a local IDE; forwarded into `SharQueue::add_ide_operation`/`remove_ide_operation`.

Callbacks fired by the queue, each wired to actually emit over the socket:
- `ide_add_confirm_callback` → emits `ide-add-confirmed` to the `"local"` room, **immediately** on receipt — not gated on the character actually landing in the tree yet (see SharQueue below for why that matters). Retries on `InternalChannelFull` instead of dropping the confirmation if the socket's outgoing buffer is momentarily full.
- `ide_add_callback` → emits `network-add` to the `"network"` room, but only once the character is actually inserted into the tree.
- `ide_remove_callback` → emits `network-remove` to the `"network"` room.
- `network_add_callback` / `network_remove_callback` → currently just log; this is the seam where applying an *incoming* network op would notify local IDEs of the result. Not wired up yet — see Network below.

`--bench-internal <trace.json>`, checked before any of the above, replays a trace directly against a `SharQueue` in-process and exits — see `src/bench.rs` and [`BENCHMARK.md`](BENCHMARK.md).

## SharQueue (`src/shar/core/queue.rs`)

Mediates between the outside world (IDE, network) and the `Tree` — nothing touches the tree directly except through here. Its central job: **decouple identity assignment from tree insertion**, so that confirming a character to the client never has to wait on that character's parent chain actually resolving in the tree.

- **On every `IdeAdd`**: assigns a real `(id, peer)` and fires the confirm callback *immediately*, before even checking whether the parent exists yet. This is what makes "delete something the instant after you typed it" safe regardless of network/processing timing — the identity always exists once assigned, even if the tree insertion is still backlogged.
- **`tag_identities`** (`HashMap<u32, (IdSize, PeerIdSize)>`) records every tag's real identity the instant it's assigned, so a later message referencing that tag (`parent_tag`) can resolve it.
- **Three backlogs**, all draining the same way (extend a worklist, then loop until it's empty — never recurse back into the draining function itself, to keep a long dependency chain from blowing the stack):
  - `ide_backlog` — an `IdeAdd` waiting on a parent *identity* that hasn't landed in the tree yet.
  - `ide_tag_backlog` — an `IdeAdd` waiting on a parent *tag* that hasn't been registered yet. This one is purely scheduling-order jitter, not a real dependency: `socketioxide` spawns each socket event as an independent tokio task with no ordering guarantee, so even though a client sends parent-then-child in order, the server can process them in either order. Bounded by scheduling jitter only — always resolves.
  - `network_backlog` — a remote op waiting on a target (parent for an add, the thing itself for a remove) this replica hasn't seen yet, since causal delivery isn't guaranteed by the network. Same drain pattern as the ide backlogs.

## Tree (`src/shar/core/tree.rs`)

`SharDirectory` recursively mirrors a directory of files; each file is a `SharFile` holding the actual CRDT state:

- **`characters: HashMap<(IdSize, PeerIdSize), CrdtRelation>`** — the real CRDT data: every character's value, its parent's identity, and whether it's tombstoned. This is what would get merged over the network between replicas — order-independent, identity-keyed.
- **`projection: Vec<Vec<(IdSize, PeerIdSize)>>`** — a line/column → identity index, derived from `characters`. This is what the IDE actually consumes (it works in row/col, not raw identities), and it's what makes sibling tie-break (which of a parent's several children does a new insert land next to) and physical insert/remove fast to apply locally. It's also how a freshly-joined IDE bootstraps identities for pre-existing content (`identities()` → `initial-state`), matched by position.
- **`find_crdt`** ring-searches outward from a hint (a row number, not ground truth) until it finds where an identity currently sits in `projection`. Cost scales linearly with how far the hint is from the real position — a known, currently-unfixed bottleneck when processing order and send order diverge under a fast burst (see [`TODO.md`](TODO.md)/[`KNOWN_ISSUES.md`](KNOWN_ISSUES.md)). Not a correctness issue: a wrong hint costs a wider search, never a wrong result.
- Newline characters split a `projection` line into two instead of occupying a column in one; a tombstoned parent's insertion point is resolved via `find_tombstone`, which climbs to the nearest live ancestor.

## Network — work in progress

Not built yet. The design intent, and what already exists toward it:

- The wire events (`network-add`/`network-remove`) and the queue-side logic to apply them (`SharQueue::add_network_operation`/`remove_network_operation`, with their own backlog for out-of-order arrival) already exist and are tested — this is the same machinery a second `shar` server would use to receive another replica's edits.
- **What's missing**: a socket handler in `main.rs` that actually accepts a peer connection into the `"network"` room and calls those queue methods on incoming `network-add`/`network-remove` events — right now only the *outgoing* broadcast side (from local edits) is wired up. `network_add_callback`/`network_remove_callback` (what should notify local IDEs once a remote op lands) are still just log statements.
- Also needed before this is real: `Remove`'s `file_path` and `NetworkAdd`'s file paths are currently absolute, local-machine paths — meaningless once an op crosses to a peer whose shar root lives somewhere else on disk. These need to travel as paths relative to the shar's root instead (see [`TODO.md`](TODO.md)).
