# Shar Architecture

```
                                ┌───────────┐
                                │    IDE    │
                                └───┬───────┘
                                    │    ▲
                      local edits   │    │  (id, peer) +
                                    │    │  adds from network
                                    ▼    │
┌───────────┐                   ┌───────────┐                    ┌───────────┐
│           │◄──────────────────│           │                    │           │
│ SharQueue │   local edits     │  Server   │                    │           │
│           │──────────────────►│           │───────────────────►│           │
│           │  (id, peer) /     └───────────┘    local edits     │           │
│           │adds from network        ▲                          │           │ 
│           │                         │ discovered peer's address│           │
│ ┌────────┐│                   ┌───────────┐                    │           │
│ │        ││◄──────────────────│           │◄────────────────── │           │
│ │  Tree  ││  edits from peer  │  Client   │  edits from peer   │           │
│ │        ││                   │           │                    │  Network  │
│ └────────┘│                   │           │                    │  (Peers)  │
└───────────┘                   └───────────┘                    │           │ 
                                      ▲                          │           │
                                      │ discovered peer's address│           │
                                      │                          │           │ 
                               ┌───────────┐                     │           │ 
                               │           │◄────────────────────│           │
                               │ Discovery │  other shars'       │           │
                               │           │  broadcasts         │           │
                               │           │────────────────────►│           │
                               └───────────┘  our broadcast:     └───────────┘
                                              session + IP
```

The diagram is a representation of a single peer's shar. This means every shar has both a client and a server. The client is used to connect to the other shars' servers, and it basically acts as a listener for changes that are made over there. The server takes changes made on the local machine and sends it out to everyone involved. The server also provides information back to the ide that the operation originated from. For more information on that, see the [IDE](#ide-client) section of this document.

## IDE (client)

The IDE never renders a remote edit by walking the tree itself — it keeps its own local mirror instead (`docState`: one entry per line, each with an `anchor` plus an array of `cells`) that stays in lockstep with whatever the editor shows, and reconciles that mirror's identities against the server as messages come in over the wire. Send-only, in that sense: nothing comes back from the server except confirmations and identity information.

**[`PLUGIN_SPECIFICATIONS.md`](PLUGIN_SPECIFICATIONS.md) is the full contract any IDE plugin has to implement.** The key ideas, briefly:

- **Every typed character gets a disposable local `tag`** the instant it's typed — nothing more than a correlation id, not a real CRDT identity, swapped for the server's real `(id, peer)` once confirmed.
- **An insert never references its parent by position, only by identity or tag** — `parent_id`/`parent_peer` if that parent's already confirmed, `parent_tag` if it's this same client's own character that hasn't been confirmed yet. The `line_hint` sent alongside (the current row) is purely a performance hint for the server's search, with no effect on correctness.
- **Deletes work the same way**: target by real `(id, peer)` if it's known, or hold the delete until a pending `tag` resolves.
- **Content already on disk** before the client connected gets its real identities from the server's one-time `initial-state` event, matched by position — safe only because nothing's been edited between the client saving and the server loading those same bytes.
- **`maybeRestartServer`** watches for the server process dying and brings it back automatically, so a crash is a recoverable blip instead of a stuck session.

## Server (`src/main.rs`)

An `axum` HTTP server with `socketioxide` mounted on top as a websocket layer. It owns one `QueueWrap` (an `Arc<RwLock<SharQueue>>`) shared by every connection.

Socket events it handles:
- `join` — a connecting client says whether it's `local` (an IDE) or not (a network peer), and which directory it wants. The first `local` join is what actually initializes the real `SharQueue` for that path (loading whatever's already on disk) and sends back `initial-state`. A non-local join just lands in the `"network"` room for now — peer connections come in through this same socketioxide server, not a separate raw listener.
- `ide-add` / `remove` — sent by a local IDE, forwarded straight into `SharQueue::add_ide_operation`/`remove_ide_operation`.
- `network-add` / `remove`

