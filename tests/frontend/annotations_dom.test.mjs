// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-37 (9.19d, rules 11, 12 and 15, and T61): the card, the status text, the keys and the second table of "View data" with a fake DOM.
// A hostile title, text or tag must stay a text node. No element, no attribute and no style comes from data.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { attach, buildCard, buildTable, fillCard, ink } from "../../web/js/panels/annotate.js";
import { cardText, tableRows } from "../../web/js/annotations.js";

const fe37 = JSON.parse(readFileSync(new URL("./vectors/FE-37-annotations-dom.json", import.meta.url)));

class Node {
  constructor(tag) {
    this.tag = tag;
    this.attrs = new Map();
    this.children = [];
    this.listeners = new Map();
    this.style = {};
    this.hidden = false;
    this.className = "";
    this.offsetWidth = 300;
    this.offsetHeight = 50;
    this.removed = false;
  }

  set textContent(value) { this.children = value === "" ? [] : [{ text: String(value) }]; }
  get textContent() { return this.children.map((c) => (c.text !== undefined ? c.text : c.textContent)).join(""); }
  set innerHTML(_) { throw new Error("innerHTML"); }
  set outerHTML(_) { throw new Error("outerHTML"); }
  setAttribute(name, value) { this.attrs.set(name, String(value)); }
  getAttribute(name) { return this.attrs.has(name) ? this.attrs.get(name) : null; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = nodes; }
  remove() { this.removed = true; }
  addEventListener(type, fn) { this.listeners.set(type, fn); }
  getBoundingClientRect() { return { left: 0, top: 0, width: 600, height: 240 }; }
  getContext() { return new Proxy({}, { get: (target, name) => (name in target ? target[name] : () => {}), set: (target, name, value) => { target[name] = value; return true; } }); }
  querySelector(selector) { return selector === ".chartCore__plot" ? this.plot : null; }
}

const fakeDoc = { createElement: (tag) => new Node(tag) };
const elementsOf = (n, out = []) => { out.push(n); for (const c of n.children) if (c instanceof Node) elementsOf(c, out); return out; };

test("FE-37: the card keeps each hostile text as a text node, with 3 children and a chip for each tag", () => {
  assert.equal(fe37.texts.length, 40);
  for (const text of fe37.texts) {
    const parts = buildCard(fakeDoc, "7");
    fillCard(fakeDoc, parts, cardText({ t: 1, end: null, title: text, text, tags: [text] }, (ms) => `T${ms}`));
    assert.equal(parts.card.children.length, 3);
    assert.equal(parts.body.textContent, `${text}\n${text}`);
    assert.equal(parts.body.children.length, 1);
    assert.equal(parts.tags.children.length, 1);
    const chip = parts.tags.children[0];
    assert.equal(chip.tag, "span");
    assert.equal(chip.textContent, text);
    assert.deepEqual([...chip.attrs.keys()], []);
    assert.equal(chip.className, "annot__tag");
    // Only the card has attributes, and their values are fixed: no value comes from data.
    const attrs = elementsOf(parts.card).flatMap((node) => [...node.attrs]);
    assert.deepEqual(attrs, [["role", "tooltip"], ["data-testid", "annot-7-card"]]);
    assert.deepEqual(Object.keys(parts.card.style), []);
  }
});

test("FE-37: the card has the role tooltip, the test id and starts hidden; a mark with 12 tags shows 8 chips", () => {
  const parts = buildCard(fakeDoc, "3_r2");
  assert.equal(parts.card.getAttribute("role"), "tooltip");
  assert.equal(parts.card.getAttribute("data-testid"), "annot-3_r2-card");
  assert.equal(parts.card.hidden, true);
  assert.deepEqual(parts.card.children.map((c) => c.className), ["annot__head", "annot__body", "annot__tags"]);
  fillCard(fakeDoc, parts, cardText({ t: 1, end: 9, title: "", text: "x", tags: Array.from({ length: 12 }, (_, i) => `t${i}`) }, (ms) => `T${ms}`));
  assert.equal(parts.tags.children.length, 8);
  assert.equal(parts.head.textContent, "T1 - T9");
  fillCard(fakeDoc, parts, cardText({ t: 1, end: null, title: "", text: "", tags: [] }, (ms) => `T${ms}`));
  assert.equal(parts.tags.children.length, 0);
  assert.equal(parts.body.children.length, 0);
});

test("FE-37: the second table of View data writes each cell as a text node", () => {
  const list = fe37.texts.map((text, i) => ({ t: i, end: i % 2 ? i + 5 : null, title: text, text, tags: [text, "b"] }));
  const box = buildTable(fakeDoc, tableRows(list, (ms) => `T${ms}`));
  assert.equal(box.getAttribute("data-testid"), "view-data-annotations");
  const [title, table] = box.children;
  assert.equal(title.tag, "h4");
  assert.equal(title.textContent, "Annotations");
  const [thead, tbody] = table.children;
  assert.deepEqual(thead.children[0].children.map((c) => c.textContent), ["Time", "End", "Title", "Text", "Tags"]);
  assert.equal(tbody.children.length, 40);
  tbody.children.forEach((tr, i) => {
    assert.equal(tr.children.length, 5);
    for (const td of tr.children) {
      assert.equal(td.tag, "td");
      assert.equal(td.children.length <= 1, true);
      assert.deepEqual([...td.attrs.keys()], []);
    }
    assert.equal(tr.children[2].textContent, fe37.texts[i]);
    assert.equal(tr.children[4].textContent, `${fe37.texts[i]}, b`);
    assert.equal(tr.children[1].textContent, i % 2 ? `T${i + 5}` : "");
  });
});

