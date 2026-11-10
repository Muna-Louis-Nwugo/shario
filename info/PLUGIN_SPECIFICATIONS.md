# Shar Plugin Specification

The contract between a `shar` server and an IDE plugin. If you're building a new plugin (for an editor other than VS Code) to join the shario ecosystem, this is the spec to implement against — the server doesn't care what editor is on the other end, only that it speaks this protocol correctly. [`shario-vscode`'s](https://github.com/Muna-Louis-Nwugo/shario-vscode) `extension.js` is the reference implementation; when in doubt, that's the working example.

This document covers the **IDE side of the protocol only** (events sent to/received from the `"local"` room). Peer-to-peer network sync (the `"network"` room, gossip, hydration) is a separate, already-built contract for server-to-server communication — a plugin never participates in it directly, and doesn't need to know how it works. What a plugin *does* need to handle is the result of that sync arriving locally, which shows up as the same `network-add`/`network-remove` events covered below, regardless of whether the edit originated from another peer's machine or just hadn't finished applying locally yet.

## Obtaining the binary

A plugin downloads a pre-built `shario` binary from the project's [GitHub Releases](https://github.com/Muna-Louis-Nwugo/shario/releases), caches it somewhere persistent (outside the workspace, so it survives across projects), and only downloads again if that cached copy is missing. The latest release always has exactly one asset per supported platform/architecture, named:

| Platform | Architecture | Asset name |
|---|---|---|
| Windows | x86_64 | `shario-windows-x64.exe` |
| Windows | arm64 | `shario-windows-arm64.exe` |
| macOS | Apple Silicon (arm64) | `shario-macos-arm64` |
| macOS | Intel (x86_64) | `shario-macos-x64` |
| Linux | x86_64 | `shario-linux-x64` |
| Linux | arm64 | `shario-linux-arm64` |

`https://github.com/Muna-Louis-Nwugo/shario/releases/latest/download/<asset-name>` always redirects to the correct file from whichever release is currently the newest *stable* one. On macOS/Linux, the downloaded file needs its executable bit set (`chmod +x`) before it can be run; a fresh download never has it.

Releases are tagged `shario_vMAJOR.MINOR.PATCH`. A bump in `MAJOR` means a breaking change was introduced — a plugin shouldn't treat that as a safe automatic update the way it would a minor/patch bump. 

## Launching the server

A plugin doesn't connect to a server that's just assumed to already be running — it's responsible for actually starting one, by running the `shario` binary with one of two subcommands:

```
shario [--port <N>] create [name]
shario [--port <N>] participate <name>
```

- **`create [name]`** — starts a brand-new session. `name` is optional; if omitted, the server picks a random one itself. Either way, **the plugin needs to know the actual name used** (to show the user, so they can share it with collaborators) — if the plugin wants to guarantee it knows the name without parsing server output, it should generate the name itself and always pass it explicitly, rather than ever relying on the server's own random fallback.
- **`participate <name>`** — joins an existing session by the name its creator used. `name` is required.
- **`--port <N>`** — which port the IDE-facing socket.io server listens on (default `3000`). Only needed to run more than one `shario` instance on the same machine at once; a plugin normally doesn't need to set this.

The plugin owns this process's lifecycle: spawn it, watch for it dying unexpectedly and restart it (with the same subcommand/name it was originally launched with — a restart has to rejoin the same session, not start a new one), and only then connect over socket.io as described below.

## Transport

A plain [socket.io](https://socket.io/) connection to the server's `/` namespace (default `http://127.0.0.1:3000`, or whatever `--port` was given). One socket connection per plugin instance.

## Connecting

Emit `join` once, immediately after connecting:

```jsonc
{ "local": true, "path": "<absolute path to the directory the server should load>" }
```

`local: true` is what makes this a plugin connection (as opposed to a network peer) — always set it. `path` must be a directory the server can read from disk; the server loads every file in it and initializes a `SharQueue` for that directory if one doesn't already exist. **A shar server currently manages exactly one directory for its whole lifetime** — the first `join`'s path wins; there's no per-connection isolation.

Right after joining, the server sends `initial-state` once per file it holds, unprompted:

```jsonc
{
  "file_path": "<absolute path>",
  "lines": [
    // one array per line; index 0 of each is that line's own anchor
    // (the root sentinel for line 0, the newline that created every other
    // line); every entry after that is one real character, in order.
    // each entry is a raw [id, peer, value] triple.
    [[0, 0, "\u0000"]],
    [[5, 1, "H"], [6, 1, "i"], [7, 1, "!"]]
  ]
}
```

**A plugin must use this to bootstrap identities for pre-existing content.** The real character values are included specifically so a plugin *could* build its document from this alone — but the reference implementation (`shario-vscode`) instead trusts whatever's already open in the editor for its values, and only uses this snapshot to fill in each existing character's real `(id, peer)` by matching position. **That position-match is only safe when the plugin's local content actually matches what the server just loaded** — true for a freshly-created session, or for reattaching to a server the same plugin instance just started, but **not guaranteed when joining an existing session from a separately-maintained copy of the files.** Two peers joining the same session with genuinely different starting content for the same file is a known, unresolved gap (see the shar repo's `info/KNOWN_ISSUES.md`) — it doesn't get rejected or flagged today, it silently produces a wrong result. A more careful plugin could instead trust the values in this payload directly rather than its own on-disk copy, sidestepping that risk at the cost of not being able to show real content until this event arrives.

## Sending an insert: `ide-add`

```jsonc
{
  "file_path": "<absolute path>",
  "parent_id": 5,          // or null
  "parent_peer": 1,        // or null
  "parent_tag": null,      // or a number
  "val": "x",              // exactly one character
  "tag": 42,               // plugin-invented, disposable, unique per connection
  "line_hint": 3
}
```

Field-by-field:

- **`parent_id` / `parent_peer`** — the real, already-confirmed identity of the character immediately before the insertion point (or the line's own anchor entry, if inserting at column 0 of a line). Use `(0, 0)` — the root sentinel — as the parent for the very first character of the document.
- **`parent_tag`** — set **instead of** `parent_id`/`parent_peer` when the parent is *this same connection's own character*, sent moments ago, that hasn't been confirmed yet (see `ide-add-confirmed` below). **Exactly one of `{parent_id, parent_peer}` or `{parent_tag}` must be set — never both, never neither.** This is what lets a plugin send a fast burst of edits without waiting for each one's round trip: reference the previous character you just sent by its tag, not its (not-yet-known) real identity.
- **`val`** — the character being inserted. A newline (or `\r`, `\u{0B}`, `\u{0C}`, `\u{85}`, `\u{2028}`, `\u{2029}`) is treated specially: it starts a new line rather than occupying a column, and everything after the split point on the current line moves down into the new line.
- **`tag`** — invented by the plugin, purely to correlate this specific send with its eventual `ide-add-confirmed` response (the same idea as a request id). Must be unique per connection; never reuse one still awaiting confirmation. Has nothing to do with real CRDT identity and is discarded once confirmed.
- **`line_hint`** — the row (0-indexed) the plugin currently believes this insertion lands on. **This is a performance hint only, checked by nothing.** A wrong value costs the server a wider search; it can never cause a wrong result or corrupt anything. Send your best current guess (typically wherever your local mirror thinks this line is) and don't worry about it going stale.

The server assigns a real `(id, peer)` and responds with `ide-add-confirmed` **immediately** — before it has even attempted to insert the character into its tree, let alone confirmed the parent exists. A plugin can safely reference a just-sent character (by tag) as the parent of its very next character, or even delete it, without waiting for anything.

## Receiving a confirmation: `ide-add-confirmed`

```jsonc
{ "tag": 42, "id": 118, "peer": 1 }
```

Sent to every plugin in the `"local"` room once per `ide-add` received, always, even if the character's parent hasn't resolved on the server yet. A plugin must:
1. Look up the pending character it sent with this `tag`.
2. Record its real `(id, peer)` in its own local mirror, replacing the tag.
3. If a `remove` for that same character was already requested locally but deferred (see below), send it now, using the identity just received.

## Receiving a late correction: `ide-add-complete`

```jsonc
{
  "file_path": "<absolute path>",
  "crdt": {
    "id": 118,
    "peer": 1,
    "relation": { "value": "x", "parent_id": 5, "parent_peer": 1, "deleted": false }
  },
  "row": 3
}
```

Sent once per `ide-add`, *after* `ide-add-confirmed` — once the character has actually been placed into the server's tree, which can be later than confirmation if its parent hadn't resolved yet. Correlate by `(id, peer)` (the character's real identity, not the long-gone `tag`).

The reason this exists: `relation.parent_id`/`parent_peer` here is the character's **real, final immediate predecessor in rendered order** — not necessarily the same parent the plugin originally sent in its `ide-add`. A plugin renders its own typed characters instantly, before any round trip, using whatever it assumed was correct at the time. If a concurrent edit from elsewhere lands in between before this event arrives, that assumption is now wrong, and nothing else will ever correct it. On receiving this, a plugin must:
1. Find its own already-rendered position for this `(id, peer)`.
2. Find the named predecessor's *current* position in its own local mirror.
3. If its own character isn't already sitting immediately after that predecessor, move it there — remove it from wherever it currently is and reinsert it after the predecessor, using the exact same logic as handling an incoming `network-add` below (this is deliberately the same shape of operation).

If either identity can't be found, skip silently — the same "don't guess" rule as everywhere else in this protocol. See the shar repo's `info/KNOWN_ISSUES.md` ("A local edit racing an already-reported remote sibling...") for the full trace of why this step is necessary.

## Sending a delete: `remove`

```jsonc
{ "file_path": "<absolute path>", "id": 118, "peer": 1, "row": 3 }
```

- **`id` / `peer`** — the real identity of the target. **If the target hasn't been confirmed yet** (you deleted something the instant after typing it, before `ide-add-confirmed` came back), **do not send this event yet** — hold it locally, keyed by the pending tag, and send it once the matching confirmation arrives with the real identity.
- **`row`** — same deal as `line_hint`: a ring-search starting point only, not checked, never a source of a wrong result.

To remove a line's own anchor (merging it into the line above), send the same event targeting that anchor's identity — there's no separate "remove whole line" event.

## Receiving a remote edit: `network-add` / `network-remove`

```jsonc
// network-add
{
  "file_path": "<absolute path>",
  "crdt": {
    "id": 203,
    "peer": 4,
    "relation": { "value": "y", "parent_id": 118, "parent_peer": 1, "deleted": false }
  },
  "row": 3
}

// network-remove
{ "file_path": "<absolute path>", "id": 203, "peer": 4, "row": 3 }
```

Sent to every plugin in the `"local"` room for any add/remove it didn't itself originate — another peer's edit arriving over the network, or this server's own catch-up of session history it just learned about. Both look identical to a plugin regardless of which one it actually was; there's no way to tell, and no need to.

**`relation.parent_id`/`parent_peer` on a `network-add` is always the real, already-resolved immediate predecessor in rendered order — never a raw CRDT-structural parent that still needs sibling tie-break applied to it.** This is the single most important property in this whole protocol for a plugin author: **a plugin never implements tie-break of any kind.** All of that — deciding where a concurrent insert from two different sources actually belongs relative to each other — happens entirely server-side, in the CRDT tree, before this event is ever sent. A plugin's job is mechanical: find the named identity's *current* position in its own local mirror, and insert the new character immediately after it (or, if `relation.value` is a line-break, split a new line there — the new character becomes that line's anchor, and everything previously after the insertion point on the old line moves down onto it, the same way handling your own newline in `ide-add` does locally).

A `network-remove` works the same way as `remove` in reverse: find the target identity, delete it. Removing a line's own anchor merges that line back into the one above it, same as the local case.

If the named parent (for an add) or target (for a remove) can't be found at all, drop the event — don't guess, don't retry. This is most often because it was already removed; a plugin is only expected to track *live* content, never tombstones, so a stale reference like this is an expected, harmless occurrence, not an error condition.

## `ide-add-failed`

Reserved in the protocol for a future case where an `ide-add` genuinely can't be applied. Not currently emitted by the server (every add either succeeds or waits harmlessly in a backlog) — a plugin should listen for it, but nothing today will trigger it.

## What a plugin is responsible for

Everything above only works if the plugin maintains **its own local mirror** of the document — one line per row, each with an anchor and an ordered list of cells, each cell holding either a pending tag or a confirmed `(id, peer)` — and keeps it in lockstep with what the editor shows, from its own edits, `initial-state`, and every `ide-add-confirmed`/`ide-add-complete`/`network-add`/`network-remove` it receives. The server never tells a plugin "here's the current state of line 5"; it only ever confirms identities and reports individual adds/removes as they happen. Computing the right `parent_id`/`parent_peer`/`parent_tag` for every send, correctly deferring removes of not-yet-confirmed content, and applying every incoming event strictly one at a time (don't let two overlapping edits race against your own editor's buffer — see `shario-vscode`'s `enqueueNetworkEdit` for the reference approach) is entirely the plugin's job.

## What a plugin does *not* need to worry about

- **Identity assignment of any kind** — a plugin never invents or sends an `id` or `peer` value for its own edits, only a `tag`. Both halves of a character's real identity are decided entirely server-side, in `SharQueue::add_ide_operation`, and handed back via `ide-add-confirmed`.
- **Tie-break order of any kind, for anyone's edits, including its own** — a plugin never computes where a concurrent insert "really" belongs relative to another one. The shar resolves this entirely server-side and hands back the real, already-correct immediate predecessor — in `network-add` for remote-originated content, and in `ide-add-complete` for a plugin's own local content once it's actually resolved. A plugin's only job is to splice at the position it's told; see `shario-vscode`'s `applyNetworkAdd`/`findByIdentity` for the reference implementation of that splice.
- **Tombstone handling beyond what's live** — a plugin is only expected to track live (not-yet-removed) content. A remote op naming something already removed is reported, not guessed at; dropping it is the correct response, not a bug to work around.
- **Ordering/backlogging** — if a plugin's messages happen to get processed out of send order, or a parent hasn't resolved yet, the server holds things until they can apply correctly. A plugin never needs to retry or reorder its own sends.
- **Network sync mechanics** — a plugin only ever talks to its own local server over the `"local"` room. Propagating edits to other machines, gossip, and catch-up hydration between servers are entirely the shar's job; a plugin just receives the result as ordinary `network-add`/`network-remove` events, indistinguishable from any other.