Callbacks the queue fires, each wired to emit back over the socket:
- `ide_add_confirm_callback` emits `ide-add-confirmed` to the `"local"` room **immediately**, not waiting on the character to actually land in the tree (see SharQueue below for why that matters). Retries instead of dropping the confirmation if `InternalChannelFull` hits because the socket's outgoing buffer is momentarily full.
- `ide_add_callback` emits `network-add` to the `"network"` room, but only once the character's actually been inserted into the tree.
- `ide_remove_callback` emits `network-remove` to the `"network"` room.
- `network_add_callback` / `network_remove_callback` are still just log statements — this is where applying an *incoming* network op would eventually notify local IDEs. Not wired up yet.

Alongside the server itself, this also starts up a UDP socket (see Discovery below) and the IDE-facing `TcpListener`, currently bound to `127.0.0.1:3000` — there's an open `TODO` in the code about also binding something LAN-reachable for peers, still unresolved.

`--bench-internal <trace.json>`, checked before any of this, replays a trace directly against a `SharQueue` in-process and exits — see `src/bench.rs` and [`BENCHMARK.md`](BENCHMARK.md).

## Client (planned, not built yet)

Once Discovery hears a new peer, the Client is meant to dial *into* that peer's Server using a socket.io client (`rust_socketio` — `socketioxide`'s server expects an actual socket.io session, Engine.IO handshake and socket.io's own packet framing included, not a bare websocket), send a `join` with `{local: false}` to land in that peer's `"network"` room, and listens for `network-add` and `network-remove` events.

The handshake is intended to work both ways, but each peer goes about it their own way. Because every peer is constantly broadcasting its identity, whenever another peer finds it, it immediately asks to join its `"network"` room. Then, the second peer sends its entire state back to the first peer to update itself. This works out being bidirectional, since if there are 2 peers, they find each other and join each other's rooms, which results in them receiving each other's "full state"s.

What travels as "full state" is the `characters` map — every CRDT's value, parent, and `deleted` flag — not `projection`, which drops tombstoned entries entirely and would leave a freshly-caught-up peer unable to resolve anything whose parent was already deleted. On the receiving end, it gets broken apart and applied one CRDT at a time through the existing `add_network_operation` path (through the same `QueueWrap` the server already uses), rather than through a dedicated bulk-import. Slower than it could be, but a deliberate choice rather than building bulk-CRDT machinery that isn't needed yet.

Again, since discovery and dialing happen independently on each side, two peers can end up each dialing the other, forming two redundant connections instead of one.

## Discovery (UDP)

A `UdpSocket` bound to `0.0.0.0:3000` — not localhost, since it has to be reachable from other machines on the LAN — runs two things concurrently, each its own `tokio::spawn`ed task:
- **Broadcasts** a `BroadcastMessage { shar_id, ip_address }` — `shar_id` a petname-generated session name (three random hyphenated words), `ip_address` this machine's own local IP — bincode-encoded, to `255.255.255.255:3000`, roughly every 10ms.
- **Listens** for everyone else doing the same thing, decoding each incoming datagram back into a `BroadcastMessage`.

What actually happens once a new peer is heard — deciding it's genuinely new, then handing its address to the Client to dial — is a `TODO` sitting right at the point in the code where it belongs, not built yet. Two other things are also left open on purpose: how a newly-dialed-in peer picks a safe `(id, peer)` identity with no coordinator involved, and what happens if a chosen (not generated) session name collides with one already in use on the network.

## SharQueue (`src/shar/core/queue.rs`)

Everything from the outside world — IDE or network, whichever path it arrived on — goes through here before it ever touches the `Tree`; nothing touches the tree directly. Its central job is decoupling identity assignment from tree insertion, so confirming a character back to the client never has to wait on that character's whole parent chain actually resolving in the tree.

