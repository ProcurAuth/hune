// Tests of the select-and-copy model (9.8e): web/js/panels/copy_text.js (the controls that a browser does not let a person select) and
// web/js/panels/text_layer.js (the layer of text over a chart canvas): the place of a text, the reading order, the signature, the cap, the wrapper of
// fillText and clearRect on a fake context, and the text that a copy gives.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { SELECTABLE, isDrag, labelOf } from "../../web/js/panels/copy_text.js";
import { MAX_TEXTS, installTextLayer, parseFont, placeText, readingOrder, scaledFont, selectedLayerText, signature, textItems } from "../../web/js/panels/text_layer.js";

const read = (path) => readFileSync(new URL(`../../web/${path}`, import.meta.url), "utf8");
const IDENTITY = [1, 0, 0, 1, 0, 0];

test("a pointer that moves 4 px or more between the press and the release is a drag; less is a click", () => {
  assert.equal(isDrag(10, 10, 10, 10), false);
  assert.equal(isDrag(10, 10, 12, 12), false);
  assert.equal(isDrag(10, 10, 14, 10), true);
  assert.equal(isDrag(10, 10, 10, 40), true);
  assert.equal(isDrag(0, 0, 3, 3, 5), false);
  assert.equal(isDrag(0, 0, 3, 4, 5), true);
});

test("the label of a control is its text with the spaces folded and cut", () => {
  assert.equal(labelOf({ textContent: "  p95   auth \n" }), "p95 auth");
  assert.equal(labelOf({ textContent: "" }), "");
  assert.equal(labelOf({}), "");
});

test("the selectable controls are the legend items, the sort buttons and the links of the cells", () => {
  for (const name of [".chartCore__legendItem", ".dataTable__sort", ".cell-link"]) assert.ok(SELECTABLE.includes(name), name);
});

test("the size of a canvas font is read in pixels and scaled with the font kept", () => {
  assert.equal(parseFont("500 11px Inter, sans-serif"), 11);
  assert.equal(parseFont("italic 12.5px/1.2 serif"), 12.5);
  assert.equal(parseFont("1em serif"), null);
  assert.equal(scaledFont("500 11px Inter, sans-serif", 2), "500 22px Inter, sans-serif");
  assert.equal(scaledFont("12px serif", 0.5), "6px serif");
  assert.equal(scaledFont("serif", 3), "serif");
});

test("a text is placed at the point of the transform divided by the pixel ratio, with the alignment and the baseline", () => {
  const base = { x: 10, y: 20, m: IDENTITY, ratio: 1, width: 40, ascent: 10, descent: 3, size: 13, align: "left", baseline: "alphabetic" };
  const left = placeText(base);
  assert.deepEqual([left.left, left.top, left.angle, left.dx, left.dy, left.w, left.h], [10, 20, 0, 0, -10, 40, 13]);
  assert.equal(placeText({ ...base, align: "center" }).dx, -20);
  assert.equal(placeText({ ...base, align: "right" }).dx, -40);
  assert.equal(placeText({ ...base, align: "end" }).dx, -40);
  assert.equal(placeText({ ...base, baseline: "middle" }).dy, -6.5);
  assert.equal(placeText({ ...base, baseline: "top" }).dy, 0);
  assert.equal(placeText({ ...base, baseline: "bottom" }).dy, -13);
  // A pixel ratio of 2: the canvas has twice the pixels of its CSS size, and the transform scales by 2.
  const dpr = placeText({ ...base, m: [2, 0, 0, 2, 0, 0], ratio: 2 });
  assert.deepEqual([dpr.left, dpr.top, dpr.scale, dpr.w], [10, 20, 1, 40]);
  // A turn of 90 degrees (the title of a vertical axis): the angle follows the transform, and the origin is the turned point.
  const turned = placeText({ ...base, x: 0, y: 0, m: [0, 1, -1, 0, 100, 50], ratio: 1 });
  assert.equal(turned.angle, Math.PI / 2);
  assert.deepEqual([turned.left, turned.top], [100, 50]);
  // The metrics of the font are missing: a fallback from the size keeps a box.
  const fallback = placeText({ ...base, ascent: undefined, descent: undefined });
  assert.ok(fallback.h > 0 && fallback.dy < 0);
});

test("the reading order is the top row first and, in a row, from left to right", () => {
  const order = readingOrder([
    { text: "c", top: 100, left: 5 },
    { text: "b", top: 10.5, left: 80 },
    { text: "a", top: 10, left: 20 },
    { text: "d", top: 100, left: 50 },
  ]);
  assert.deepEqual(order.map((i) => i.text), ["a", "b", "c", "d"]);
});

