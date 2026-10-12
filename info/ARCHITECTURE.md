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

## Overview

A shar is one peer's copy of a shared project directory. Every peer runs the same local server process, which does two jobs at once: it talks to your own IDE over one connection, and it talks to every other peer's server over the network. Typing a character yourself and seeing someone else's edit appear are the same operation from the server's point of view: an edit comes in one side and goes out the other.

There's no central server deciding what the document "really" looks like. Every peer has to independently land on the exact same result from the same set of edits, however many of them there are and whatever order they happen to arrive in. Everything below exists to make that actually true.

## Tree (`src/shar/core/tree.rs`)

This is where a file's actual content lives. Instead of a flat string, every character is its own permanent node: a unique `(id, peer)` identity, its value, and a pointer to whichever character it was typed after (its "parent"). A character is identified by where it came from, not by where it currently sits on screen. That's what lets two peers agree on "the same" character even though they each found out about it at a different time, from a different direction.

Deleting a character never removes its node. It's marked tombstoned and kept forever, since another peer's edit, already sent before the delete happened, might still reference it as a parent; removing it outright would leave that edit with nowhere to attach.

Two views of this exist side by side: the full set of nodes (identity-keyed, tombstones included) is the real, authoritative data every peer has to agree on; a second, derived view lays those same nodes out as plain lines and columns, which is what makes cursor positions and "where does a new character land" fast to work with.

Two characters typed after the same parent by two peers who couldn't coordinate still need to land in the same order everywhere. That's settled by a fixed rule instead of by timing: whichever has the larger id comes first, and on the rare occasion two ids tie, whichever peer value is smaller does. Every peer applies the same rule to the same information, so there's never anything to actually negotiate.

Content that already exists on disk by the time a session is started is given the peer "0", to identify it as pre-existing information rather than something anyone actually typed. This helps, because when two shars join from the same starting file, both sides invent the exact same identities for it independently, instead of each side making up its own and disagreeing. This isn't always the case, though; see [`KNOWN_ISSUES.md`](KNOWN_ISSUES.md).

## SharQueue (`src/shar/core/queue.rs`)

Nothing touches the tree directly. Every edit, typed locally or arriving from another peer, passes through this queue first. It's what actually assigns real identities and decides whether an edit can be applied yet.

An edit can arrive before the thing it depends on has: an insert whose parent hasn't been seen yet, or a delete targeting something not seen yet. Rather than fail, it just waits, held until the specific identity it's missing actually shows up. Each wait is tied to exactly what it's blocked on, so the moment that dependency arrives, anything waiting on it gets retried automatically, without needing to re-check everything else sitting around.

A local edit is confirmed back to the IDE the instant it's assigned a real identity, before it's even been inserted into the tree. This is what makes deleting something an instant after typing it always safe, with nothing to wait on. An edit bound for (or arriving from) another peer carries its own value and parent explicitly, rather than a position, since two peers have no shared notion of where anything "is."

## Server (`src/main.rs`)

The server is the process that owns one `SharQueue` for a directory and exposes it to your own IDE over one connection: accepting its edits, and handing it edits that arrived from elsewhere, confirmed or applied the way described above. Sending an edit on to the network is the other half of that, covered after Networking, below.

## IDE (plugin)

A plugin never computes identities itself. It only ever invents a disposable local `tag` the instant something is typed, as a placeholder until the server's response swaps it for a real identity. This is what lets someone type a fast burst of characters without waiting for a round trip after every single one: the next character just references the previous one by its tag, and the server sorts out the real identities once everything catches up. An insert references its parent the same way the server does internally: by real identity if it's already confirmed, or by tag if it's this same connection's own not-yet-confirmed character. Deletes work the same way, held locally until their target has a real identity.

Content already on disk before the plugin connects gets matched up with the server's identities once, right after connecting, by position, safe only because nothing's been edited in between.

The full contract (and what a plugin for a different editor would need to implement) is [`PLUGIN_SPECIFICATIONS.md`](PLUGIN_SPECIFICATIONS.md).

## Networking (iroh)

A peer-to-peer connection runs over [iroh](https://iroh.computer), which connects by a peer's public key rather than a raw network address. This matters because two peers are often on networks that won't actually let them reach each other directly: some WiFi networks deliberately block devices on the same network from talking to one another. iroh tries a direct connection first and falls back to relaying through a third party automatically if that fails, so nothing else in this system has to know or care which one actually happened.

Two different things travel this way:
- **Ongoing edits**: exactly the same edits the queue already produces for local typing, broadcast out to every peer in the session and applied the same way on arrival. This can and does deliver the same edit more than once, or out of order. That's harmless, since SharQueue already tolerates both: a repeat of an edit that's already applied is simply ignored, and anything arriving before its dependency just waits, same as above.
- **Catching a new peer up**: broadcasting only reaches peers already present, so it can't hand a newly-joined peer anything that happened before they arrived. Joining a session also means asking a couple of already-connected peers directly for their entire current state, and replaying it in locally the same way any other edit is applied. A peer that joins later than you, and might have history you don't, gets asked the same way the moment it's noticed.

## Client (iroh)

Sending an edit out is only half the picture. Something also has to listen for what comes back. A separate, always-running task does that: it stays subscribed to the session for as long as the shar is running, and for every edit it receives, hands it to the local `SharQueue` the same way one of your own typed characters would be. The one-time catch-up described above (asking a couple of peers for their full state when you join, or when gossip notices someone new) runs through this same task: what comes back just gets replayed through the queue one edit at a time, not handled any differently from an edit arriving the normal way.

## Discovery (Redis)

Before any of the above can happen, a peer needs to learn who else is in its session at all. iroh can tell you how to reach a specific peer once you already know its key, but it has no notion of "who's in this session." A small shared database fills that gap: one entry per session, holding its current peers' keys.

Creating a session checks the name isn't already taken and starts that entry empty, since there's no one else to list yet. Joining one looks the entry up and uses it to actually connect, and only adds your own key to it once that's actually succeeded, so a session is never listed as reachable before it really is. Your own key never ends up among the ones you try to connect to, either: asking yourself for a connection isn't meaningful, and iroh refuses it outright if it ever happened. Entries expire automatically after a day so an abandoned session doesn't live forever.

Known limitation: two peers updating the same session's entry at nearly the same moment can race and silently lose one of their updates. Acceptable at today's scale, not fixed.

## Status

**Working end to end:**
- Local edits sync through the whole chain to another peer and back.
- A newly-joined peer gets caught up on history it missed.
- Two peers whose shar directories have completely unrelated names still sync correctly, since file paths now travel relative to each shar's own root rather than as a local absolute path.
- Creating a session with a name that's already taken is rejected instead of silently overwriting it.

**Still open:**
- The database race described above.
- A shar's state doesn't persist across a restart, and editing its files with something other than the shar itself can silently corrupt or lose history. See [`KNOWN_ISSUES.md`](KNOWN_ISSUES.md).
- `find_crdt`'s search cost under a fast burst of edits: a performance issue, not a correctness one. See [`TODO.md`](TODO.md)/[`KNOWN_ISSUES.md`](KNOWN_ISSUES.md).