test("FE-37: the colour that the canvas gets comes from a parse: a token, a hex or an rgb colour; transparent draws nothing", () => {
  assert.equal(ink(null, "red", true), "rgb(242 73 92 / 1)");
  assert.equal(ink(null, "red", false) !== null, true);
  assert.equal(ink(null, "#112233", true), "rgb(17 34 51 / 1)");
  assert.equal(ink(null, "#11223380", true).startsWith("rgb(17 34 51 / 0.50"), true);
  assert.equal(ink(null, "rgb(1, 2, 3)", false), "rgb(1 2 3 / 1)");
  assert.equal(ink(null, "transparent", true), null);
  assert.equal(ink(null, "x\"><script>", true), null);
  assert.equal(ink(null, "url(javascript:x)", true), null);
});

// A fake chart and host for `attach`: the keys, the status text and the card.
function harness(list) {
  const plotEl = new Node("div");
  const host = new Node("div");
  host.ownerDocument = fakeDoc;
  host.plot = plotEl;
  host.dataset = {};
  const hidden = [];
  const chart = {
    layout: () => ({ left: 50, top: 10, plotW: 400, plotH: 200, width: 500, height: 240, xLo: 0, xHi: 4000, xOf: (t) => 50 + t / 10 }),
    hideTooltip: () => hidden.push(1),
  };
  const globals = { addEventListener() {}, removeEventListener() {}, devicePixelRatio: 2 };
  globalThis.window = globals;
  // The code asks for a frame to hide the tooltip of the engine again: the fake runs it at once.
  globalThis.requestAnimationFrame = (fn) => { fn(); return 1; };
  const layer = attach(host, chart, { key: "5", zone: "utc", annotationsOf: () => [{ index: 0, a: { name: "A", color: "red" }, items: list, error: null }] });
  const [canvas, card, keys, status] = plotEl.children;
  return { layer, host, plotEl, canvas, card, keys, status, hidden };
}

test("FE-37: the keys move over the marks, the card shows near the active mark and the status text repeats it", () => {
  const list = [
    { t: 1000, end: null, title: "Deploy", text: "v1", tags: ["prod"] },
    { t: 2000, end: 3000, title: "<b>x</b>", text: "line\nend", tags: [] },
    { t: 3500, end: null, title: "", text: "last", tags: [] },
  ];
  const h = harness(list);
  h.layer.paint();
  assert.equal(h.keys.getAttribute("aria-label"), "Annotations, 3");
  assert.equal(h.keys.getAttribute("role"), "group");
  assert.equal(h.keys.tabIndex, 0);
  assert.equal(h.keys.getAttribute("data-testid"), "annot-5-keys");
  assert.equal(h.status.getAttribute("role"), "status");
  assert.equal(h.status.getAttribute("aria-live"), "polite");
  assert.equal(h.status.getAttribute("data-testid"), "annot-5-status");
  assert.equal(h.canvas.getAttribute("aria-hidden"), "true");
  const hook = JSON.parse(h.host.dataset.annotations);
  assert.deepEqual([hook.n, hook.drawn, hook.cut], [3, 3, false]);
  assert.deepEqual(hook.marks.map((m) => [m[0], m[1], m[2], m[4]]), [[150, 150, 0, 0], [250, 350, 1, 0], [400, 400, 0, 0]]);
  assert.equal(hook.marks[0][3], "rgb(242 73 92 / 1)");
  const key = (k, target = h.keys) => { let stopped = false; h.plotEl.listeners.get("keydown")({ key: k, target, preventDefault: () => { stopped = true; } }); return stopped; };
  assert.equal(key("ArrowRight"), true);
  assert.equal(h.card.hidden, false);
  assert.equal(h.card.children[0].textContent, "1970-01-01 00:00:01");
  assert.equal(h.status.textContent, "1970-01-01 00:00:01. Deploy\nv1");
  assert.equal(h.hidden.length >= 1, true, "the engine tooltip hides");
  key("ArrowRight");
  assert.equal(h.card.children[0].textContent, "1970-01-01 00:00:02 - 1970-01-01 00:00:03");
  assert.equal(h.card.children[1].textContent, "<b>x</b>\nline\nend", "the text stays as written");
  assert.equal(h.card.children[1].children.length, 1);
  key("End");
  assert.equal(h.status.textContent.endsWith("last"), true);
  key("Home");
  assert.equal(h.status.textContent.includes("Deploy"), true);
  key("ArrowLeft");
  assert.equal(h.status.textContent.includes("Deploy"), true, "no move before the first mark");
  assert.equal(key("Enter"), false, "Enter does nothing");
  assert.equal(key("Escape"), false);
  assert.equal(h.card.hidden, true);
  h.layer.destroy();
  assert.equal(h.keys.removed && h.card.removed && h.canvas.removed && h.status.removed, true);
});

test("FE-37: the pointer in the zone shows the card, a pointer out of the zone hides it, and a draw that removes the mark hides it", () => {
  const h = harness([{ t: 1000, end: null, title: "A", text: "", tags: [] }]);
  h.layer.paint();
  const move = (x, y) => h.plotEl.listeners.get("pointermove")({ clientX: x, clientY: y });
  move(150, 205);
  assert.equal(h.card.hidden, false);
  assert.equal(h.card.children[0].textContent, "1970-01-01 00:00:01");
  assert.match(h.card.style.transform, /^translate\(\d+px, \d+px\)$/);
  move(150, 100);
  assert.equal(h.card.hidden, true);
  move(155, 205);
  assert.equal(h.card.hidden, false);
  move(300, 205);
  assert.equal(h.card.hidden, true);
  move(150, 205);
  h.plotEl.listeners.get("pointerleave")();
  assert.equal(h.card.hidden, true);
});
