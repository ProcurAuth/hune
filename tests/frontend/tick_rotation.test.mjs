// The rotation of the x tick labels of a bar chart (4.31 `x_tick_rotation`, 9.19, E25 of the copied chart engine, decision ID-K295a).
// The engine runs here on a small fake of the DOM and of the canvas. The fake context follows the transform (save, restore, translate,
// rotate) and records each `fillText` with its place on the canvas and its angle. A test fails when a label is not drawn turned.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const CHAR = 6.5; // the width of one character of the fake `measureText`
const WIDTH = 600; // the width of the plot element
const HEIGHT = 200;
const FONT = 12; // FONT_SIZE of the engine

let texts = []; // each `fillText` of the last chart: { text, x, y, angle, align, baseline }

function fakeContext() {
  const state = { textAlign: "start", textBaseline: "alphabetic" };
  let m = [1, 0, 0, 1, 0, 0];
  const stack = [];
  const mul = (n) => { m = [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3], m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]]; };
  const methods = {
    measureText: (s) => ({ width: String(s).length * CHAR }),
    save: () => stack.push({ m: m.slice(), align: state.textAlign, baseline: state.textBaseline }),
    restore: () => { const s = stack.pop(); m = s.m; state.textAlign = s.align; state.textBaseline = s.baseline; },
    setTransform: (...a) => { m = a; },
    translate: (x, y) => mul([1, 0, 0, 1, x, y]),
    rotate: (a) => mul([Math.cos(a), Math.sin(a), -Math.sin(a), Math.cos(a), 0, 0]),
    fillText: (text, x, y) => texts.push({ text, x: m[0] * x + m[2] * y + m[4], y: m[1] * x + m[3] * y + m[5], angle: Math.atan2(m[1], m[0]), align: state.textAlign, baseline: state.textBaseline }),
  };
  return new Proxy(state, {
    get: (t, k) => (k in methods ? methods[k] : k in t ? t[k] : () => undefined),
    set: (t, k, v) => { t[k] = v; return true; },
  });
}

function fakeElement() {
  const kids = new Map();
  const own = { style: { setProperty() {} }, classList: { add() {}, remove() {}, contains: () => false, toggle() {} }, dataset: {}, children: [], hidden: false };
  return new Proxy(own, {
    get(t, k) {
      if (k in t) return t[k];
      if (typeof k === "symbol") return undefined;
      if (k === "getContext") return () => (t.ctx ??= fakeContext());
      if (k === "getBoundingClientRect") return () => ({ width: WIDTH, height: HEIGHT, left: 0, top: 0, right: WIDTH, bottom: HEIGHT });
      if (k === "querySelector") return (sel) => (kids.has(sel) ? kids.get(sel) : kids.set(sel, fakeElement()).get(sel));
      if (k === "querySelectorAll") return () => [];
      if (/^(offset|client)(Width|Height)$/.test(k)) return k.endsWith("Width") ? WIDTH : HEIGHT + 28;
      return () => undefined;
    },
    set(t, k, v) { t[k] = v; return true; },
  });
}

globalThis.document = { createElement: () => fakeElement(), documentElement: fakeElement(), addEventListener() {}, removeEventListener() {}, fonts: null };
globalThis.window = { devicePixelRatio: 1, matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }), getSelection: () => null };
globalThis.MutationObserver = class { observe() {} disconnect() {} };
globalThis.ResizeObserver = class { observe() {} disconnect() {} };
globalThis.getComputedStyle = () => ({ color: "rgb(204, 204, 220)", fontFamily: "Inter", getPropertyValue: () => "" });
globalThis.requestAnimationFrame = () => 0;

const { chartCore } = await import("../../web/js/vendor/chdash/app_chart_core.js");
const { render } = await import("../../web/js/panels/barchart.js");

const CATEGORIES = ["alpha", "bravo-long-category", "c", "delta", "echo", "foxtrot"];

// Draws a bar chart on the engine and gives the labels of its x axis and the plot: { labels, plot: [left, top, w, h], ticks }.
function draw(rotation, { categories = CATEGORIES, height = HEIGHT, extra = {} } = {}) {
  texts = [];
  const host = fakeElement();
  const chart = chartCore.create(host, {
    height, xKind: "category", xs: Float64Array.from(categories, (_, i) => i), categories, type: "bar", xGrid: false,
    series: [{ id: "a", label: "A", color: "#f00", values: Float64Array.from(categories, (_, i) => i + 1), nulls: null, group: "all" }],
    ...(rotation === undefined ? {} : { xTickRotation: rotation }), ...extra,
  });
  chart.flush();
  const root = chart.root;
  const plot = root.dataset.plot.split(" ").map(Number);
  const labels = texts.filter((t) => categories.some((c) => t.text === c || t.text.endsWith("…")));
  const ticks = JSON.parse(root.dataset.xTicks);
  chart.destroy();
  return { labels, plot, ticks };
}

