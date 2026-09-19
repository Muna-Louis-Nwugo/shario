# Shario (Work in Progress)

Shario is a real-time collaborative text editor backend built on a CRDT (conflict-free replicated data type), meant for students, professors, co-workers, and others sharing a codebase live over a network. It replaces the "git commit; git push; git pull" workflow (and proprietary IDE-specific live-share tools) with a shared, IDE-agnostic core that any editor can attach to over a websocket.

**Status: work in progress, not ready for real use yet.** Local collaborative editing (one shar server on one machine, one or more IDE clients attached to it) works and is tested/benchmarked against real editing-trace datasets — see `info/BENCHMARK.md`. Syncing between multiple machines over an actual network is not implemented yet; that's the current focus. See `info/TODO.md` and `info/KNOWN_ISSUES.md` for what's done, in progress, and known-broken.

Shario is licensed under the GNU General Public License v3 — free to use, forever.

The name "Shario" is a combination of "Share" and "I/O."

## Architecture, in short

A `shar` server holds the CRDT state for a directory and mediates every edit through it. IDE clients (currently a VS Code extension) connect to the server over a websocket, send/receive single-character insert and delete events, and the server keeps every connected client converged. A second websocket connection (work in progress) is meant to let separate `shar` servers — each fronting their own local IDE — sync with each other the same way, so editing works across machines, not just within one.

Read the full breakdown — every component, the wire protocol, and what's still unbuilt — in [`info/ARCHITECTURE.md`](info/ARCHITECTURE.md).

## Getting started

Requires a local Rust toolchain (`cargo`) and Node.js (for the VS Code extension and the benchmark harness).

```bash
# run the server against a directory
cd shario
cargo run                       # serves on 127.0.0.1:3000

# in shario-vscode: launch the extension in a VS Code Extension Development Host
cd shario-vscode
code .
# press F5, then run "Shar: Connect" from the command palette
# (make sure the server is running and the connected workspace folder
# matches the directory the server loaded)
```

Then just type or delete in a file inside the shar's directory — edits sync live to every other IDE connected to the same server.

## Project layout

- `shario/` — the Rust server (this repo). `src/shar/core/tree.rs` is the CRDT itself; `src/shar/core/queue.rs` mediates between the IDE/network and the tree; `src/main.rs` is the axum/socketioxide server and wire protocol.
- [`shario-vscode`](https://github.com/Muna-Louis-Nwugo/shario-vscode) — the VS Code extension client (`extension.js`), a sibling repo/directory, not part of this one.
- `bench/` — a benchmark harness that replays real [josephg/editing-traces](https://github.com/josephg/editing-traces) datasets through the actual server binary, over a real socket, to catch correctness/perf issues synthetic unit tests can't. See `info/BENCHMARK.md` for results and the harness's own comments for how to run it yourself.
- `info/` — living documentation: `ARCHITECTURE.md` (how it's built), `TODO.md` (what's planned/done, with reasoning), `KNOWN_ISSUES.md` (observed problems without a full fix yet), `BENCHMARK.md` (measured perf/correctness results).

## Terminal usage

Not implemented yet — `cargo run` starts the server directly for now. A `shar` CLI (init a directory, join a session, etc.) is planned but not built.