- **Every `IdeAdd`** gets a real `(id, peer)` assigned and its confirm callback fired *immediately*, before the parent's even been checked for. That's what makes deleting something the instant after typing it safe regardless of timing — the identity exists the moment it's assigned, whether or not the tree insertion itself is still backlogged.
- **`tag_identities`** (`HashMap<u32, (IdSize, PeerIdSize)>`) records a tag's real identity the moment it's assigned, so anything that later references that tag via `parent_tag` can resolve it.
- **Three backlogs**, and all three drain the same way — extend a worklist, then loop until it's empty, never recursing back into the draining function itself, so a long dependency chain can't blow the stack:
  - `ide_backlog` holds an `IdeAdd` waiting on a parent *identity* that hasn't landed in the tree yet.
  - `ide_tag_backlog` holds an `IdeAdd` waiting on a parent *tag* that hasn't been registered yet — purely scheduling-order jitter, not a real dependency, since `socketioxide` spawns each socket event as its own independent tokio task with no ordering guarantee: a client can send parent-then-child in order and still have the server process them in either order. Bounded only by that jitter, so it always resolves.
  - `network_backlog` holds a remote op waiting on a target — the parent, for an add, or the thing itself, for a remove — that this replica hasn't seen yet, since the network gives no guarantee of causal delivery. Same drain pattern as the ide backlogs, and the mechanism the Client's one-at-a-time bulk catch-up leans on to resolve anything that arrives out of order.

## Tree (`src/shar/core/tree.rs`)

`SharDirectory` recursively mirrors a directory full of files; each file is a `SharFile` holding the real CRDT state:

- **`characters: HashMap<(IdSize, PeerIdSize), CrdtRelation>`** is the actual CRDT data — every character's value, its parent's identity, and whether it's been tombstoned. This is what would get merged between replicas over the network — order-independent, keyed by identity — and it's what a Client's full-state catch-up sends (tombstones included; see Client above for why `projection` alone isn't enough).
- **`projection: Vec<Vec<(IdSize, PeerIdSize)>>`** is a line/column-to-identity index derived from `characters`. This is what the IDE actually works with, since it deals in row/col rather than raw identities, and it's also what makes sibling tie-break (deciding where a new insert lands among a parent's existing children) and physical insert/remove fast to apply locally. A freshly-joined IDE also bootstraps its identities for pre-existing content from this, via `identities()` → `initial-state`, matched by position.
- **`find_crdt`** ring-searches outward from a hint — a row number, not ground truth — until it finds where an identity currently sits in `projection`. The cost scales linearly with how far off that hint is from the real position, a known, still-unfixed bottleneck whenever processing order and send order diverge under a fast burst (see [`TODO.md`](TODO.md)/[`KNOWN_ISSUES.md`](KNOWN_ISSUES.md)). Not a correctness problem, though — a bad hint just costs a wider search, never a wrong result.
- A newline splits a `projection` line into two rather than occupying a column within one; a tombstoned parent's insertion point gets resolved via `find_tombstone`, which climbs up to the nearest live ancestor instead.

## Network — progress so far, and what's still needed

**Built so far:** the UDP discovery broadcast/listen loop (see Discovery above); the wire events (`network-add`/`network-remove`) and the queue-side logic that applies them (`SharQueue::add_network_operation`/`remove_network_operation`, with their own backlog for anything arriving out of order) — already tested, and reused as-is for the Client's planned catch-up path.

**Still needed:**
- Actually reacting to a discovered peer by dialing it (the Client, via `rust_socketio`) — marked with a `TODO`, not implemented yet.
- A socket handler in `main.rs` that accepts a peer connection and calls `add_network_operation`/`remove_network_operation` on an incoming `network-add`/`network-remove` — doesn't exist yet; only the outgoing broadcast side, from local edits, is wired up so far.
- `network_add_callback`/`network_remove_callback` actually notifying local IDEs once a remote op lands, rather than just logging it.
- The bidirectional full-state handshake itself, plus deciding where and how the peer-accepting `TcpListener` should bind (there's already a `TODO` for this in the code, separate from the IDE's `127.0.0.1` one).
- Peer-id generation and session-name collision handling — both deliberately left open for now.
- `Remove`'s `file_path` and `NetworkAdd`'s file paths are still absolute, local-machine paths, which is meaningless once an op crosses over to a peer whose shar root lives somewhere else entirely. These need to travel as paths relative to the shar's root instead (see [`TODO.md`](TODO.md)).
