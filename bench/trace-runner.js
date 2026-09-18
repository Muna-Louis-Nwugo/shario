'use strict';

// Plays a real josephg/editing-traces file through the actual shario server binary
// over a real socket.io connection -- not an in-process shortcut. Measures wall-clock
// throughput, add-confirmation latency, and server RSS over the run, and verifies the
// final document text matches the trace's own recorded endContent.
//
// Usage: node trace-runner.js <path-to-trace.json[.gz]> [--label name]

const fs = require('fs');
const os = require('os');
const path = require('path');
const net = require('net');
const zlib = require('zlib');
const { spawn } = require('child_process');
const { io } = require('socket.io-client');
const { createDocMirror } = require('./doc-mirror');

const PORT = 3000;
const SERVER_BIN = path.resolve(__dirname, '..', 'target', 'release', 'shario');

function loadTrace(filePath) {
    let raw = fs.readFileSync(filePath);
    if (filePath.endsWith('.gz')) raw = zlib.gunzipSync(raw);
    return JSON.parse(raw.toString('utf8'));
}

function isServerUp() {
    return new Promise((resolve) => {
        const probe = net.connect({ port: PORT, host: '127.0.0.1' }, () => {
            probe.end();
            resolve(true);
        });
        probe.on('error', () => resolve(false));
    });
}

async function waitForServerUp(timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
        if (await isServerUp()) return true;
        await new Promise((r) => setTimeout(r, 50));
    }
    return false;
}

function readRssKb(pid) {
    try {
        const status = fs.readFileSync(`/proc/${pid}/status`, 'utf8');
        const match = status.match(/VmRSS:\s+(\d+)\s+kB/);
        return match ? parseInt(match[1], 10) : null;
    } catch {
        return null; // process may have exited, or /proc unavailable (non-Linux)
    }
}

function percentile(sortedArr, p) {
    if (sortedArr.length === 0) return null;
    const idx = Math.min(sortedArr.length - 1, Math.floor(p * sortedArr.length));
    return sortedArr[idx];
}

