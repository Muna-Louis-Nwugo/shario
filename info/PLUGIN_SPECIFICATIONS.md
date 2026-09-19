# Shar Plugin Specification

The contract between a `shar` server and an IDE plugin. If you're building a new plugin (for an editor other than VS Code) to join the shario ecosystem, this is the spec to implement against — the server doesn't care what editor is on the other end, only that it speaks this protocol correctly. `shario-vscode`'s `extension.js` is the reference implementation; when in doubt, that's the working example.

This document covers the **IDE side of the protocol only** (`"local"` room). It does not cover peer-to-peer network sync (`"network"` room) — that's a separate, not-yet-built contract for server-to-server communication, not something a plugin needs to implement.

## Transport

A plain [socket.io](https://socket.io/) connection to the server's `/` namespace (default `http://127.0.0.1:3000`). One socket connection per plugin instance.

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
    // each entry is a raw [id, peer] pair.
    [[0, 0]],
    [[5, 1], [6, 1], [7, 1]]
  ]
}
```

**A plugin must use this to bootstrap identities for pre-existing content** — anything already on disk before the plugin connected has no identity of its own until matched against this snapshot, by position. This match is only safe because nothing has been edited between the plugin reading the file off disk and the server loading the same bytes — do this before allowing any edits to a file.

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

## Sending a delete: `remove`

```jsonc
{ "file_path": "<absolute path>", "id": 118, "peer": 1, "row": 3 }
```

- **`id` / `peer`** — the real identity of the target. **If the target hasn't been confirmed yet** (you deleted something the instant after typing it, before `ide-add-confirmed` came back), **do not send this event yet** — hold it locally, keyed by the pending tag, and send it once the matching confirmation arrives with the real identity.
- **`row`** — same deal as `line_hint`: a ring-search starting point only, not checked, never a source of a wrong result.

To remove a line's own anchor (merging it into the line above), send the same event targeting that anchor's identity — there's no separate "remove whole line" event.

## `ide-add-failed`

Reserved in the protocol for a future case where an `ide-add` genuinely can't be applied. Not currently emitted by the server (every add either succeeds or waits harmlessly in a backlog) — a plugin should listen for it, but nothing today will trigger it.

## What a plugin is responsible for

Everything above only works if the plugin maintains **its own local mirror** of the document — one line per row, each with an anchor and an ordered list of cells, each cell holding either a pending tag or a confirmed `(id, peer)` — and keeps it in lockstep with what the editor shows, purely from the plugin's own edits and `ide-add-confirmed`/`initial-state`. The server never tells a plugin "here's the current state of line 5"; it only ever confirms identities and (once network sync exists) broadcasts remote edits. Computing the right `parent_id`/`parent_peer`/`parent_tag` for every send, and correctly deferring removes of not-yet-confirmed content, is entirely the plugin's job.

## What a plugin does *not* need to worry about

- **Identity assignment of any kind** — a plugin never invents or sends an `id` or `peer` value for its own edits, only a `tag`. Both halves of a character's real identity are decided entirely server-side, in `SharQueue::add_ide_operation`, and handed back via `ide-add-confirmed`.
- **Convergence rules** — sibling tie-break order, tombstone handling, how concurrent edits from other sources ultimately resolve into one agreed document: all of that lives in the CRDT tree (`SharFile`/`SharDirectory`), entirely server-side. A plugin never decides where something "really" belongs relative to another replica's edits; it just reports what it did and mirrors what the server confirms.
- **Ordering/backlogging** — if a plugin's messages happen to get processed out of send order, or a parent hasn't resolved yet, the server holds things until they can apply correctly. A plugin never needs to retry or reorder its own sends.
- **Network sync** — a plugin only ever talks to its own local server. Propagating edits to other machines is the server's job (once built), not the plugin's.
