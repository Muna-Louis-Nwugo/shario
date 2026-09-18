'use strict';

// Exercises the REAL extension.js code (not a reimplementation) outside VS Code:
// mocks just enough of the `vscode` module for `activate()` to run, connects to a
// real, manually-started shar server, kills that server to simulate a crash, and
// verifies `maybeRestartServer` (wired to the socket's own "disconnect" event)
// actually brings a new one up and reconnects.

const fs = require('fs');
const os = require('os');
const path = require('path');
const net = require('net');
const Module = require('module');
const { spawn } = require('child_process');

const EXTENSION_DIR = path.resolve(__dirname, '..', '..', 'shario-vscode');
const SERVER_BIN = path.resolve(__dirname, '..', 'target', 'release', 'shario');
const PORT = 3000;

// --- minimal `vscode` mock -------------------------------------------------
const events = []; // chronological log of everything the extension told the "user"
const registeredCommands = new Map();
let changeListenerFn = null;

const outputChannelLines = [];
const mockVscode = {
    window: {
        createOutputChannel: () => ({
            appendLine: (line) => outputChannelLines.push(line),
            append: (chunk) => outputChannelLines.push(chunk),
        }),
        showInformationMessage: (msg) => events.push({ type: 'info', msg }),
        showWarningMessage: (msg) => events.push({ type: 'warning', msg }),
        showErrorMessage: (msg) => events.push({ type: 'error', msg }),
    },
    workspace: {
        workspaceFolders: null, // set below, once the scratch dir exists
        saveAll: async () => {},
        textDocuments: [],
        onDidChangeTextDocument: (fn) => {
            changeListenerFn = fn;
            return { dispose() {} };
        },
    },
    commands: {
        registerCommand: (name, fn) => {
            registeredCommands.set(name, fn);
            return { dispose() {} };
        },
    },
};

// intercept `require('vscode')` so the real extension.js loads unmodified
const originalResolve = Module._resolveFilename;
Module._resolveFilename = function (request, ...rest) {
    if (request === 'vscode') return 'vscode';
    return originalResolve.call(this, request, ...rest);
};
require.cache['vscode'] = { id: 'vscode', filename: 'vscode', loaded: true, exports: mockVscode };

function isServerUp() {
    return new Promise((resolve) => {
        const probe = net.connect({ port: PORT, host: '127.0.0.1' }, () => {
            probe.end();
            resolve(true);
        });
        probe.on('error', () => resolve(false));
    });
}

async function waitFor(conditionFn, timeoutMs, label) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (await conditionFn()) return true;
        await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error(`timed out waiting for: ${label}`);
}

async function main() {
    const scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'shario-watchdog-'));
    fs.writeFileSync(path.join(scratchDir, 'doc.txt'), '');
    mockVscode.workspace.workspaceFolders = [{ uri: { fsPath: scratchDir } }];

    console.log(`scratch dir: ${scratchDir}`);
    console.log('starting the FIRST server manually (simulating one already running)...');
    let currentServer = spawn(SERVER_BIN, [], { cwd: scratchDir, stdio: 'ignore' });
    const firstPid = currentServer.pid;
    await waitFor(isServerUp, 10000, 'first server to come up');
    console.log(`first server up, pid ${firstPid}`);

    // load the real extension.js only now that the vscode mock is in place
    const extension = require(path.join(EXTENSION_DIR, 'extension.js'));
    extension.activate({ subscriptions: [] });

    console.log('invoking the real "shar.connect" command handler...');
    await registeredCommands.get('shar.connect')();

    await waitFor(
        () => Promise.resolve(events.some((e) => e.msg === 'Connected to Shar')),
        10000,
        'the extension to report "Connected to Shar"',
    );
    console.log('extension confirmed connected.');

    console.log(`killing the server (pid ${firstPid}) to simulate a crash...`);
    process.kill(firstPid, 'SIGKILL');

    // give the extension's own socket.io client a moment to notice
    await waitFor(
        () => Promise.resolve(events.some((e) => e.type === 'warning' && e.msg === 'Disconnected from Shar')),
        10000,
        'the extension to notice the disconnect',
    );
    console.log('extension noticed the disconnect -- watchdog should be spawning a replacement now...');

    // the watchdog spawns its own child process; wait for the port to come back
    await waitFor(isServerUp, 30000, 'a replacement server to come up');
    console.log('a server is listening again (spawned by the watchdog, not by us).');

    // socket.io's own auto-reconnect should now succeed and re-fire "connect"
    await waitFor(
        () => Promise.resolve(events.filter((e) => e.msg === 'Connected to Shar').length >= 2),
        20000,
        'the extension to reconnect and report "Connected to Shar" a second time',
    );
    console.log('extension reconnected automatically after the watchdog restart.');

    console.log('\nPASS: watchdog detected the crash, restarted the server, and the client reconnected on its own.');

    extension.deactivate();
    await new Promise((r) => setTimeout(r, 500));
    fs.rmSync(scratchDir, { recursive: true, force: true });
    process.exit(0);
}

main().catch((err) => {
    console.error('FAIL:', err.message);
    console.error('\nevents so far:', JSON.stringify(events, null, 2));
    console.error('\noutput channel:', outputChannelLines.join('\n'));
    process.exit(1);
});
