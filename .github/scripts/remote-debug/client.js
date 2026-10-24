// A self-contained stand-in for "someone has shario-vscode open": builds
// the real shario binary, spawns it, waits for it to come up, then connects
// as a local IDE using the REAL shario-vscode/extension.js (via
// fake-vscode.js, a minimal stand-in for the VS Code API) -- not a second,
// independent reimplementation of its identity logic that could have its
// own, different bugs. extension.js itself is never modified.
//
// Owning the whole lifecycle itself (build -> spawn -> wait -> connect) in
// one script, rather than splitting it across separate workflow steps,
// means there's no cross-step timing gap: this client connects the instant
// its own server is ready, the same way a real IDE opening would.
//
// Only watches/renders -- it never types anything itself. A message typed
// through a second local-room client on the *same* server would never
// reach this client's own document anyway (same-server multi-client sync
// only broadcasts a content-free confirmation, see
// network_callbacks.rs's ide_add_confirm_callback), so there's nothing a
// self-typed message would prove. What this is for is rendering whatever
// arrives via real cross-server gossip -- another, genuinely separate
// peer's edits -- which is also the reason the actual document state is
// written out to a real file on every change: another, separate workflow
// step reads that file later, after whatever the real test activity was.
//
// Usage: node client.js <sharioRepoDir> <vscodeRepoDir> <mode> <sessionName>
//          <port> <testDir> <fileRelPath> <outputFile> <readyFile>
//          <durationSeconds>
//
// <readyFile> gets written the moment the server this script builds and
// spawns is actually listening -- since the build alone can take 30-60s,
// anything else that wants to talk to that server (another workflow step
// simulating a second peer, say) needs a real signal to wait on rather
// than guessing how long the build will take.
const fs = require("fs");
const net = require("net");
const os = require("os");
const path = require("path");
const Module = require("module");
const { execFileSync, spawn } = require("child_process");

const [
  ,
  ,
  SHARIO_REPO_DIR,
  VSCODE_REPO_DIR,
  MODE,
  SESSION_NAME,
  PORT_STR,
  TEST_DIR,
  FILE_REL,
  OUTPUT_FILE,
  READY_FILE,
  DURATION_SECONDS_STR,
] = process.argv;

const PORT = parseInt(PORT_STR, 10);
const DURATION_MS = parseInt(DURATION_SECONDS_STR, 10) * 1000;
const ABS_TEST_DIR = path.resolve(TEST_DIR);
const ABS_FILE_PATH = path.join(ABS_TEST_DIR, FILE_REL);
const SHARIO_BIN = path.join(path.resolve(SHARIO_REPO_DIR), "target", "debug", "shario");

let serverProcess = null;

function cleanupAndExit(code) {
  if (serverProcess && !serverProcess.killed) {
    serverProcess.kill();
  }
  process.exit(code);
}
process.on("SIGTERM", () => cleanupAndExit(0));
process.on("SIGINT", () => cleanupAndExit(0));

function waitForPort(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = net.connect(port, "127.0.0.1");
      // a bare net.connect has no timeout of its own -- if the SYN gets
      // silently dropped instead of refused, Linux's own retry backoff can
      // sit for minutes with neither "connect" nor "error" ever firing,
      // which would blow straight past timeoutMs since that's only ever
      // checked inside the "error" handler below
      socket.setTimeout(1000, () => socket.destroy(new Error("connect attempt timed out")));
      socket.once("connect", () => {
        socket.end();
        resolve();
      });
      socket.once("error", () => {
        socket.destroy();
        if (Date.now() > deadline) {
          reject(new Error(`port ${port} never came up within ${timeoutMs}ms`));
        } else {
          setTimeout(attempt, 1000);
        }
      });
    };
    attempt();
  });
}

