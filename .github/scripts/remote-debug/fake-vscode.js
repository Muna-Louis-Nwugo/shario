// A minimal, functional stand-in for the `vscode` module, just complete
// enough to actually run shario-vscode/extension.js headlessly: a real,
// mutable in-memory document that `WorkspaceEdit` genuinely mutates, so
// reading `doc.getText()` afterward reflects what the real extension code
// actually did -- not a hand-rolled reimplementation of its logic.
//
// Only supports the exact edit shapes extension.js itself ever constructs
// (single-character insert, single-character delete, a delete spanning
// exactly one line-merge, or a whole-document replace -- see
// extension.js's overwriteBuffer) -- this is a test double, not a
// general-purpose vscode shim.

class Position {
  constructor(line, character) {
    this.line = line;
    this.character = character;
  }
}

class Range {
  // real vscode.Range supports both (startLine, startChar, endLine, endChar)
  // and (startPosition, endPosition) -- extension.js's overwriteBuffer uses
  // the latter to span a whole document without needing to know its length
  // up front, so both forms need to work here too.
  constructor(a, b, c, d) {
    if (a instanceof Position) {
      this.start = a;
      this.end = b;
    } else {
      this.start = new Position(a, b);
      this.end = new Position(c, d);
    }
  }
}

class FakeDocument {
  constructor(fsPath, initialText) {
    this.uri = { scheme: "file", fsPath, toString: () => fsPath };
    this._lines = initialText.split("\n");
  }

  get lineCount() {
    return this._lines.length;
  }

  lineAt(lineNumber) {
    const text = this._lines[lineNumber];
    return { text, range: new Range(lineNumber, 0, lineNumber, text.length) };
  }

  getText() {
    return this._lines.join("\n");
  }

  _insert(position, text) {
    const line = this._lines[position.line];
    if (text === "\n") {
      const before = line.slice(0, position.character);
      const after = line.slice(position.character);
      this._lines.splice(position.line, 1, before, after);
    } else {
      this._lines[position.line] = line.slice(0, position.character) + text + line.slice(position.character);
    }
  }

  _delete(range) {
    if (range.start.line === range.end.line) {
      const line = this._lines[range.start.line];
      this._lines[range.start.line] = line.slice(0, range.start.character) + line.slice(range.end.character);
      return;
    }
    // exactly one line-merge (end.line === start.line + 1, end.character === 0)
    const merged = this._lines[range.start.line] + this._lines[range.end.line];
    this._lines.splice(range.start.line, 2, merged);
  }

  _replace(text) {
    // only ever used for a whole-document replace (overwriteBuffer always
    // spans (0,0) to the document's real end) -- not a general arbitrary-range
    // implementation, so the range itself is irrelevant here
    this._lines = text.split("\n");
  }
}

class WorkspaceEdit {
  constructor() {
    this._ops = [];
  }
  insert(uri, position, text) {
    this._ops.push({ kind: "insert", uri, position, text });
  }
  delete(uri, range) {
    this._ops.push({ kind: "delete", uri, range });
  }
  replace(uri, range, text) {
    this._ops.push({ kind: "replace", uri, range, text });
  }
}

function createFakeVscode({ documents, workspaceFolderPath }) {
  const registeredCommands = {};
  const changeListeners = [];

  const vscode = {
    Position,
    Range,
    WorkspaceEdit,
    workspace: {
      workspaceFolders: [{ uri: { fsPath: workspaceFolderPath } }],
      textDocuments: documents,
      onDidChangeTextDocument: (listener) => {
        changeListeners.push(listener);
        return { dispose() {} };
      },
      getConfiguration: () => ({ get: (_key, def) => def }),
      saveAll: async () => true,
      applyEdit: async (edit) => {
        for (const op of edit._ops) {
          const doc = documents.find((d) => d.uri.toString() === op.uri.toString());
          if (!doc) continue;
          if (op.kind === "insert") doc._insert(op.position, op.text);
          else if (op.kind === "delete") doc._delete(op.range);
          else doc._replace(op.text);
        }
        return true;
      },
    },
    window: {
      createOutputChannel: () => ({
        appendLine: (s) => console.log("[shar output]", s),
        append: (s) => process.stdout.write(`[shar output] ${s}`),
      }),
      showInformationMessage: (msg) => {
        console.log("[info]", msg);
        return Promise.resolve(undefined);
      },
      showErrorMessage: (msg) => {
        console.log("[error]", msg);
        return Promise.resolve(undefined);
      },
      showWarningMessage: (msg, ...choices) => {
        console.log("[warning]", msg);
        return Promise.resolve(choices[choices.length - 1]);
      },
      showInputBox: async () => undefined,
      visibleTextEditors: [],
    },
    commands: {
      registerCommand: (name, handler) => {
        registeredCommands[name] = handler;
        return { dispose() {} };
      },
    },
    env: { clipboard: { writeText: async () => {} } },
  };

  return { vscode, registeredCommands };
}

module.exports = { createFakeVscode, FakeDocument, Position, Range };
