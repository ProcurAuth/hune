// F42 (10.3d, 10.3f, 10.3g): the SQL area with a fake DOM. It tests the write path of the format, the own undo, the keys, the notices, the caps,
// the band text and the fault notice. The folded view, the pair marks and the layout need a browser: the cases B170 to B177 test them.
// The tree is never a security check: each test also looks at the save path (`onChange`) and the run (`onRun`).
import test, { mock } from "node:test";
import assert from "node:assert/strict";

class FakeNode {
  constructor(tag) {
    this.tag = tag;
    this.attrs = new Map();
    this.children = [];
    this.listeners = new Map();
    this.style = { setProperty: () => {} };
    this.dataset = {};
    this.className = "";
    this.hidden = false;
    this.parent = null;
    this.scrollTop = 0;
    this.scrollLeft = 0;
    this.replaceWrites = 0;
    this.valueText = "";
    this.start = 0;
    this.end = 0;
    this.direction = "none";
    this.classList = { add: (c) => { this.className = `${this.className} ${c}`.trim(); }, remove: (c) => { this.className = this.className.split(" ").filter((x) => x !== c).join(" "); }, contains: (c) => this.className.split(" ").includes(c) };
  }

  get value() { return this.valueText; }
  set value(v) { this.valueText = String(v); this.start = this.end = Math.min(this.start, this.valueText.length); }
  get selectionStart() { return this.start; }
  get selectionEnd() { return this.end; }
  get selectionDirection() { return this.direction; }
  get textContent() { return this.children.map((c) => (typeof c === "string" ? c : c.textContent)).join(""); }
  set textContent(v) { this.children = v === "" ? [] : [String(v)]; }
  get firstChild() { return this.children[0] || null; }
  get childNodes() { return this.children; }
  setSelectionRange(a, b, dir = "none") {
    this.start = a;
    this.end = b;
    this.direction = dir;
    // The browser fires selectionchange on the document, also for a change by script.
    for (const fn of docListeners.get("selectionchange") || []) fn({ type: "selectionchange" });
  }
  setRangeText(text, a, b, mode) {
    this.valueText = this.valueText.slice(0, a) + text + this.valueText.slice(b);
    if (mode === "end") this.start = this.end = a + text.length;
  }
  focus() { fakeDocument.activeElement = this; this.fire({ type: "focus" }); }
  blur() { if (fakeDocument.activeElement === this) fakeDocument.activeElement = null; this.fire({ type: "blur" }); }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  getAttribute(k) { return this.attrs.has(k) ? this.attrs.get(k) : null; }
  append(...nodes) { for (const n of nodes) { if (n instanceof FakeNode) n.parent = this; this.children.push(n); } }
  insertBefore(n, ref) { n.parent = this; const i = this.children.indexOf(ref); this.children.splice(i < 0 ? this.children.length : i, 0, n); }
  removeChild(n) { this.children.splice(this.children.indexOf(n), 1); }
  replaceChildren(...nodes) { this.replaceWrites += 1; this.children = []; this.append(...nodes); }
  replaceWith() {}
  remove() {}
  contains(n) { return n === this || this.children.some((c) => c instanceof FakeNode && c.contains(n)); }
  closest() { return null; }
  cloneNode() { return new FakeNode(this.tag); }
  normalize() {}
  querySelectorAll() { return []; }
  querySelector(sel) { return this.find((n) => `[data-testid="${n.dataset.testid}"]` === sel); }
  find(pred) { if (pred(this)) return this; for (const c of this.children) { if (c instanceof FakeNode) { const r = c.find(pred); if (r) return r; } } return null; }
  addEventListener(type, fn) { if (!this.listeners.has(type)) this.listeners.set(type, []); this.listeners.get(type).push(fn); }
  removeEventListener(type, fn) { this.listeners.set(type, (this.listeners.get(type) || []).filter((f) => f !== fn)); }
  fire(event) {
    try {
      event.target = this;
    } catch {
      // A real Event has a target that cannot be set: the handlers of the editor do not read it.
    }
    for (const fn of this.listeners.get(event.type) || []) fn(event);
    return event;
  }
  dispatchEvent(event) { return this.fire(event); }
  click() { this.fire({ type: "click", preventDefault() {} }); }
}