test("the signature changes with a text, a place, an angle, a font or the size of the canvas, and with nothing else", () => {
  const item = { text: "100 ms", left: 5, top: 7, angle: 0, font: "11px serif" };
  const sig = signature([item], 300, 100);
  assert.equal(signature([{ ...item }], 300, 100), sig);
  assert.notEqual(signature([{ ...item, text: "200 ms" }], 300, 100), sig);
  assert.notEqual(signature([{ ...item, left: 6 }], 300, 100), sig);
  assert.notEqual(signature([{ ...item, top: 9 }], 300, 100), sig);
  assert.notEqual(signature([{ ...item, angle: 1 }], 300, 100), sig);
  assert.notEqual(signature([{ ...item, font: "12px serif" }], 300, 100), sig);
  assert.notEqual(signature([item], 301, 100), sig);
  // A change under a tenth of a pixel is not a change.
  assert.equal(signature([{ ...item, left: 5.01 }], 300, 100), sig);
});

test("the texts of a draw are cut to the cap in reading order and carry a scaled font", () => {
  const records = [];
  for (let i = 0; i < 1000; i++) records.push({ text: `t${i}`, x: i, y: 1000 - i, font: "10px serif", align: "left", baseline: "alphabetic", m: [2, 0, 0, 2, 0, 0], width: 20, asc: 8, desc: 2 });
  const items = textItems(records, 2);
  assert.equal(items.length, MAX_TEXTS);
  assert.equal(MAX_TEXTS, 400);
  // The top rows come first: the records with the smallest y, which are the last records.
  assert.equal(items[0].text, "t999");
  assert.equal(items[0].font, "10px serif");
  // The rows of the cap are the 400 highest rows.
  assert.ok(items.every((i) => i.top <= items[MAX_TEXTS - 1].top + 4));
});

// A fake 2D context class: it records the calls that reach the real methods.
function fakeWorld({ host = () => ({ name: "host" }) } = {}) {
  const calls = [];
  class Context {
    constructor(canvas) {
      this.canvas = canvas;
      this.font = "11px serif";
      this.textAlign = "left";
      this.textBaseline = "alphabetic";
      this.matrix = { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 };
    }
    getTransform() { return this.matrix; }
    measureText(text) { return { width: String(text).length * 6, fontBoundingBoxAscent: 9, fontBoundingBoxDescent: 3 }; }
    fillText(text, x, y) { calls.push(["fillText", text, x, y]); }
    clearRect(x, y, w, h) { calls.push(["clearRect", x, y, w, h]); }
  }
  const queue = [];
  const synced = [];
  const canvas = { isConnected: true, clientWidth: 200, clientHeight: 100, width: 200, height: 100 };
  const sync = (c, h, items) => { synced.push(items.map((i) => i.text)); c.__huneTextLayer = { isConnected: true }; };
  const win = { CanvasRenderingContext2D: Context };
  const installed = installTextLayer({ win, schedule: (fn) => queue.push(fn), host, sync });
  const flush = () => { while (queue.length) queue.shift()(); };
  return { Context, canvas, calls, queue, synced, installed, flush, win, sync };
}

test("the wrapper of fillText records the texts of one draw and gives them to the layer once, at the end of the draw", () => {
  const w = fakeWorld();
  assert.equal(w.installed, true);
  const ctx = new w.Context(w.canvas);
  ctx.fillText("100 ms", 10, 20);
  ctx.fillText("200 ms", 10, 50);
  ctx.fillText("   ", 10, 60);
  assert.equal(w.queue.length, 1, "one end of draw for the three calls");
  assert.equal(w.calls.filter((c) => c[0] === "fillText").length, 3, "the real fillText ran for each call, blank text too");
  w.flush();
  assert.deepEqual(w.synced, [["100 ms", "200 ms"]], "the blank text is not in the layer");
  // The same draw again: the signature is the same and the layer stays as it is.
  ctx.fillText("100 ms", 10, 20);
  ctx.fillText("200 ms", 10, 50);
  w.flush();
  assert.equal(w.synced.length, 1);
  // A draw with another text changes the layer.
  ctx.fillText("300 ms", 10, 20);
  w.flush();
  assert.deepEqual(w.synced[1], ["300 ms"]);
});

test("a clear of the whole canvas starts the list again, and a clear of a part does not", () => {
  const w = fakeWorld();
  const ctx = new w.Context(w.canvas);
  ctx.fillText("old", 5, 10);
  ctx.clearRect(0, 0, 200, 100);
  ctx.fillText("new", 5, 10);
  w.flush();
  assert.deepEqual(w.synced, [["new"]]);
  ctx.fillText("a", 5, 10);
  ctx.clearRect(0, 0, 20, 10);
  ctx.fillText("b", 5, 40);
  w.flush();
  assert.deepEqual(w.synced[1], ["a", "b"]);
  assert.deepEqual(w.calls.filter((c) => c[0] === "clearRect").length, 2, "the real clearRect ran each time");
});

test("a canvas that gets no layer records nothing more, and a canvas that is not in the page is not given to the layer", () => {
  const none = fakeWorld({ host: () => null });
  const a = new none.Context(none.canvas);
  a.fillText("x", 1, 1);
  none.flush();
  a.fillText("y", 1, 1);
  assert.equal(none.queue.length, 0, "no new end of draw after the host is refused");
  assert.deepEqual(none.synced, []);
  assert.equal(none.calls.filter((c) => c[0] === "fillText").length, 2, "the drawing is not touched");

  const away = fakeWorld();
  away.canvas.isConnected = false;
  const b = new away.Context(away.canvas);
  b.fillText("x", 1, 1);
  away.flush();
  assert.deepEqual(away.synced, []);
});