const deg = (a) => (a * 180) / Math.PI;
const near = (a, b, eps = 1e-6) => assert.ok(Math.abs(a - b) <= eps, `${a} is not ${b}`);

test("rotation 0, or no option, draws the labels as before: no rotation, the top of the text at 9 px under the plot, a band of 21 px", () => {
  const few = ["alpha", "bravo", "c", "delta", "echo", "foxtrot"];
  for (const r of [undefined, 0]) {
    const { labels, plot } = draw(r, { categories: few });
    assert.equal(labels.length, few.length);
    for (const t of labels) {
      assert.equal(t.angle, 0);
      assert.equal(t.align, "center");
      assert.equal(t.baseline, "top");
      near(t.y, plot[1] + plot[3] + 9);
    }
    near(plot[3], HEIGHT - 8 - 21);
  }
});

test("a rotation draws each label turned about its anchor: the angle is the rotation (clockwise on the screen for a positive value), the start of the text is at the tick for a positive angle (Grafana)", () => {
  for (const r of [45, -45, 90, -90, 30]) {
    const { labels, plot } = draw(r);
    assert.ok(labels.length > 0, `rotation ${r}: labels are drawn`);
    for (const t of labels) {
      near(deg(t.angle), r);
      assert.equal(t.align, r > 0 ? "left" : "right", `rotation ${r}`);
      assert.equal(t.baseline, "middle");
      // The anchor is under the plot, and the top of the box of the label is 9 px under it.
      near(t.y, plot[1] + plot[3] + 9 + (FONT / 2) * Math.cos((Math.abs(r) * Math.PI) / 180));
    }
  }
});

test("the bottom margin grows by the height of the tallest turned label, and the plot shrinks by as much", () => {
  const widest = Math.max(...CATEGORIES.map((c) => (c.length > 18 ? 18 : c.length) * CHAR));
  const flat = draw(0).plot[3];
  for (const r of [45, -45, 90, 20]) {
    const s = Math.sin((Math.abs(r) * Math.PI) / 180), c = Math.cos((Math.abs(r) * Math.PI) / 180);
    const tall = Math.max(FONT, widest * s + FONT * c);
    near(draw(r).plot[3], HEIGHT - 8 - 9 - tall);
    assert.ok(draw(r).plot[3] < flat, `rotation ${r}: the plot is lower than at 0`);
  }
  // A turn of 90 degrees takes the width of the text as height: the bottom margin of the picture is about 9 + 18 characters.
  assert.ok(flat - draw(90).plot[3] > 50);
  // The size is the same for +r and -r.
  near(draw(45).plot[3], draw(-45).plot[3]);
});

test("no label is cut at the canvas edges, and the box of each label stays at or under the plot bottom and inside the canvas", () => {
  const many = Array.from({ length: 12 }, (_, i) => `a-long-category-${i}`);
  for (const r of [45, -45, 90, -90, 60, -10]) {
    const { ticks, plot } = draw(r, { categories: many });
    assert.ok(ticks.length > 0);
    for (const [, , left, right] of ticks) {
      assert.ok(left >= 0, `rotation ${r}: left ${left}`);
      assert.ok(right <= WIDTH, `rotation ${r}: right ${right}`);
    }
    // The lowest point of a label: its anchor y plus its box under the anchor; it must fit in the canvas height.
    const { labels } = draw(r, { categories: many });
    const s = Math.sin((Math.abs(r) * Math.PI) / 180), c = Math.cos((Math.abs(r) * Math.PI) / 180);
    for (const t of labels) {
      const w = t.text.length * CHAR;
      assert.ok(t.y + w * s + (FONT / 2) * c <= HEIGHT + 1e-6, `rotation ${r}: ${t.text} reaches ${t.y + w * s + (FONT / 2) * c}`);
      assert.ok(t.y - (FONT / 2) * c >= plot[1] + plot[3], `rotation ${r}: the label starts under the plot`);
    }
  }
});

test("a label that would leave less than 40 px of plot ends with an ellipsis", () => {
  const { labels, plot } = draw(90, { categories: ["a-label-of-eighteen", "b"], height: 100 });
  assert.ok(plot[3] >= 40, `plot height ${plot[3]}`);
  assert.ok(labels.some((t) => t.text.endsWith("…")), "the long label is cut");
  for (const t of labels) assert.ok(t.y + t.text.length * CHAR <= 100 + 1e-6);
});

