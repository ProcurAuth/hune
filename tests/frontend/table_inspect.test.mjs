// SPDX-License-Identifier: MIT OR Apache-2.0
// The drawer "Inspect value" of a table cell (9.20, 4.18 `inspect`, threat T100): the text (trim, JSON indent, depth and size limits)
// and the dialog, built with a fake `document` (textContent only, hostile values, the copy).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { INSPECT_CAP, indentJson, inspectText } from "../../web/js/table_model.js";
import { FakeNode, installFakeDom } from "./fake_dom.mjs";

// --- the text --------------------------------------------------------------------------------------------------------------------

test("inspectText: the raw value as text, with no blanks at the two ends (Grafana 12.4.2)", () => {
  const cases = [
    [null, ""],
    [undefined, ""],
    [3.5, "3.5"],
    [1794211200000, "1794211200000"],
    [true, "true"],
    ["123", "123"],
    ["18446744073709551615.000000000001", "18446744073709551615.000000000001"],
    ['"quoted"', '"quoted"'],
    ["{bad json", "{bad json"],
    ["plain <b>bold</b> text", "plain <b>bold</b> text"],
    ["  padded\t\n", "padded"],
    ["a\nb", "a\nb"],
    ["", ""],
  ];
  for (const [value, text] of cases) assert.deepEqual(inspectText(value), { text, cut: false }, JSON.stringify(value));
});

test("inspectText: a JSON object or array gets an indent of 2 spaces, as JSON.stringify(v, null, 2) for the same tokens", () => {
  const samples = [
    '{"service":"api","tags":{"env":"prod","zone":"eu-1"},"list":[1,2,[3,4]],"html":"<b>x</b>","n":null,"e":{},"a":[]}',
    "[ 1 , 2 ]",
    '  {"pad":1}  ',
    '[{"id":1,"name":"n1"},{"id":2,"name":"n2"}]',
    "[]",
    "{}",
    '{"a":{ },"b":[ ],"c":[{}]}',
    '{"t":true,"f":false,"z":null,"neg":-1.5}',
  ];
  for (const s of samples) {
    const r = inspectText(s);
    assert.equal(r.text, JSON.stringify(JSON.parse(s), null, 2), s);
    assert.equal(r.cut, false);
  }
  // The tokens stay: a number keeps its digits (JSON.parse would round it) and a string keeps its escapes.
  assert.equal(inspectText('{"id":18446744073709551615,"v":1.10,"e":-1.5e-3}').text, '{\n  "id": 18446744073709551615,\n  "v": 1.10,\n  "e": -1.5e-3\n}');
  assert.equal(inspectText('["a\\"b\\\\c\\u0041",":,{}[]"]').text, '[\n  "a\\"b\\\\c\\u0041",\n  ":,{}[]"\n]');
  // A text that JSON.parse refuses, or a JSON scalar, stays as it is.
  for (const s of ["{1:2}", "[1,]", "{\"a\":1}x", '"{}"', "nulls", "[1] [2]"]) assert.equal(inspectText(s).text, s, s);
});

test("indentJson: 64 levels get the indent, a 65th level gives the raw text", () => {
  const nest = (n) => `${"[".repeat(n)}1${"]".repeat(n)}`;
  assert.equal(inspectText(nest(64)).text, JSON.stringify(JSON.parse(nest(64)), null, 2));
  assert.equal(indentJson(nest(65)), null);
  assert.equal(inspectText(nest(65)).text, nest(65));
  const objects = (n) => `${'{"k":'.repeat(n)}1${"}".repeat(n)}`;
  assert.equal(indentJson(objects(65)), null);
  assert.notEqual(indentJson(objects(64)), null);
});

test("inspectText: the limit of 1,048,576 code units, the note flag and the surrogate pairs", () => {
  assert.equal(INSPECT_CAP, 1048576);
  assert.deepEqual(inspectText("x".repeat(INSPECT_CAP)), { text: "x".repeat(INSPECT_CAP), cut: false });
  const long = inspectText("y".repeat(INSPECT_CAP + 1));
  assert.equal(long.cut, true);
  assert.equal(long.text.length, INSPECT_CAP);
  // The cut does not split the pair of U+1F600 that sits on the limit.
  const pair = inspectText(`${"a".repeat(INSPECT_CAP - 1)}\u{1F600}b`);
  assert.equal(pair.cut, true);
  assert.equal(pair.text.length, INSPECT_CAP - 1);
  assert.ok(!/[\uD800-\uDBFF]$/.test(pair.text));
  // A JSON text longer than the limit gets no indent: the start of the raw text shows.
  const bigArray = `[${Array.from({ length: 200000 }, (_, i) => i).join(",")}]`;
  assert.ok(bigArray.length > INSPECT_CAP);
  const raw = inspectText(bigArray);
  assert.equal(raw.cut, true);
  assert.equal(raw.text, bigArray.slice(0, INSPECT_CAP));
  // A JSON text inside the limit whose indent goes above it: the start of the indented text shows.
  const items = `[${Array.from({ length: 120000 }, (_, i) => i).join(",")}]`;
  assert.ok(items.length <= INSPECT_CAP);
  const indented = inspectText(items);
  assert.equal(indented.cut, true);
  assert.equal(indented.text, JSON.stringify(JSON.parse(items), null, 2).slice(0, INSPECT_CAP));
});

