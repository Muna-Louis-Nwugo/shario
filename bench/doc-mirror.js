'use strict';

// Mirrors extension.js's docState model exactly (lines -> {anchor, cells}, tag-based
// deferred removal for a character deleted before its own add is confirmed), just
// driven by a flat character offset (as editing-traces uses) instead of VS Code's
// native (row, col) change events. This is what makes it a faithful stand-in for a
// real client rather than a shortcut that skips the position -> identity translation
// a real IDE has to do.

function createDocMirror({ emitAdd, emitRemove }) {
    // starts as a single, empty line -- matches a freshly loaded, empty shario file
    // (index 0 of every line is its own anchor: the root sentinel for line 0, the
    // creating newline for every other line; real content starts at index 1)
    const lines = [{ anchor: { id: 0, peer: 0, tag: null }, cells: [] }];

    const pendingAdds = new Map(); // tag -> cell/anchor object
    const pendingRemovals = new Map(); // tag -> { row, col }
    let nextTag = 0;

    // Finds which (row, col0) a flat character offset currently falls on. `col0` is
    // 0-indexed among the line's *real* characters (not shario's anchor-shifted
    // column) -- callers translate as needed, see the comment at each call site.
    function locate(offset) {
        let acc = 0;
        for (let row = 0; row < lines.length; row++) {
            const cells = lines[row].cells;
            if (offset <= acc + cells.length) {
                return { row, col0: offset - acc };
            }
            // +1 for the newline separating this line from the next (the last line
            // has none, but its cells.length is always the loop's terminal case, so
            // this branch is never reached for it)
            acc += cells.length + 1;
        }
        throw new Error(`offset ${offset} out of range (doc length ${acc})`);
    }

    // Sends the remove now if this identity is already known (a confirmed add);
    // otherwise holds it until the add it belongs to resolves, since we don't know
    // what to tell the server yet -- identical in spirit to extension.js's
    // removeIdentity.
    function removeIdentity(identity, row) {
        if (identity.tag !== null) {
            pendingRemovals.set(identity.tag, { row });
            return;
        }
        emitRemove(row, identity.id, identity.peer);
    }

    function insertChar(offset, value) {
        const { row, col0 } = locate(offset);
        // the cell/anchor immediately before the insertion point -- referenced by
        // its real (id, peer) if already known, or by its own tag if it's this
        // same client's own not-yet-confirmed add
        const parent = col0 === 0 ? lines[row].anchor : lines[row].cells[col0 - 1];
        const tag = nextTag++;

        if (value === '\n') {
            const anchor = { id: null, peer: null, tag };
            pendingAdds.set(tag, anchor);
            const moved = lines[row].cells.splice(col0);
            lines.splice(row + 1, 0, { anchor, cells: moved });
        } else {
            const cell = { value, id: null, peer: null, tag };
            pendingAdds.set(tag, cell);
            lines[row].cells.splice(col0, 0, cell);
        }

        emitAdd(parent, row, value, tag);
    }

    function deleteChar(offset) {
        const { row, col0 } = locate(offset);
        if (col0 < lines[row].cells.length) {
            const [cell] = lines[row].cells.splice(col0, 1);
            removeIdentity(cell, row);
        } else {
            // col0 == cells.length: deleting this line's own trailing newline --
            // merges the next line up into this one, same as extension.js's
            // removeAnchor
            const next = lines[row + 1];
            lines[row].cells.push(...next.cells);
            lines.splice(row + 1, 1);
            removeIdentity(next.anchor, row + 1);
        }
    }

    // Called on "ide-add-confirmed": fills in the real (id, peer) for a pending add,
    // and fires any remove that was waiting on this exact tag to resolve.
    function confirmAdd(tag, id, peer) {
        const pending = pendingAdds.get(tag);
        if (!pending) return;
        pendingAdds.delete(tag);
        pending.id = id;
        pending.peer = peer;
        pending.tag = null;

        const waiting = pendingRemovals.get(tag);
        if (waiting) {
            pendingRemovals.delete(tag);
            emitRemove(waiting.row, id, peer);
        }
    }

    // Reconstructs the current flat document text, for comparing against a trace's
    // endContent as a correctness check independent of timing/throughput.
    function currentText() {
        return lines.map((line) => line.cells.map((c) => c.value).join('')).join('\n');
    }

    return { insertChar, deleteChar, confirmAdd, currentText, pendingAdds, pendingRemovals };
}

module.exports = { createDocMirror };
