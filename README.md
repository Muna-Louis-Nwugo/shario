# Shario

Shario is a real-time collaborative text editor backend built on a CRDT (conflict-free replicated data type), meant for students, professors, co-workers, and others sharing a codebase live over a network. It replaces the "git commit; git push; git pull" workflow (and proprietary IDE-specific live-share tools) with a shared, IDE-agnostic core that any editor can attach to over a websocket.


Shario is licensed under the GNU General Public License v3 — free to use, forever.

The name "Shario" is a combination of "Share" and "I/O." It's pronounciation: "shar-ee-o"

## Architecture, in short

A `shar` server holds the CRDT state for a directory and mediates every edit through it. IDE clients (currently a VS Code extension) connect to the server over a websocket, send/receive single-character insert and delete events, and the server keeps every connected client converged. Separate `shar` servers, each fronting their own local IDE, sync with each other the same way over the network, so editing works across machines, not just within one.

Read the full breakdown — every component, the wire protocol, and what's still unbuilt — in [`info/ARCHITECTURE.md`](info/ARCHITECTURE.md).

## Getting started

Requires a local Rust toolchain (`cargo`) and Node.js (for the VS Code extension and the benchmark harness). Session discovery goes through a small proxy (see the sibling `shario-discovery-proxy` repo, not part of this one) rather than talking to a database directly: set `DISCOVERY_PROXY_URL` to that proxy's deployed URL in a `.env` file at the repo root (gitignored, not committed).

```bash
cd shario

# start a brand-new session (prints the session name if you don't give one)
cargo run -- create [name]

# or join an existing session by the name its creator used
cargo run -- participate <name>

# --port <N> picks the IDE-facing socket.io port (default 3000) — only
# needed to run more than one shario instance on the same machine
```

```bash
# in shario-vscode: launch the extension in a VS Code Extension Development Host
cd shario-vscode
code .
# press F5, then run "Shar: Create" or "Shar: Participate" from the command palette
```

Then just type or delete in a file inside the shar's directory — edits sync live to every other IDE connected to the same session, on this machine or another.

## Project layout

- `shario/` — the Rust server (this repo). `src/shar/core/tree.rs` is the CRDT itself; `src/shar/core/queue.rs` mediates between the IDE/network and the tree; `src/main.rs` is the axum/socketioxide server and wire protocol.
- [`shario-vscode`](https://github.com/Muna-Louis-Nwugo/shario-vscode) — the VS Code extension client (`extension.js`), a sibling repo/directory, not part of this one.
- `bench/` — a benchmark harness that replays real [josephg/editing-traces](https://github.com/josephg/editing-traces) datasets through the actual server binary, over a real socket, to catch correctness/perf issues synthetic unit tests can't. See [`info/BENCHMARK.md`](info/BENCHMARK.md) for results and the harness's own comments for how to run it yourself.
- `info/` — living documentation: [`ARCHITECTURE.md`](info/ARCHITECTURE.md) (how it's built), [`PLUGIN_SPECIFICATIONS.md`](info/PLUGIN_SPECIFICATIONS.md) (the wire contract between server and IDE plugin), [`TODO.md`](info/TODO.md) (what's planned/done, with reasoning), [`KNOWN_ISSUES.md`](info/KNOWN_ISSUES.md) (observed problems without a full fix yet), [`BENCHMARK.md`](info/BENCHMARK.md) (measured perf/correctness results).

## Building a plugin for another editor

Shario is meant to be IDE-agnostic. **If you want to build a plugin for a different editor, [`info/PLUGIN_SPECIFICATIONS.md`](info/PLUGIN_SPECIFICATIONS.md) is the specification to implement against. It covers every event, field, and invariant the server expects, independent of any particular editor's API.

## Terminal usage

`shario create [name]` / `shario participate <name>` (see Getting started above) is the whole CLI today. ; packaging a standalone binary is a planned next step, see [`info/TODO.md`](info/TODO.md).