test("indentJson: the work stops at the limit of the output, so a deep and wide text stays linear", () => {
  // 60 levels and many items: the full indent would be about 120 times the input.
  const wide = `${"[".repeat(60)}${Array(100000).fill(1).join(",")}${"]".repeat(60)}`;
  const out = indentJson(wide, INSPECT_CAP);
  assert.ok(out.length <= INSPECT_CAP + 200, `${out.length}`);
  // The minimum of 7 runs after 2 warm-ups: the measure of 2026-11-22 is about 50 ms for 1 MiB of output.
  const times = [];
  for (let k = 0; k < 9; k++) {
    const t0 = performance.now();
    inspectText(wide);
    times.push(performance.now() - t0);
  }
  const best = Math.min(...times.slice(2));
  assert.ok(best < 400, `${best.toFixed(1)} ms`);
});

// --- the dialog with a fake document ---------------------------------------------------------------------------------------------

installFakeDom();
document.body = new FakeNode("body");
FakeNode.prototype.showModal = function showModal() {
  this.open = true;
};
FakeNode.prototype.close = function close() {
  this.dispatch("close");
};
// openDialog looks for a first field with a selector list that the small engine does not read: it finds no field here.
const plainQuery = FakeNode.prototype.querySelector;
FakeNode.prototype.querySelector = function querySelector(sel) {
  return sel.includes(",") ? null : plainQuery.call(this, sel);
};
const copied = [];
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { clipboard: { writeText: async (t) => { copied.push(t); } } } });
const { openInspect } = await import("../../web/js/panels/table.js");

const HOSTILE = [
  "<img src=x onerror=alert(1)>",
  "<script>alert(1)</script>",
  '" onmouseover="alert(1)',
  "</pre><b>x</b>",
  '{"html":"<img src=x onerror=alert(1)>","list":["<svg onload=alert(1)>"]}',
  "‮evil\u0000",
];

test("openInspect: the drawer writes the value and the name with textContent only, and no element comes from the data", () => {
  for (const value of HOSTILE) {
    const name = `col <img src=y onerror=alert(2)>`;
    const dialog = openInspect(name, value);
    assert.equal(dialog.dataset.testid, "inspect-value");
    assert.equal(dialog.querySelector(".dialog__title").textContent, "Inspect value");
    assert.equal(dialog.querySelector(".dialog__subtitle").textContent, name);
    const pre = dialog.querySelector('[data-testid="inspect-value-text"]');
    assert.equal(pre.tag, "pre");
    assert.equal(pre.textContent, inspectText(value).text);
    assert.equal(pre.children.length, 0, "the text is one text, not elements");
    assert.equal(pre.getAttribute("aria-label"), `Value of ${name}`);
    for (const n of dialog.all()) assert.ok(!["img", "script", "b"].includes(n.tag), `${n.tag} in the drawer`);
    // The only svg is the icon of the close button.
    assert.equal(dialog.all().filter((n) => n.tag === "svg").length, 1);
    assert.equal(dialog.querySelector(".inspect__meta"), null, "no note for a short value");
    dialog.close();
  }
  // The JSON text with markup gets the indent and stays text.
  const d = openInspect("j", HOSTILE[4]);
  assert.equal(d.querySelector("pre").textContent, '{\n  "html": "<img src=x onerror=alert(1)>",\n  "list": [\n    "<svg onload=alert(1)>"\n  ]\n}');
  d.close();
});

test("openInspect: the note of the limit, the buttons, and a copy of the shown text that keeps the drawer open", async () => {
  const dialog = openInspect("big", `  ${"z".repeat(INSPECT_CAP + 10)}`);
  const note = dialog.querySelector(".inspect__meta");
  assert.ok(note, "a note above the block");
  assert.equal(note.textContent, "The value has more than 1,048,576 characters. The drawer shows the start.");
  assert.equal(dialog.querySelector("pre").textContent.length, INSPECT_CAP);
  const buttons = dialog.querySelectorAll(".dialog__footer button");
  assert.deepEqual(buttons.map((b) => b.textContent), ["Copy to Clipboard", "Close"]);
  copied.length = 0;
  buttons[0].dispatch("click");
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(copied.length, 1);
  assert.equal(copied[0], "z".repeat(INSPECT_CAP));
  assert.equal(dialog.parent, document.body, "the copy keeps the drawer open");
  buttons[1].dispatch("click");
  assert.equal(dialog.parent, null, "Close removes the drawer");
});

test("the table code builds the button and the drawer with no HTML string", () => {
  const src = readFileSync(new URL("../../web/js/panels/table.js", import.meta.url), "utf8");
  assert.ok(!/innerHTML|outerHTML|insertAdjacentHTML|document\.write/.test(src));
  assert.match(src, /"aria-label": "Inspect value"/);
  assert.match(src, /icon\("eye", 16\)/);
});
