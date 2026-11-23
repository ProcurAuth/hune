// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-49 (10.3b, T79): the form of an open variable in the editor. It tests the three SQL examples with the lexer of the page, the insert of an example,
// the preview "Show options" with a fake document and 40 hostile texts, the body of the preview of a variable, the switch member and the default rule.
// The page part of F41 (the combobox, 9.10) has its own test file (FE-48).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { EXAMPLES, PREVIEW_ROWS, SAVE_FIRST, addExampleHints, countText, createPreview, errorText, exampleItems, exampleText, insertExample, previewRows } from "../../web/js/editor/forms/var_examples.js";
import { variablePreviewBody } from "../../web/js/editor/preview_body.js";
import { newVariable, setAllowCustom } from "../../web/js/editor/forms/model.js";
import { defaultErrors } from "../../web/js/editor/forms/variables.js";
import { findMacros, findVariableNames } from "../../web/js/sqlscan.js";
import { parseSql } from "../../web/js/editor/ast/sql_tree.js";

const vectors = JSON.parse(readFileSync(new URL("./vectors/FE-49-open-variables-form.json", import.meta.url)));
const HOSTILE = vectors.hostile;

class Node {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
    this.attrs = new Map();
    this.dataset = {};
    this.listeners = new Map();
    this.className = "";
    this.hidden = false;
    this.disabled = false;
    this.htmlWrites = 0;
  }

  set innerHTML(_) { this.htmlWrites += 1; }
  get innerHTML() { return ""; }
  get textContent() { return this.children.map((c) => (typeof c === "string" ? c : c.textContent)).join(""); }
  set textContent(v) { this.children = v === "" ? [] : [String(v)]; }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  getAttribute(k) { return this.attrs.has(k) ? this.attrs.get(k) : null; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = [...nodes]; }
  addEventListener(type, fn) { this.listeners.set(type, [...(this.listeners.get(type) || []), fn]); }
  async click() { for (const fn of this.listeners.get("click") || []) await fn({ type: "click" }); }
  find(pred) {
    if (pred(this)) return this;
    for (const c of this.children) {
      const r = typeof c === "string" ? null : c.find(pred);
      if (r) return r;
    }
    return null;
  }

  all() { return [this, ...this.children.flatMap((c) => (typeof c === "string" ? [] : c.all()))]; }
  querySelector(sel) { return this.find((n) => `[data-testid="${n.dataset.testid}"]` === sel); }
}
const fakeDoc = () => ({ createElement: (tag) => new Node(tag), execCommand: () => false });
const TAGS = new Set(["div", "button", "p", "ul", "li", "code", "span"]);

// A promise that the test settles by hand, to see the state while a request runs.
const deferred = () => {
  let resolve;
  let reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
};
const tick = () => new Promise((r) => setImmediate(r));

function block(doc, over = {}) {
  const calls = [];
  const variable = { kind: "query", name: "v", source: "s", sql: ["SELECT 1"], regex: null, sort: "none", multi: false, include_all: false, default: [] };
  const preview = createPreview(doc, {
    getVariable: () => variable,
    canUse: () => true,
    hasDashboard: () => true,
    send: (signal) => { calls.push(signal); return Promise.resolve({ options: [{ value: "a", text: "A" }] }); },
    ...over,
  });
  const at = (id) => preview.node.querySelector(`[data-testid="${id}"]`);
  return { preview, calls, variable, button: at("variable-form-preview"), status: at("variable-preview-status"), list: at("variable-preview-list") };
}

