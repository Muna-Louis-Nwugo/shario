// Runs the REAL shario-vscode/extension.js headlessly against a real,
// already-running shar server, using fake-vscode.js as a minimal but
// functional stand-in for the VS Code API. The point: verify what the
// actual shipped client resolves a session down to -- actual reconstructed
// text, read back from a real (if fake) document -- not a second,
// independent reimplementation of its identity/tie-break logic that could
// have its own, different bugs.
//
// Only watches/renders -- it never types anything itself. "Typing" and
// "receiving" are independent concerns here: a message typed through a
// second local-room client on the *same* server would never reach this
// client's own document anyway (confirmed directly -- same-server
// multi-client sync only broadcasts a content-free confirmation, see
// network_callbacks.rs's ide_add_confirm_callback), so there's nothing a
// self-typed message would prove. What this is actually for is rendering
// whatever arrives via real cross-server gossip -- i.e. the other,
// genuinely separate peer's edits.
//
// Usage: node driver.js <vscodeRepoDir> <testDir> <fileRelPath> <durationSeconds>
const fs = require("fs");
const path = require("path");
const Module = require("module");

const [, , VSCODE_REPO_DIR, TEST_DIR, FILE_REL, DURATION_SECONDS_STR] = process.argv;
const DURATION_MS = parseInt(DURATION_SECONDS_STR, 10) * 1000;
const ABS_FILE_PATH = path.join(path.resolve(TEST_DIR), FILE_REL);

const { createFakeVscode, FakeDocument } = require("./fake-vscode.js");

const initialText = fs.existsSync(ABS_FILE_PATH) ? fs.readFileSync(ABS_FILE_PATH, "utf8") : "";
const doc = new FakeDocument(ABS_FILE_PATH, initialText);
const { vscode, registeredCommands } = createFakeVscode({
  documents: [doc],
  workspaceFolderPath: path.resolve(TEST_DIR),
});

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

const extensionPath = path.join(VSCODE_REPO_DIR, "extension.js");
const ext = require(extensionPath);

function printDocument(label) {
  console.log(`\n--- document (${label}) ---`);
  console.log(doc.getText());
  console.log("--- end ---\n");
}

async function main() {
  ext.activate({
    subscriptions: [],
    globalStorageUri: { fsPath: path.join(TEST_DIR, ".fake-global-storage") },
  });

  if (!registeredCommands["shar.connect"]) {
    console.error("shar.connect was never registered -- activate() must have failed");
    process.exit(1);
  }

  console.log("[driver] invoking the real shar.connect...");
  await registeredCommands["shar.connect"]();

  await new Promise((r) => setTimeout(r, 2000));
  printDocument("after connect + initial-state");

  console.log(`[driver] listening for incoming edits for ${DURATION_SECONDS_STR}s...`);
  await new Promise((r) => setTimeout(r, DURATION_MS));
  printDocument("final state");
  process.exit(0);
}

main().catch((err) => {
  console.error("[driver] fatal:", err);
  process.exit(1);
});
