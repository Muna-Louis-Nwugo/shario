# Known Issues

Bugs that are known/observed but not yet root-caused or fixed. Unlike
`TODO.md`, these don't have a confirmed mechanism or a planned fix yet --
just a symptom.

None currently. The large-burst crash (client-side position tracking
diverging under load) is fixed -- see `TODO.md`'s "1.1" entry and
`BENCHMARK.md` for full-trace verification. `shario-vscode`'s
`maybeRestartServer` watchdog (see `extension.js`) still auto-restarts the
server if it ever dies for an unrelated reason.