test("the three SQL examples: four lines each, valid with the lexer of the page, the macro and the names of 10.3b", () => {
  assert.equal(EXAMPLES.length, 3);
  assert.deepEqual(EXAMPLES.map((e) => e.lines.length), [4, 4, 4]);
  assert.equal(EXAMPLES[1].hint, "Set Refresh to On time range change.");
  assert.equal(EXAMPLES[0].hint, undefined);
  assert.equal(exampleText(0), "SELECT service\nFROM events\nWHERE timestamp >= now() - INTERVAL 3 DAY\nGROUP BY service ORDER BY max(timestamp) DESC LIMIT 1000");
  const macros = [[], ["__timeFilter"], []];
  EXAMPLES.forEach((e, n) => {
    const text = exampleText(n);
    // No variable name, no semicolon, no comment, the limit of 1,000 rows and balanced brackets.
    assert.deepEqual(findVariableNames(text), [], `example ${n}`);
    assert.deepEqual(findMacros(text), macros[n], `example ${n}`);
    assert.ok(!/;|--|\/\*|'/.test(text), `example ${n}`);
    assert.match(text, /LIMIT 1000$/, `example ${n}`);
    assert.equal(text.split("(").length, text.split(")").length, `example ${n}`);
    const tree = parseSql(text, "clickhouse");
    assert.equal(tree.capped, null);
    assert.ok(tree.tokens.every((t) => t.closed !== false), `example ${n}`);
    assert.equal(tree.tokens.at(-1).end, text.length);
  });
  assert.match(exampleText(2), /^SELECT DISTINCT toString\(id\) AS value, name AS text\n/);
});

test("the menu items have the hooks variable-sql-example-<n>, from 0, and the hint goes under its label as text", () => {
  const picked = [];
  const items = exampleItems((n) => picked.push(n));
  assert.deepEqual(items.map((i) => i.testid), ["variable-sql-example-0", "variable-sql-example-1", "variable-sql-example-2"]);
  items[2].onSelect();
  assert.deepEqual(picked, [2]);
  const doc = fakeDoc();
  const popup = new Node("div");
  for (const i of items) {
    const b = new Node("button");
    b.dataset.testid = i.testid;
    b.textContent = i.label;
    popup.append(b);
  }
  addExampleHints(popup, doc);
  const second = popup.querySelector('[data-testid="variable-sql-example-1"]');
  assert.equal(second.textContent, `${EXAMPLES[1].label}${EXAMPLES[1].hint}`);
  assert.equal(popup.querySelector('[data-testid="variable-sql-example-0"]').textContent, EXAMPLES[0].label);
});

test("an example goes in with insertText so the undo of the browser works, and falls back to setRangeText", () => {
  const log = [];
  const input = { focus: () => log.push("focus"), selectionStart: 3, selectionEnd: 5, setRangeText: (...a) => log.push(["range", ...a]), dispatchEvent: (e) => log.push(e.type) };
  insertExample(input, "TEXT", { execCommand: (...a) => { log.push(a); return true; } });
  assert.deepEqual(log, ["focus", ["insertText", false, "TEXT"]]);
  log.length = 0;
  insertExample(input, "TEXT", { execCommand: () => false });
  assert.deepEqual(log, ["focus", ["range", "TEXT", 3, 5, "end"], "input"]);
  log.length = 0;
  insertExample(input, "TEXT", { execCommand: () => { throw new Error("no"); } });
  assert.deepEqual(log, ["focus", ["range", "TEXT", 3, 5, "end"], "input"]);
});

test("the rows of the preview use the rules of the page and stop at 50 rows", () => {
  const raw = Array.from({ length: 120 }, (_, i) => ({ value: `v${String(i).padStart(3, "0")}`, text: `t${i}` }));
  const r = previewRows([...raw, raw[0]], { regex: null, sort: "alpha_asc" });
  assert.equal(PREVIEW_ROWS, 50);
  assert.equal(r.count, 120);
  assert.equal(r.rows.length, 50);
  assert.equal(r.more, 70);
  assert.deepEqual(previewRows([{ value: "a", text: "A" }, { value: "b", text: "B" }], { regex: "/A/", sort: "none" }).rows, [{ value: "a", text: "A" }]);
  assert.deepEqual(previewRows(undefined, { regex: null, sort: "none" }), { count: 0, rows: [], more: 0 });
  assert.equal(countText(1), "1 option");
  assert.equal(countText(0), "0 options");
  assert.equal(countText(120), "120 options");
});

test("the status of an error: the message, the line, and the pointers of invalid_document", () => {
  assert.equal(errorText({ code: "query_error", message: "Boom" }), "Boom");
  assert.equal(errorText({ code: "sql_invalid", message: "Bad", extra: { line: 2, column: 7 } }), "Bad (line 2, column 7)");
  assert.equal(errorText({ code: "invalid_document", message: "No.", extra: { errors: [{ pointer: "/dashboard_variables/0/sql/1", message: "Nope." }] } }), "No. /dashboard_variables/0/sql/1: Nope.");
  const many = { code: "invalid_document", message: "No.", extra: { errors: Array.from({ length: 5 }, (_, i) => ({ pointer: `/p${i}`, message: "m" })) } };
  assert.match(errorText(many), /And 2 more\.$/);
  assert.equal(errorText(null), "The options did not load.");
});

test("the preview block: hooks, the count, the rows as value and text, the status line is live", async () => {
  const doc = fakeDoc();
  const b = block(doc, { send: async () => ({ options: [{ value: "a", text: "A" }, { value: "b", text: "b" }] }) });
  assert.ok(b.button && b.status && b.list);
  assert.equal(b.button.textContent, "Show options");
  assert.equal(b.status.getAttribute("aria-live"), "polite");
  assert.equal(b.list.hidden, true);
  await b.button.click();
  assert.equal(b.status.textContent, "2 options");
  assert.deepEqual(b.list.children.map((li) => li.textContent), ["a A", "b"]);
  assert.equal(b.list.hidden, false);
  assert.equal(b.button.disabled, false);
  b.preview.clear();
  assert.equal(b.status.textContent, "");
  assert.equal(b.list.children.length, 0);
});

test("more than 50 options: 50 rows and the text 'and <n> more'", async () => {
  const doc = fakeDoc();
  const options = Array.from({ length: 53 }, (_, i) => ({ value: `v${i}`, text: `v${i}` }));
  const b = block(doc, { send: async () => ({ options }) });
  await b.button.click();
  assert.equal(b.status.textContent, "53 options");
  assert.equal(b.list.children.length, 51);
  assert.equal(b.list.children.at(-1).textContent, "and 3 more");
});

test("the button is not active while a request runs, and the answer of a cleared request is dropped", async () => {
  const doc = fakeDoc();
  const d = deferred();
  const b = block(doc, { send: () => d.promise });
  const first = b.button.click();
  await tick();
  assert.equal(b.button.disabled, true);
  assert.equal(b.status.textContent, "Loading the options.");
  await b.button.click();
  assert.equal(b.calls.length, 0);
  b.preview.clear();
  assert.equal(b.button.disabled, false);
  d.resolve({ options: [{ value: "late", text: "late" }] });
  await first;
  assert.equal(b.status.textContent, "");
  assert.equal(b.list.children.length, 0);
});

test("an error of the request shows in the status line, and an abort shows nothing", async () => {
  const doc = fakeDoc();
  const bad = block(doc, { send: async () => { throw Object.assign(new Error("The query failed."), { code: "query_error" }); } });
  await bad.button.click();
  assert.equal(bad.status.textContent, "The query failed.");
  assert.match(bad.status.className, /field-error/);
  assert.equal(bad.button.disabled, false);
  const aborted = block(doc, { send: async () => { throw new Error("aborted"); }, isAbort: () => true });
  await aborted.button.click();
  assert.equal(aborted.status.textContent, "Loading the options.");
});

test("a dashboard with no uid gives the notice 'Save the dashboard to show the options.' and sends nothing", async () => {
  const doc = fakeDoc();
  const b = block(doc, { hasDashboard: () => false });
  await b.button.click();
  assert.equal(b.status.textContent, SAVE_FIRST);
  assert.equal(SAVE_FIRST, "Save the dashboard to show the options.");
  assert.equal(b.calls.length, 0);
});

test("the block is absent for a source that the person cannot use", () => {
  const doc = fakeDoc();
  let use = false;
  const preview = createPreview(doc, { getVariable: () => ({}), canUse: () => use, hasDashboard: () => true, send: async () => ({}) });
  assert.equal(preview.node.hidden, true);
  use = true;
  preview.sync();
  assert.equal(preview.node.hidden, false);
});

test("FE-49: 40 hostile texts are text nodes in the preview rows and in the status line (T79)", async () => {
  assert.equal(HOSTILE.length, 40);
  const doc = fakeDoc();
  const options = HOSTILE.map((s, i) => ({ value: `${i}:${s}`, text: s }));
  const b = block(doc, { send: async () => ({ options: options.slice(0, 40) }), getVariable: () => ({ regex: null, sort: "none" }) });
  await b.button.click();
  const nodes = b.preview.node.all();
  assert.ok(nodes.every((n) => TAGS.has(n.tag)), "only the tags of the block exist");
  assert.ok(nodes.every((n) => n.htmlWrites === 0));
  // 40 rows: no more than 50. Each text is in one node as a string.
  assert.equal(b.list.children.length, 40);
  HOSTILE.forEach((s, i) => {
    const row = b.list.children[i];
    assert.equal(row.children.length, 2);
    assert.equal(row.children[0].children[0], `${i}:${s}`);
    assert.equal(row.children[1].children[0], ` ${s}`);
  });
  for (const s of HOSTILE) {
    const failing = block(doc, { send: async () => { throw Object.assign(new Error(s), { code: "query_error" }); } });
    await failing.button.click();
    assert.deepEqual(failing.status.children, [s]);
    assert.ok(failing.preview.node.all().every((n) => TAGS.has(n.tag) && n.htmlWrites === 0));
  }
});

test("the body of the preview of a variable: the variable and its parents, the values of the parents only, no max_data_points", () => {
  const parent = { kind: "custom", name: "env", options: [{ text: "Prod", value: "prod" }], multi: false, include_all: false, all_value: null, default: [] };
  const other = { kind: "text", name: "free", value_type: "text", default: "x" };
  const self = { ...newVariable("query", "svc", "s"), sql: ["SELECT s FROM t WHERE env = '$env'"], allow_custom_value: true };
  const doc = { variables: [other, parent, self] };
  const range = { from_ms: 10, to_ms: 20 };
  const body = variablePreviewBody({ doc, variable: self, target: { uid: "abc" }, range, known: { env: { all: false, values: ["dev"] } } });
  assert.deepEqual(Object.keys(body), ["uid", "from_ms", "to_ms", "dashboard_variables", "variables", "variable"]);
  assert.deepEqual(body.dashboard_variables.map((v) => v.name), ["env", "svc"]);
  assert.equal(body.dashboard_variables[1].allow_custom_value, true);
  assert.deepEqual(body.variables, { env: { all: false, values: ["dev"] } });
  assert.equal(body.variable, "svc");
  assert.ok(!("max_data_points" in body) && !("panel" in body));
  // A parent that the page does not know takes its default (the first option of a custom variable).
  assert.deepEqual(variablePreviewBody({ doc, variable: self, target: { uid: "abc" }, range, known: {} }).variables, { env: { all: false, values: ["prod"] } });
  // A variable with no parent.
  const alone = variablePreviewBody({ doc: { variables: [other, newVariable("query", "q", "s")] }, variable: newVariable("query", "q", "s"), target: { uid: "u" }, range, known: {} });
  assert.deepEqual(alone.variables, {});
  assert.deepEqual(alone.dashboard_variables.map((v) => v.name), ["q"]);
});

test("setAllowCustom: the member follows default and comes before placement; false takes it out; text never gets it (4.38)", () => {
  const keys = (o) => Object.keys(o);
  const placed = { ...newVariable("query", "a", "s"), placement: { section: 0, x: 0, y: 0, w: 6, h: 2 } };
  const open = setAllowCustom(placed, true);
  assert.deepEqual(keys(open).slice(-3), ["default", "allow_custom_value", "placement"]);
  assert.equal(open.allow_custom_value, true);
  assert.deepEqual(keys(setAllowCustom(open, false)), keys(placed));
  assert.deepEqual(keys(setAllowCustom(newVariable("custom", "b"), true)), ["kind", "name", "label", "hide", "options", "multi", "include_all", "all_value", "default", "allow_custom_value"]);
  assert.deepEqual(keys(setAllowCustom(setAllowCustom(newVariable("custom", "b"), true), true)).filter((k) => k === "allow_custom_value").length, 1);
  assert.deepEqual(keys(setAllowCustom(newVariable("text", "t"), true)), keys(newVariable("text", "t")));
  // A new variable is closed: no member.
  assert.ok(!("allow_custom_value" in newVariable("query", "a", "s")));
  assert.ok(!("allow_custom_value" in newVariable("custom", "a")));
  // The copy never changes the source.
  assert.ok(!("allow_custom_value" in placed));
});

test("defaultErrors: a closed custom variable needs an option, an open one takes a typed value (4.11, rules 4 and 5)", () => {
  const closed = { ...newVariable("custom", "c"), options: [{ text: "One", value: "1" }], multi: true, default: ["1", "zzz"] };
  assert.equal(defaultErrors(closed), 'The default "zzz" is not an option.');
  assert.equal(defaultErrors({ ...closed, allow_custom_value: false }), 'The default "zzz" is not an option.');
  const open = { ...closed, allow_custom_value: true };
  assert.equal(defaultErrors(open), "");
  assert.match(defaultErrors({ ...open, default: ["x".repeat(1025)] }), /has more than 1024 bytes\.$/);
  assert.equal(defaultErrors({ ...open, default: ["x".repeat(1024)] }), "");
  // 1,024 bytes are 512 two-byte characters: 513 of them are 1,026 bytes.
  assert.equal(defaultErrors({ ...open, default: ["é".repeat(512)] }), "");
  assert.match(defaultErrors({ ...open, default: ["é".repeat(513)] }), /more than 1024 bytes/);
  for (const bad of ["a\u0000b", "a\u001fb", "a\u007fb", "a\nb"]) assert.match(defaultErrors({ ...open, default: [bad] }), /has a control character\.$/);
  // The other rules do not change with the switch.
  assert.equal(defaultErrors({ ...open, multi: false }), "A variable with one value has one default at most.");
  assert.equal(defaultErrors({ ...open, default: ["$__all"] }), "The default $__all needs the option All.");
  assert.equal(defaultErrors({ ...open, include_all: true, default: ["$__all"] }), "");
  // A query variable takes a default that is not an option, open or closed.
  const q = { ...newVariable("query", "q", "s"), multi: true, default: ["anything"] };
  assert.equal(defaultErrors(q), "");
  assert.equal(defaultErrors({ ...q, allow_custom_value: true }), "");
});