test("a fault in the recording never stops the drawing, and the install is made one time", () => {
  const w = fakeWorld();
  const ctx = new w.Context(w.canvas);
  ctx.getTransform = () => { throw new Error("no transform"); };
  ctx.fillText("still drawn", 1, 2);
  assert.deepEqual(w.calls.at(-1), ["fillText", "still drawn", 1, 2]);
  assert.equal(installTextLayer({ win: w.win, schedule: () => {}, host: () => null, sync: () => {} }), false, "a second install does nothing");
  assert.equal(installTextLayer({ win: {} }), false, "a page with no 2D context does nothing");
});

test("the records stop at twice the cap in one draw, so a chart with thousands of labels stays fast", () => {
  const w = fakeWorld();
  const ctx = new w.Context(w.canvas);
  for (let i = 0; i < 5000; i++) ctx.fillText(`t${i}`, i % 200, Math.floor(i / 200));
  w.flush();
  assert.equal(w.synced[0].length, MAX_TEXTS);
  assert.equal(w.calls.filter((c) => c[0] === "fillText").length, 5000);
});

// A fake document for the text of a copy.
function fakeSelection({ spans, from, to, anchorInLayer = true, focusInLayer = true }) {
  const layer = { children: spans, closest: (sel) => (sel === ".textLayer" ? layer : null), nodeType: 1 };
  const outside = { closest: () => null, nodeType: 1 };
  const nodes = spans.map((s) => s.firstChild);
  const range = {
    startContainer: from.node, startOffset: from.offset, endContainer: to.node, endOffset: to.offset,
    intersectsNode: (n) => {
      if (n === layer) return true;
      const at = spans.indexOf(n);
      const lo = nodes.indexOf(from.node);
      const hi = nodes.indexOf(to.node);
      return at >= lo && at <= hi;
    },
  };
  const selection = { isCollapsed: false, rangeCount: 1, getRangeAt: () => range, anchorNode: anchorInLayer ? layer : outside, focusNode: focusInLayer ? layer : outside };
  return { getSelection: () => selection, querySelectorAll: (sel) => (sel === ".textLayer" ? [layer] : []) };
}

test("a copy of a selection inside the layers gives the selected part of each text, one on each line", () => {
  const spans = ["100 ms", "200 ms", "300 ms"].map((data) => ({ firstChild: { data } }));
  const all = fakeSelection({ spans, from: { node: spans[0].firstChild, offset: 0 }, to: { node: spans[2].firstChild, offset: 6 } });
  assert.equal(selectedLayerText(all), "100 ms\n200 ms\n300 ms");
  const part = fakeSelection({ spans, from: { node: spans[0].firstChild, offset: 2 }, to: { node: spans[1].firstChild, offset: 3 } });
  assert.equal(selectedLayerText(part), "0 ms\n200");
  const outside = fakeSelection({ spans, from: { node: spans[0].firstChild, offset: 0 }, to: { node: spans[1].firstChild, offset: 6 }, focusInLayer: false });
  assert.equal(selectedLayerText(outside), null, "a selection that leaves the layers uses the copy of the browser");
  const empty = fakeSelection({ spans, from: { node: spans[0].firstChild, offset: 0 }, to: { node: spans[0].firstChild, offset: 0 } });
  empty.getSelection().isCollapsed = true;
  assert.equal(selectedLayerText(empty), null);
});

test("the page loads the two modules and the stylesheet, and the stylesheet lets the controls show a selection", () => {
  assert.match(read("js/panel_frame.js"), /import \{ installCopyText \} from "\.\/panels\/copy_text\.js";/);
  assert.match(read("js/panel_frame.js"), /import \{ installTextLayer \} from "\.\/panels\/text_layer\.js";/);
  assert.match(read("js/panel_frame.js"), /^installCopyText\(\);\ninstallTextLayer\(\);$/m);
  assert.match(read("shell.html"), /css\/panel-select\.css/);
  const css = read("css/panel-select.css");
  assert.match(css, /\.chartCore__legendItem[^{]*\{\s*user-select: text/);
  assert.match(css, /a\.cell-link \{ -webkit-user-drag: none; \}/);
  assert.match(css, /\.textLayer > span \{[^}]*color: transparent/);
  assert.match(css, /\.textLayer \{ position: absolute; overflow: hidden; pointer-events: none; \}/);
  assert.match(css, /\.textLayer > span::selection \{ background: color-mix\(in srgb, var\(--accent-fill\) 35%, transparent\); color: transparent; \}/);
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgba?\(/.test(css), "no colour literal in the rules");
});

test("the chart engine of the vendor copy is not changed by this feature", () => {
  const vendor = read("js/vendor/chdash/app_chart_core.js");
  assert.ok(!/textLayer|text_layer|copy_text/.test(vendor));
});