async function main() {
  console.log("[client] building shario...");
  execFileSync("cargo", ["build"], { cwd: path.resolve(SHARIO_REPO_DIR), stdio: "inherit" });

  fs.mkdirSync(ABS_TEST_DIR, { recursive: true });
  if (!fs.existsSync(ABS_FILE_PATH)) {
    fs.writeFileSync(ABS_FILE_PATH, "");
  }

  // launched from a neutral directory, not ABS_TEST_DIR -- the shar root
  // comes entirely from the "join" event's own path, never from this
  // process's cwd, but setup_logging() writes a rotating shar.<date>.log
  // into cwd regardless, and that log file landing inside the shar root
  // itself is exactly what made SharDirectory::new() take several real
  // seconds to load on a prior run (see info/TODO.md's unfiltered-walk
  // entry) -- keep it out of the way entirely instead of hitting that again.
  const serverLaunchDir = fs.mkdtempSync(path.join(os.tmpdir(), "shario-server-launch-"));
  console.log(`[client] starting shario --port ${PORT} ${MODE} ${SESSION_NAME}...`);
  serverProcess = spawn(SHARIO_BIN, ["--port", String(PORT), MODE, SESSION_NAME], {
    cwd: serverLaunchDir,
    env: { ...process.env, RUST_LOG: "shario=debug" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  serverProcess.stdout.pipe(process.stdout);
  serverProcess.stderr.pipe(process.stderr);
  serverProcess.on("exit", (code) => {
    console.log(`[client] shario server exited with code ${code}`);
  });

  await waitForPort(PORT, 30000);
  // waitForPort only proves *something* is listening on that port -- if a
  // stale, unrelated process already held it, the real spawn above would
  // have crashed with AddrInUse right after, and this would otherwise
  // silently "succeed" against that other process instead of catching it.
  if (serverProcess.exitCode !== null || serverProcess.killed) {
    console.error(`[client] the server process exited (code ${serverProcess.exitCode}) before coming up -- port ${PORT} was probably already in use by something else`);
    cleanupAndExit(1);
    return;
  }
  console.log("[client] server is up");
  fs.writeFileSync(READY_FILE, "ready");

  const { createFakeVscode, FakeDocument } = require("./fake-vscode.js");
  const initialText = fs.existsSync(ABS_FILE_PATH) ? fs.readFileSync(ABS_FILE_PATH, "utf8") : "";
  const doc = new FakeDocument(ABS_FILE_PATH, initialText);
  const { vscode, registeredCommands } = createFakeVscode({
    documents: [doc],
    workspaceFolderPath: ABS_TEST_DIR,
  });

  // every change -- not just a final snapshot -- gets written out live, so
  // a later workflow step reading this file sees real progress even if it
  // checks partway through, not just whatever happened to be true at exit
  const originalApplyEdit = vscode.workspace.applyEdit;
  vscode.workspace.applyEdit = async (edit) => {
    const result = await originalApplyEdit(edit);
    fs.writeFileSync(OUTPUT_FILE, doc.getText());
    return result;
  };
  fs.writeFileSync(OUTPUT_FILE, doc.getText());

  // require('vscode') inside extension.js needs to resolve to our fake
  // instance -- override resolution for just that one specifier, same
  // technique used to unit-test extension.js's own internals earlier.
  const FAKE_VSCODE_ID = "\0fake-vscode";
  const originalResolveFilename = Module._resolveFilename;
  Module._resolveFilename = function (request, ...rest) {
    if (request === "vscode") return FAKE_VSCODE_ID;
    return originalResolveFilename.call(this, request, ...rest);
  };
  require.cache[FAKE_VSCODE_ID] = {
    id: FAKE_VSCODE_ID,
    filename: FAKE_VSCODE_ID,
    loaded: true,
    exports: vscode,
  };

  // path.resolve first -- a relative VSCODE_REPO_DIR without a leading
  // "./" would otherwise make require() treat this as a bare module
  // specifier instead of a file path
  const extensionPath = path.join(path.resolve(VSCODE_REPO_DIR), "extension.js");
  const ext = require(extensionPath);

  ext.activate({
    subscriptions: [],
    globalStorageUri: { fsPath: path.join(ABS_TEST_DIR, ".fake-global-storage") },
  });

  if (!registeredCommands["shar.connect"]) {
    console.error("[client] shar.connect was never registered -- activate() must have failed");
    cleanupAndExit(1);
    return;
  }

  console.log("[client] invoking the real shar.connect...");
  await registeredCommands["shar.connect"]();

  console.log(`[client] connected -- listening for ${DURATION_SECONDS_STR}s, writing ${OUTPUT_FILE} on every change...`);
  await new Promise((r) => setTimeout(r, DURATION_MS));

  console.log("[client] done listening, final text:");
  console.log(doc.getText());
  cleanupAndExit(0);
}

main().catch((err) => {
  console.error("[client] fatal:", err);
  cleanupAndExit(1);
});