async function main() {
    const args = process.argv.slice(2);
    const tracePath = args[0];
    if (!tracePath) {
        console.error('usage: node trace-runner.js <path-to-trace.json[.gz]> [--label name]');
        process.exit(1);
    }
    const labelIdx = args.indexOf('--label');
    const label = labelIdx !== -1 ? args[labelIdx + 1] : path.basename(tracePath);

    const trace = loadTrace(tracePath);
    const totalPatches = trace.txns.reduce((n, t) => n + t.patches.length, 0);
    let totalCharInserts = 0;
    let totalCharDeletes = 0;
    for (const txn of trace.txns) {
        for (const [, delCount, insertedContent] of txn.patches) {
            totalCharInserts += insertedContent.length;
            totalCharDeletes += delCount;
        }
    }
    const totalCharOps = totalCharInserts + totalCharDeletes;

    console.log(`[${label}] loaded: ${totalPatches} patches, ${totalCharInserts} char inserts, ${totalCharDeletes} char deletes (${totalCharOps} total char ops), endContent length ${trace.endContent.length}`);

    // scratch shar directory: one empty file, matching a freshly created document
    const scratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'shario-bench-'));
    const filePath = path.join(scratchDir, 'doc.txt');
    fs.writeFileSync(filePath, '');

    console.log(`[${label}] starting server (release build) against ${scratchDir}...`);
    const debug = args.includes('--debug');
    const server = spawn(SERVER_BIN, [], {
        cwd: scratchDir,
        stdio: ['ignore', 'ignore', 'inherit'],
        env: debug ? { ...process.env, RUST_LOG: 'shario=debug' } : process.env,
    });
    let serverExited = false;
    server.on('exit', (code) => {
        serverExited = true;
        if (code !== 0 && code !== null) console.error(`[${label}] server exited early with code ${code}`);
    });

    const rssSamplesKb = [];
    const rssInterval = setInterval(() => {
        const rss = readRssKb(server.pid);
        if (rss !== null) rssSamplesKb.push(rss);
    }, 50);

    const up = await waitForServerUp(10000);
    if (!up) {
        clearInterval(rssInterval);
        server.kill();
        throw new Error('server did not come up within 10s');
    }
    const idleRssKb = readRssKb(server.pid);

    const socket = io(`http://127.0.0.1:${PORT}`);
    await new Promise((resolve, reject) => {
        socket.on('connect', resolve);
        socket.on('connect_error', reject);
        setTimeout(() => reject(new Error('socket connect timeout')), 10000);
    });
    socket.emit('join', { local: true, path: scratchDir });
    // give the join a moment to actually initialize the queue server-side before
    // the first "ide-add" arrives
    await new Promise((r) => setTimeout(r, 200));

    let emittedAdds = 0;
    let confirmedAdds = 0;
    const sendTimes = new Map(); // tag -> hrtime bigint
    const latenciesMs = [];

    const debugPayloads = new Map(); // tag -> payload, temporary diagnostic only
    const mirror = createDocMirror({
        emitAdd: (parent, lineHint, value, tag) => {
            emittedAdds++;
            sendTimes.set(tag, process.hrtime.bigint());
            const payload = {
                file_path: filePath,
                parent_id: parent.tag === null ? parent.id : null,
                parent_peer: parent.tag === null ? parent.peer : null,
                parent_tag: parent.tag,
                val: value,
                tag,
                line_hint: lineHint,
            };
            debugPayloads.set(tag, payload);
            socket.emit('ide-add', payload);
        },
        emitRemove: (row, id, peer) => {
            socket.emit('remove', { file_path: filePath, id, peer, row });
        },
    });

    socket.on('ide-add-confirmed', (data) => {
        const sentAt = sendTimes.get(data.tag);
        if (sentAt !== undefined) {
            sendTimes.delete(data.tag);
            latenciesMs.push(Number(process.hrtime.bigint() - sentAt) / 1e6);
        }
        mirror.confirmAdd(data.tag, data.id, data.peer);
        confirmedAdds++;
    });

    console.log(`[${label}] playing trace...`);
    const startNs = process.hrtime.bigint();

    let opsSinceYield = 0;
    for (const txn of trace.txns) {
        for (const [pos, delCount, insertedContent] of txn.patches) {
            for (let i = 0; i < delCount; i++) {
                mirror.deleteChar(pos);
                opsSinceYield++;
            }
            for (let i = 0; i < insertedContent.length; i++) {
                mirror.insertChar(pos + i, insertedContent[i]);
                opsSinceYield++;
            }
            if (opsSinceYield >= 500) {
                opsSinceYield = 0;
                await new Promise((r) => setImmediate(r));
                if (serverExited) throw new Error('server exited mid-run');
            }
        }
    }
    const allSentNs = process.hrtime.bigint();
    console.log(`[${label}] all ${emittedAdds} adds emitted in ${Number(allSentNs - startNs) / 1e6}ms, waiting for confirmations to drain...`);

    const drainDeadline = Date.now() + 120000;
    while (confirmedAdds < emittedAdds) {
        if (Date.now() > drainDeadline) {
            console.error(`[${label}] STUCK, ${sendTimes.size} tags never confirmed. First 20:`);
            let shown = 0;
            for (const tag of sendTimes.keys()) {
                if (shown++ >= 20) break;
                console.error(JSON.stringify(debugPayloads.get(tag)));
            }
            throw new Error(`timed out waiting for confirmations (${confirmedAdds}/${emittedAdds})`);
        }
        if (serverExited) throw new Error('server exited mid-run');
        await new Promise((r) => setTimeout(r, 10));
    }
    const endNs = process.hrtime.bigint();

    // let RSS settle briefly before taking the final reading
    await new Promise((r) => setTimeout(r, 300));
    const finalRssKb = readRssKb(server.pid);
    clearInterval(rssInterval);

    const totalMs = Number(endNs - startNs) / 1e6;
    const opsPerSec = totalCharOps / (totalMs / 1000);
    latenciesMs.sort((a, b) => a - b);
    const peakRssKb = rssSamplesKb.length ? Math.max(...rssSamplesKb) : null;

    const actualText = mirror.currentText();
    const matches = actualText === trace.endContent;

    socket.disconnect();
    server.kill();
    fs.rmSync(scratchDir, { recursive: true, force: true });

    const result = {
        label,
        totalCharOps,
        totalCharInserts,
        totalCharDeletes,
        totalMs,
        opsPerSec,
        idleRssKb,
        peakRssKb,
        finalRssKb,
        latencyMs: {
            mean: latenciesMs.reduce((a, b) => a + b, 0) / (latenciesMs.length || 1),
            p50: percentile(latenciesMs, 0.5),
            p95: percentile(latenciesMs, 0.95),
            p99: percentile(latenciesMs, 0.99),
            max: latenciesMs[latenciesMs.length - 1] ?? null,
        },
        correctnessCheck: matches ? 'PASS' : 'FAIL',
    };

    console.log(`\n[${label}] RESULT`);
    console.log(JSON.stringify(result, null, 2));
    if (!matches) {
        console.error(`[${label}] MISMATCH: expected ${trace.endContent.length} chars, got ${actualText.length} chars`);
    }

    return result;
}

main().catch((err) => {
    console.error(err);
    process.exit(1);
});
