'use strict';
// Standalone sanity check for doc-mirror.js's position-tracking logic, with fake
// emit/confirm wired to fire synchronously -- isolates "does the offset -> row/col
// math and splicing agree with a flat string" from any real server/socket concerns.
const { createDocMirror } = require('./doc-mirror');
const assert = require('assert');

function run(actions, expected) {
    const pendingConfirms = [];
    const mirror = createDocMirror({
        emitAdd: (parent, lineHint, value, tag) => {
            // simulate the server always assigning id == tag+1, peer 0, and
            // confirming immediately/synchronously
            pendingConfirms.push(() => mirror.confirmAdd(tag, tag + 1, 0));
        },
        emitRemove: (row, id, peer) => {
            // no-op: removes don't need a response for this test
        },
    });

    for (const action of actions) {
        if (action[0] === 'insert') {
            mirror.insertChar(action[1], action[2]);
        } else {
            mirror.deleteChar(action[1]);
        }
        // flush confirmations immediately, same as a real round trip that always
        // resolves before the next character is typed
        while (pendingConfirms.length) pendingConfirms.shift()();
    }

    assert.strictEqual(mirror.currentText(), expected, `expected ${JSON.stringify(expected)}, got ${JSON.stringify(mirror.currentText())}`);
    assert.strictEqual(mirror.pendingAdds.size, 0, 'no adds should still be pending');
    assert.strictEqual(mirror.pendingRemovals.size, 0, 'no removes should still be pending');
}

// basic sequential typing
run(
    [['insert', 0, 'a'], ['insert', 1, 'b'], ['insert', 2, 'c']],
    'abc',
);

// newline split then continue on the new line
run(
    [
        ['insert', 0, 'a'], ['insert', 1, 'b'],
        ['insert', 2, '\n'],
        ['insert', 3, 'c'], ['insert', 4, 'd'],
    ],
    'ab\ncd',
);

// delete an ordinary character
run(
    [
        ['insert', 0, 'a'], ['insert', 1, 'b'], ['insert', 2, 'c'],
        ['delete', 1], // removes 'b'
    ],
    'ac',
);

// delete a newline merges the two lines
run(
    [
        ['insert', 0, 'a'], ['insert', 1, '\n'], ['insert', 2, 'b'],
        ['delete', 1], // the newline
    ],
    'ab',
);

// insert in the middle of an existing line (parent lookup must land mid-line, not
// just at the end)
run(
    [
        ['insert', 0, 'a'], ['insert', 1, 'c'],
        ['insert', 1, 'b'], // "a_c" -> insert at offset 1 -> "abc"
    ],
    'abc',
);

// delete-before-confirm: this only exercises the deferred path if emitAdd/confirm
// aren't flushed between actions, so build it by hand instead of via run()
{
    const sent = [];
    const mirror = createDocMirror({
        emitAdd: (parent, lineHint, value, tag) => sent.push({ type: 'add', parent, lineHint, value, tag }),
        emitRemove: (row, id, peer) => sent.push({ type: 'remove', row, id, peer }),
    });
    mirror.insertChar(0, 'x'); // tag 0, not confirmed yet
    mirror.deleteChar(0); // deletes 'x' before it's confirmed -- must defer, not emit a remove yet
    assert.strictEqual(sent.length, 1, 'the remove must not go out before the add is confirmed');
    assert.strictEqual(mirror.pendingRemovals.size, 1, 'the remove should be sitting in pendingRemovals');

    mirror.confirmAdd(0, 42, 7); // now the add resolves
    assert.strictEqual(sent.length, 2, 'confirming the add should release the deferred remove');
    assert.deepStrictEqual(sent[1], { type: 'remove', row: 0, id: 42, peer: 7 });
    assert.strictEqual(mirror.pendingRemovals.size, 0);
}

console.log('all doc-mirror sanity checks passed');