test("labels are thinned by their turned width: more labels show when they turn", () => {
  const many = Array.from({ length: 60 }, (_, i) => `category ${i}`);
  const flat = draw(0, { categories: many }).labels.length;
  const turned = draw(90, { categories: many }).labels.length;
  const slant = draw(45, { categories: many }).labels.length;
  assert.ok(turned > flat, `90 degrees: ${turned} labels, flat: ${flat}`);
  assert.ok(slant > flat, `45 degrees: ${slant} labels, flat: ${flat}`);
  assert.ok(turned >= slant, "a vertical label takes the least room");
  // Two labels shown never come closer than a line of text apart (their boxes do not overlap).
  const xs = draw(90, { categories: many }).labels.map((t) => t.x);
  for (let i = 1; i < xs.length; i++) assert.ok(xs[i] - xs[i - 1] >= FONT, "two vertical labels are at least one line apart");
});

test("layoutXLabels: 0 is the layout of the engine; a rotation keeps the whole label inside the canvas and moves the anchor when it must", () => {
  const measure = (s) => s.length * CHAR;
  const ticks = [0, 1, 2, 3].map((v) => ({ v, label: "label-" + v }));
  const xOf = (v) => 10 + v * 30;
  const flat = chartCore.layoutXLabels(ticks, xOf, 0, 200, 200, measure, measure);
  assert.deepEqual(chartCore.layoutXLabels(ticks, xOf, 0, 200, 200, measure, measure, 0), flat);
  // Centred on the tick, clamped by w/2 + 1 at the left edge.
  near(flat[0].x, (7 * CHAR) / 2 + 1);
  near(flat[0].left, 1);
  for (const r of [45, -45, 90]) {
    const out = chartCore.layoutXLabels(ticks, xOf, 0, 200, 200, measure, measure, r);
    assert.ok(out.length >= 1);
    for (const item of out) {
      assert.ok(item.left >= 1 - 1e-9 && item.right <= 199 + 1e-9, `rotation ${r}: ${item.left} ${item.right}`);
      assert.ok(item.left <= item.x && item.x <= item.right);
    }
  }
  // -45 degrees (counter-clockwise): the label hangs to the left of its anchor; 45 degrees (clockwise): to the right.
  const ccw = chartCore.layoutXLabels([{ v: 3, label: "abcdefgh" }], xOf, 0, 200, 200, measure, measure, -45)[0];
  const cw = chartCore.layoutXLabels([{ v: 0, label: "abcdefgh" }], xOf, 0, 200, 200, measure, measure, 45)[0];
  assert.ok(ccw.x - ccw.left > ccw.right - ccw.x && cw.right - cw.x > cw.x - cw.left);
});

test("the bar chart passes x_tick_rotation to the engine for a vertical chart, and a horizontal chart ignores it", () => {
  const field = (name, kind, values) => ({ name, kind, values, nulls: new Uint8Array(values.length), config: { unit: "none", thresholds: { mode: "absolute", steps: [] } } });
  const fields = [{ ...field("k", "text", ["a", "b", "c"]), kind: "text" }, field("v", "number", Float64Array.of(1, 2, 3))];
  const made = [];
  const create = chartCore.create;
  chartCore.create = (h, o) => { const c = create(h, o); made.push({ c, o }); return c; };
  const host = Object.assign(fakeElement(), { clientHeight: HEIGHT + 28, clientWidth: WIDTH });
  try {
    for (const [orientation, rotation] of [["vertical", 0], ["vertical", 45], ["vertical", -90]]) {
      texts = [];
      const base = { orientation, x_field: null, stacking: "none", show_values: "never", bar_width: 97, group_width: 70, x_tick_rotation: rotation, legend: { show: false, placement: "bottom" }, tooltip: { mode: "single" } };
      const panel = { id: 1, title: "t", type: "barchart", options: base };
      const handle = render(host, { panel, fields, names: ["k", "v"], zone: undefined });
      const { c, o } = made[made.length - 1];
      assert.equal(o.xTickRotation, rotation);
      c.flush();
      const labels = texts.filter((t) => ["a", "b", "c"].includes(t.text));
      assert.equal(labels.length, 3, `rotation ${rotation}`);
      for (const t of labels) near(deg(t.angle), rotation);
      handle.destroy();
    }
  } finally {
    chartCore.create = create;
  }
  // A horizontal bar chart draws its own bars (9.24) and returns before the engine options are made: it reads no rotation (4.31).
  const source = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../web/js/panels/barchart.js"), "utf8");
  const early = source.indexOf("if (horizontal) {");
  assert.ok(early > 0 && early < source.indexOf("xTickRotation: o.x_tick_rotation"));
  assert.ok(!/x_tick_rotation/.test(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../web/js/panels/barchart_h.js"), "utf8")));
});
