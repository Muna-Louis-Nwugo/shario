# Known Issues

Bugs that are known/observed but not yet root-caused or fixed. Unlike
`TODO.md`, these don't have a confirmed mechanism or a planned fix yet --
just a symptom.

## Burst-of-bursts backlog storm

**Symptom:** add over ~500 lines, delete them all, and immediately (before
the delete has finished processing) send over 100 new lines as additions --
everything starts getting backlogged.

**Cause:** unknown. Not the recursive backlog-clearing/stack-overflow issue
-- logs confirm no dependency chain is ever started when this happens.

**Priority:** low -- this requires stacking three extreme operations back
to back (large add, large delete, large add again, with no gap) to trigger.
Nobody should realistically be doing that; not a blocker for normal use.