const docListeners = new Map();
const fakeDocument = {
  activeElement: null,
  createElement: (tag) => new FakeNode(tag),
  createTextNode: (t) => String(t),
  createDocumentFragment: () => new FakeNode("fragment"),
  createTreeWalker: () => ({ nextNode: () => null }),
  querySelector: () => null,
  getElementById: () => null,
  querySelectorAll: () => [],
  addEventListener: (type, fn) => docListeners.set(type, [...(docListeners.get(type) || []), fn]),
  removeEventListener: (type, fn) => docListeners.set(type, (docListeners.get(type) || []).filter((f) => f !== fn)),
  commands: [],
  execCommand(command, _ui, text) {
    this.commands.push([command, text]);
    const input = this.activeElement;
    if (!input) return false;
    const before = { type: "beforeinput", inputType: command === "delete" ? "deleteContentBackward" : "insertText", defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
    input.fire(before);
    input.setRangeText(command === "delete" ? "" : text, input.selectionStart, input.selectionEnd, "end");
    // One `input` event for each line, as the browser does (measured: 10.3d).
    for (let i = 0; i < (command === "delete" ? 1 : text.split("\n").length); i++) input.fire(new Event("input"));
    return true;
  },
};
Object.assign(globalThis, { document: fakeDocument, Node: FakeNode, Element: FakeNode, window: globalThis });
const { createSqlEditor, NATIVE_FORMAT_LINES } = await import("../../web/js/editor/sql_editor.js");

const testid = (editor, id) => editor.el.find((n) => n.dataset.testid === id);
const status = (editor) => testid(editor, "sql-status").textContent;
const keyEvent = (init) => ({ type: "keydown", key: "", code: "", ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, repeat: false, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...init });
const FORMAT_KEY = { key: "F", code: "KeyF", ctrlKey: true, shiftKey: true };

// Makes an editor with mock timers. `settle()` runs the delays of the parse (150 ms) and of the caret (50 ms), and the page debounce (300 ms).
function setup(options = {}) {
  mock.timers.enable({ apis: ["setTimeout"] });
  const changes = [];
  let runs = 0;
  fakeDocument.commands = [];
  const editor = createSqlEditor({ lines: options.lines || ["SELECT 1"], onChange: (l) => changes.push(l), onRun: () => { runs += 1; }, dialect: options.dialect, variables: options.variables });
  const settle = (ms = 400) => mock.timers.tick(ms);
  return { editor, changes, runs: () => runs, settle, input: editor.input, press: (init) => editor.input.fire(keyEvent(init)) };
}

test.afterEach(() => {
  mock.timers.reset();
});

test("the tool row has the hooks of 9.29, a live status line and the keys list that the text area names", () => {
  const { editor, input } = setup();
  const live = testid(editor, "sql-status");
  assert.equal(live.getAttribute("aria-live"), "polite");
  assert.ok(testid(editor, "sql-format"));
  assert.equal(testid(editor, "sql-unfold-all").style.visibility, "hidden");
  assert.equal(testid(editor, "sql-undo-format").hidden, true);
  const keys = testid(editor, "sql-keys");
  assert.equal(input.getAttribute("aria-describedby"), keys.getAttribute("id"));
  assert.equal(input.getAttribute("aria-keyshortcuts"), "Control+Shift+F Control+Shift+BracketLeft Control+Shift+BracketRight Control+Shift+Backslash");
  assert.match(keys.textContent, /Ctrl\+Shift\+F/);
  assert.equal(testid(editor, "sql-folded").style.visibility, "hidden");
  assert.equal(testid(editor, "sql-block").style.visibility, "hidden");
  editor.destroy();
});

test("the tools slot puts extra buttons in the tool row, after Format and before Undo format", () => {
  const extra = fakeDocument.createElement("button");
  mock.timers.enable({ apis: ["setTimeout"] });
  const editor = createSqlEditor({ lines: ["SELECT 1"], onChange() {}, tools: [extra] });
  const row = testid(editor, "sql-format").parent;
  assert.ok(row.children.indexOf(extra) > row.children.indexOf(testid(editor, "sql-format")));
  assert.ok(row.children.indexOf(extra) < row.children.indexOf(testid(editor, "sql-unfold-all")));
  const late = fakeDocument.createElement("button");
  editor.addTool(late);
  assert.ok(row.children.indexOf(late) < row.children.indexOf(testid(editor, "sql-undo-format")));
  editor.destroy();
});

test("the format with Ctrl+Shift+F writes with execCommand, makes one repaint, keeps the non-blank caret and sends one change", () => {
  const { editor, input, press, settle, changes } = setup({ lines: ["select a,b from t where x=1 and (y=2 or z=3)"] });
  input.focus();
  input.setSelectionRange(14, 14);
    const code = editor.el.find((n) => n.tag === "code");
  const writesBefore = code.replaceWrites;
  const event = keyEvent(FORMAT_KEY);
  input.fire(event);
  assert.equal(event.defaultPrevented, true);
  assert.equal(input.value, "select a, b\nfrom t\nwhere x = 1 and (y = 2 or z = 3)");
  assert.equal(fakeDocument.commands.length, 1);
  assert.equal(fakeDocument.commands[0][0], "insertText");
  // The fake fires one `input` event for each line. The editor repaints once, after the write.
  assert.equal(code.replaceWrites - writesBefore, 1);
  assert.equal(status(editor), "Formatted.");
  // The caret keeps the number of non-blank characters before it: "select a,b f" has 10 before the caret at 14.
  assert.equal(input.selectionStart, input.selectionEnd);
  assert.equal(input.value.slice(0, input.selectionStart).replace(/\s/g, ""), "select a,b from t where x=1 and (y=2 or z=3)".slice(0, 14).replace(/\s/g, ""));
  assert.equal(testid(editor, "sql-undo-format").hidden, true);
  assert.equal(changes.length, 0);
  settle(400);
  assert.deepEqual(changes, [["select a, b", "from t", "where x = 1 and (y = 2 or z = 3)"]]);
  // A second format changes nothing: no write, no event, and the message of 10.3d.
  fakeDocument.commands = [];
  press(FORMAT_KEY);
  assert.deepEqual(fakeDocument.commands, []);
  assert.equal(status(editor), "Nothing to change.");
  editor.destroy();
});

test("the key Cmd+Shift+F formats, a repeat does not, and a key with Alt is not handled", () => {
  const { editor, input, press } = setup({ lines: ["select a,b from t"] });
  input.focus();
  const mac = keyEvent({ key: "F", code: "KeyF", metaKey: true, shiftKey: true });
  input.fire(mac);
  assert.equal(mac.defaultPrevented, true);
  assert.equal(input.value, "select a, b\nfrom t");
  input.setRangeText("select a,b from t", 0, input.value.length, "end");
  const repeat = keyEvent({ ...FORMAT_KEY, repeat: true });
  input.fire(repeat);
  assert.equal(input.value, "select a,b from t");
  const alt = keyEvent({ ...FORMAT_KEY, altKey: true });
  input.fire(alt);
  assert.equal(alt.defaultPrevented, false);
  assert.equal(input.value, "select a,b from t");
  press({ key: "f", code: "KeyF", ctrlKey: true });
  assert.equal(input.value, "select a,b from t");
  editor.destroy();
});

test("a result of more than 500 lines is written with setRangeText, with an own undo: keys, events and the button", () => {
  const statements = Array.from({ length: 300 }, (_, i) => `select a${i},b${i} from t${i} where x=${i}`);
  const { editor, input, press, settle } = setup({ lines: statements.join(";\n").split("\n") });
  const before = input.value;
  input.focus();
  input.setSelectionRange(14, 14);
  press(FORMAT_KEY);
  assert.deepEqual(fakeDocument.commands, []);
  assert.ok(input.value.split("\n").length > NATIVE_FORMAT_LINES);
  assert.notEqual(input.value, before);
  assert.equal(status(editor), "Formatted. The undo history of the browser is cleared.");
  const undo = testid(editor, "sql-undo-format");
  assert.equal(undo.hidden, false);
  const formatted = input.value;
  // Ctrl+Z restores the text, with the keys of 10.3d.
  const z = keyEvent({ key: "z", code: "KeyZ", ctrlKey: true });
  input.fire(z);
  assert.equal(z.defaultPrevented, true);
  assert.equal(input.value, before);
  assert.equal(undo.hidden, true);
  // Ctrl+Shift+Z, then Cmd+Z, Ctrl+Y: redo, undo, redo.
  const redo = keyEvent({ key: "Z", code: "KeyZ", ctrlKey: true, shiftKey: true });
  input.fire(redo);
  assert.equal(redo.defaultPrevented, true);
  assert.equal(input.value, formatted);
  input.fire(keyEvent({ key: "z", code: "KeyZ", metaKey: true }));
  assert.equal(input.value, before);
  input.fire(keyEvent({ key: "y", code: "KeyY", ctrlKey: true }));
  assert.equal(input.value, formatted);
  // The events historyUndo and historyRedo do the same.
  const hu = { type: "beforeinput", inputType: "historyUndo", defaultPrevented: false, preventDefault() { this.defaultPrevented = true; } };
  input.fire(hu);
  assert.equal(hu.defaultPrevented, true);
  assert.equal(input.value, before);
  input.fire({ type: "beforeinput", inputType: "historyRedo", preventDefault() {} });
  assert.equal(input.value, formatted);
  // The button.
  undo.click();
  assert.equal(input.value, before);
  assert.equal(status(editor), "Format undone.");
  input.fire({ type: "beforeinput", inputType: "historyRedo", preventDefault() {} });
  assert.equal(input.value, formatted);
  // Any other input removes the entry: the keys are then native again.
  input.fire(new Event("input"));
  assert.equal(testid(editor, "sql-undo-format").hidden, true);
  const native = keyEvent({ key: "z", code: "KeyZ", ctrlKey: true });
  input.fire(native);
  assert.equal(native.defaultPrevented, false);
  assert.equal(input.value, formatted);
  settle(400);
  editor.destroy();
});

test("when execCommand fails, the editor uses the own undo", () => {
  const original = fakeDocument.execCommand;
  fakeDocument.execCommand = () => false;
  try {
    const { editor, input, press } = setup({ lines: ["select a,b from t"] });
    input.focus();
    press(FORMAT_KEY);
    assert.equal(input.value, "select a, b\nfrom t");
    assert.equal(testid(editor, "sql-undo-format").hidden, false);
    editor.destroy();
  } finally {
    fakeDocument.execCommand = original;
  }
});

test("the format refuses a text with an error, says where, and moves the caret there", () => {
  const { editor, input, press } = setup({ lines: ["select (a,", "b from t"] });
  input.focus();
  input.setSelectionRange(0, 0);
  press(FORMAT_KEY);
  assert.equal(input.value, "select (a,\nb from t");
  assert.equal(status(editor), "Format needs a complete text: line 1, column 8: a bracket has no match.");
  assert.equal(input.selectionStart, 7);
  assert.deepEqual(fakeDocument.commands, []);
  editor.setLines(["select 'abc", "from t"]);
  press(FORMAT_KEY);
  assert.equal(status(editor), "Format needs a complete text: line 1, column 8: a string, a name or a comment has no end.");
  editor.setLines(["   "]);
  press(FORMAT_KEY);
  assert.equal(status(editor), "Nothing to format.");
  editor.setLines(["SHOW TABLES"]);
  press(FORMAT_KEY);
  assert.equal(status(editor), "Nothing to format.");
  editor.destroy();
});

test("a message that is not a cap goes away after 6 s", () => {
  const { editor, input, press } = setup({ lines: ["select a,b from t"] });
  input.focus();
  press(FORMAT_KEY);
  assert.equal(status(editor), "Formatted.");
  mock.timers.tick(5900);
  assert.equal(status(editor), "Formatted.");
  mock.timers.tick(200);
  assert.equal(status(editor), "");
  editor.destroy();
});

test("a text above a cap gives a notice that stays, and the format refuses with the same notice; save and run go on", () => {
  const { editor, input, press, settle, changes, runs } = setup({ lines: ["SELECT 1"] });
  editor.setLines([`SELECT '${"a".repeat(262136)}'`]);
  settle(200);
  assert.equal(status(editor), "Format and folds are off: the text has more than 262,144 bytes.");
  mock.timers.tick(20000);
  assert.equal(status(editor), "Format and folds are off: the text has more than 262,144 bytes.");
  input.focus();
  press(FORMAT_KEY);
  assert.equal(status(editor), "Format and folds are off: the text has more than 262,144 bytes.");
  assert.deepEqual(fakeDocument.commands, []);
  // The text goes to the page as for any edit, and Ctrl+Enter runs.
  input.fire(new Event("input"));
  settle(400);
  assert.equal(changes.length, 1);
  assert.equal(changes[0].join("\n").length, 262136 + 9);
  mock.timers.tick(7000);
  input.fire(keyEvent({ key: "Enter", code: "Enter", ctrlKey: true }));
  assert.equal(runs(), 1);
  // A text of a lower depth: the notice goes away.
  editor.setLines(["SELECT 1"]);
  settle(200);
  assert.equal(status(editor), "");
  editor.setLines([`SELECT ${"(".repeat(129)}1${")".repeat(129)}`]);
  settle(200);
  assert.equal(status(editor), "Format and folds are off: the text goes deeper than 128 levels.");
  editor.destroy();
});

test("a fault of the tree code gives a notice and never stops the save, the run or the text", () => {
  const { editor, input, press, settle, changes, runs } = setup({ lines: ["SELECT 1"], dialect: "mongodb", variables: () => { throw new Error("boom"); } });
  settle(200);
  assert.match(status(editor), /^Format and folds stopped: an error occurred in the structure code\. The text is not changed\.$/);
  input.focus();
  press({ key: "[", code: "BracketLeft", ctrlKey: true, shiftKey: true });
  press({ key: "\\", code: "Backslash", ctrlKey: true, shiftKey: true });
  input.setRangeText("x", 0, 0, "end");
  input.fire(new Event("input"));
  settle(400);
  assert.deepEqual(changes, [["xSELECT 1"]]);
  input.fire(keyEvent({ key: "Enter", code: "Enter", metaKey: true }));
  assert.equal(runs(), 1);
  assert.equal(input.value, "xSELECT 1");
  editor.destroy();
});

// F42 fix (10.3c rule 1): a fault of the colours (here a RangeError of the highlighter, as a text of 130,000 tokens made it) never loses a
// change of the draft. The change and the parse are armed before the colours are drawn. The text stays plain and the standing notice shows.
test("a fault of the colour layer never stops a draft change: the text stays plain and the notice shows", () => {
  const { editor, input, settle, changes, runs } = setup({ lines: ["SELECT 1"] });
  settle(200);
  const colour = editor.el.find((n) => n.tag === "code");
  colour.replaceChildren = () => { throw new RangeError("Maximum call stack size exceeded"); };
  input.focus();
  input.setRangeText("x", 0, 0, "end");
  input.fire(new Event("input"));
  assert.equal(colour.textContent, "xSELECT 1\n");
  settle(400);
  assert.deepEqual(changes, [["xSELECT 1"]]);
  assert.match(status(editor), /^Format and folds stopped: an error occurred in the structure code\. The text is not changed\.$/);
  // The tree is built for the new version in spite of the fault of the colours, and the run is not touched.
  input.fire(keyEvent({ key: "Enter", code: "Enter", metaKey: true }));
  assert.equal(runs(), 1);
  // A later change with working colours takes the notice away.
  delete colour.replaceChildren;
  input.setRangeText("y", 0, 0, "end");
  input.fire(new Event("input"));
  settle(400);
  assert.deepEqual(changes, [["xSELECT 1"], ["yxSELECT 1"]]);
  assert.equal(status(editor), "");
  editor.destroy();
});

test("a MongoDB source formats as one JSON value and a selection does not matter", () => {
  const { editor, input, press } = setup({ lines: ['[{"$match":{"a":1}}]'], dialect: () => "mongodb" });
  input.focus();
  input.setSelectionRange(2, 5);
  press(FORMAT_KEY);
  assert.equal(input.value, '[{"$match": {"a": 1}}]');
  editor.setLines(['{"a":1,']);
  press(FORMAT_KEY);
  assert.match(status(editor), /^Format needs a complete text: line 1, column 1: the text is not valid JSON\.$/);
  editor.destroy();
});

test("a selection formats the statements that it touches and the dialect follows the function of the source", () => {
  let kind = "clickhouse";
  const { editor, input, press } = setup({ lines: ["select 1 from a where x=1;", "select 2 from b where y=2"], dialect: () => kind });
  input.focus();
  const second = input.value.indexOf("select 2");
  input.setSelectionRange(second + 2, second + 4);
  press(FORMAT_KEY);
  assert.equal(input.value, "select 1 from a where x=1;\nselect 2\nfrom b\nwhere y = 2");
  kind = "postgres";
  editor.setLines(["select $$a(b$$, x::int from t where y=1"]);
  input.setSelectionRange(0, 0);
  press(FORMAT_KEY);
  assert.equal(input.value, "select $$a(b$$, x::int\nfrom t\nwhere y = 1");
  editor.destroy();
});

test("the band and the text of the block follow the caret 50 ms after a selection change, only with a fresh tree and a collapsed caret", () => {
  const { editor, input, settle } = setup({ lines: ["WITH a AS (", "  SELECT 1", ")", "SELECT 2"] });
  settle(200);
  input.focus();
  input.setSelectionRange(20, 20);
  settle(60);
  const info = testid(editor, "sql-info");
  assert.equal(info.textContent, "Block: subquery, lines 1 to 3.");
  const band = testid(editor, "sql-block");
  assert.notEqual(band.style.visibility, "hidden");
  assert.equal(band.dataset.type, "subquery");
  // On the name of the CTE the block is the CTE.
  input.setSelectionRange(5, 5);
  settle(60);
  assert.equal(info.textContent, "Block: cte a, lines 1 to 3.");
  input.setSelectionRange(20, 20);
  settle(60);
  // A selection hides the band.
  input.setSelectionRange(14, 20);
  settle(60);
  assert.equal(band.style.visibility, "hidden");
  assert.equal(info.textContent, "");
  // An input removes it at once, and the parse brings it back after the delay.
  input.setSelectionRange(20, 20);
  settle(60);
  assert.notEqual(band.style.visibility, "hidden");
  input.setRangeText("x", 20, 20, "end");
  input.fire(new Event("input"));
  assert.equal(band.style.visibility, "hidden");
  assert.equal(info.textContent, "");
  settle(200);
  input.setSelectionRange(21, 21);
  settle(60);
  assert.notEqual(band.style.visibility, "hidden");
  // A blur removes it.
  input.blur();
  assert.equal(band.style.visibility, "hidden");
  editor.destroy();
});

test("the parse runs once after the last input, never for each key: the markers go at once and come back after the pause", () => {
  const { editor, input } = setup({ lines: ["SELECT 1"] });
  const count = () => { let n = 0; const visit = (x) => { if (x.dataset && x.dataset.testid === "sql-fold") n += 1; for (const c of x.children || []) if (typeof c !== "string") visit(c); }; visit(editor.el); return n; };
  mock.timers.tick(200);
  editor.setLines(["SELECT (", "1)"]);
  assert.equal(count(), 0);
  mock.timers.tick(200);
  assert.equal(count(), 1);
  const button = testid(editor, "sql-fold");
  assert.equal(button.getAttribute("aria-label"), "Fold lines 1 to 2");
  assert.equal(button.getAttribute("aria-expanded"), "true");
  assert.equal(button.getAttribute("tabindex"), "-1");
  assert.equal(button.dataset.from, "8");
  // An input removes the markers at once. A pause shorter than 150 ms does not let the parse run.
  for (let i = 0; i < 5; i++) {
    input.fire(new Event("input"));
    assert.equal(count(), 0);
    mock.timers.tick(100);
    assert.equal(count(), 0);
  }
  mock.timers.tick(100);
  assert.equal(count(), 1);
  editor.destroy();
});

test("Tab and the keys of 10.3 work as before", () => {
  const { editor, input, press } = setup({ lines: ["a"] });
  input.focus();
  input.setSelectionRange(1, 1);
  const tab = keyEvent({ key: "Tab", code: "Tab" });
  input.fire(tab);
  assert.equal(tab.defaultPrevented, true);
  assert.equal(input.value, "a  ");
  // Escape and then Tab leaves the text area: the second key is not caught.
  press({ key: "Escape", code: "Escape" });
  const out = keyEvent({ key: "Tab", code: "Tab" });
  input.fire(out);
  assert.equal(out.defaultPrevented, false);
  editor.destroy();
});

test("the fold keys say what they did when there is nothing to fold", () => {
  const { editor, input, press, settle } = setup({ lines: ["SELECT 1"] });
  input.focus();
  press({ key: "[", code: "BracketLeft", ctrlKey: true, shiftKey: true });
  assert.equal(status(editor), "Nothing to fold here.");
  press({ key: "]", code: "BracketRight", ctrlKey: true, shiftKey: true });
  assert.equal(status(editor), "Nothing is folded.");
  // Cmd is not a fold key (tab changes in macOS browsers): the key is not handled.
  const meta = keyEvent({ key: "[", code: "BracketLeft", metaKey: true, shiftKey: true });
  input.fire(meta);
  assert.equal(meta.defaultPrevented, false);
  settle(10);
  editor.destroy();
});
