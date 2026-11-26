// SPDX-License-Identifier: MIT OR Apache-2.0
// The fill, the line, the orientation `auto` and the grid lines of a bar chart (the later options, item P03; 4.18, 4.30, 9.19, E34 of the copied
// chart engine). The panel and the engine run here on a small fake of the DOM and of the canvas. The fake context records each fill and
// each stroke with its style, its line width, the rects and the line segments of its path. The expected values are those of Grafana
// 12.4.2, measured on 2026-11-22 with a probe of the canvas calls on 16 bar charts in a temporary dashboard of the folder `hune-parity`:
//   fillOpacity 61: fill rgba(255, 0, 0, 0.61), line #ff0000 width 1; fillOpacity 100, lineWidth 0: fill #ff0000, no stroke;
//   fillOpacity 0, lineWidth 4: no fill, line width 4; no member: fill alpha 0.8, line width 1.
//   The box of one bar: [22, 17, 352, 190] at lineWidth 0, [24, 19, 348, 186] at lineWidth 4 (half the line inside each side).
//   axisGridShow false: no grid line, the ticks stay. orientation auto: horizontal bars in a panel of 252 x 448 px (also with no
//   orientation member), vertical bars in panels of 773 x 220, 382 x 372, 447 x 372 and 512 x 372 px (the rule `width < height`).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const WIDTH = 600;
const HEIGHT = 200;

let ops = []; // each fill and stroke of the charts since the last reset: { k, style, width, rects, segs }

function fakeContext() {
  const state = {};
  let path = { rects: [], segs: [] };
  const methods = {
    measureText: (s) => ({ width: String(s).length * 6.5 }),
    beginPath: () => { path = { rects: [], segs: [] }; },
    rect: (x, y, w, h) => path.rects.push([x, y, w, h]),
    moveTo: (x, y) => path.segs.push(["M", x, y]),
    lineTo: (x, y) => path.segs.push(["L", x, y]),
    fill: () => ops.push({ k: "fill", style: String(state.fillStyle), rects: path.rects.slice() }),
    stroke: () => ops.push({ k: "stroke", style: String(state.strokeStyle), width: state.lineWidth, rects: path.rects.slice(), segs: path.segs.slice() }),
  };
  return new Proxy(state, {
    get: (t, k) => (k in methods ? methods[k] : k in t ? t[k] : () => undefined),
    set: (t, k, v) => { t[k] = v; return true; },
  });
}

globalThis.Node = class {};

// An element that keeps its class, its style properties and its children, so that a test can find the bars of the DOM chart.
function fakeElement() {
  const kids = new Map();
  const props = {};
  const own = Object.assign(Object.create(globalThis.Node.prototype), {
    className: "", children: [], dataset: {}, hidden: false, parentNode: null,
    style: { props, setProperty(k, v) { props[k] = v; } },
    classList: { add() {}, remove() {}, contains: () => false, toggle() {} },
  });
  const self = new Proxy(own, {
    get(t, k) {
      if (k in t) return t[k];
      if (typeof k === "symbol") return undefined;
      if (k === "append" || k === "appendChild") return (...list) => { for (const c of list) { if (c && typeof c === "object") c.parentNode = self; t.children.push(c); } return list[0]; };
      if (k === "replaceChildren") return (...list) => { t.children = list; };
      if (k === "remove") return () => { const p = t.parentNode; if (p) p.children.splice(p.children.indexOf(self), 1); };
      if (k === "getContext") return () => (t.ctx ??= fakeContext());
      if (k === "getBoundingClientRect") return () => ({ width: WIDTH, height: HEIGHT, left: 0, top: 0, right: WIDTH, bottom: HEIGHT });
      if (k === "querySelector") return (sel) => (kids.has(sel) ? kids.get(sel) : kids.set(sel, fakeElement()).get(sel));
      if (k === "querySelectorAll") return () => [];
      if (/^(offset|client)(Width|Height)$/.test(k)) return k.endsWith("Width") ? WIDTH : HEIGHT + 28;
      return () => undefined;
    },
    set(t, k, v) { t[k] = v; return true; },
  });
  return self;
}

const observers = []; // each ResizeObserver: { cb, targets, gone }
globalThis.document = { createElement: () => fakeElement(), createTextNode: (s) => ({ text: s }), documentElement: fakeElement(), addEventListener() {}, removeEventListener() {}, fonts: null };
globalThis.window = { devicePixelRatio: 1, matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }), getSelection: () => null };
globalThis.MutationObserver = class { observe() {} disconnect() {} };
globalThis.ResizeObserver = class {
  constructor(cb) { this.cb = cb; this.targets = []; this.gone = false; observers.push(this); }
  observe(el) { this.targets.push(el); }
  disconnect() { this.gone = true; }
};
globalThis.getComputedStyle = () => ({ color: "rgb(204, 204, 220)", fontFamily: "Inter", fontSize: "12px", fontWeight: "400", getPropertyValue: () => "" });
globalThis.requestAnimationFrame = () => 0;

const { chartCore } = await import("../../web/js/vendor/chdash/app_chart_core.js");
const { render, autoHorizontal } = await import("../../web/js/panels/barchart.js");

const field = (name, kind, values, set = {}) => ({ name, kind, values, nulls: new Uint8Array(values.length), config: { unit: "none", thresholds: { mode: "absolute", steps: [] }, set } });
const OPTIONS = { orientation: "vertical", x_field: null, stacking: "none", show_values: "never", bar_width: 97, group_width: 70, x_tick_rotation: 0, legend: { show: false, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } };

// Draws the panel of two bars (a = 10, b = 5, as the probe of Grafana). Gives the engine calls, the plot [left, top, w, h], the engine
// charts that the panel made, and the host. `size` is [clientWidth, clientHeight] of the host.
function draw(options, { set = {}, size = [WIDTH, HEIGHT + 28] } = {}) {
  ops = [];
  const made = [];
  const create = chartCore.create;
  chartCore.create = (h, o) => { const c = create(h, o); made.push(c); return c; };
  const host = Object.assign(fakeElement(), { clientWidth: size[0], clientHeight: size[1] });
  const fields = [field("k", "text", ["a", "b"]), field("v", "number", Float64Array.of(10, 5), set)];
  let handle;
  try {
    handle = render(host, { panel: { id: 1, title: "t", type: "barchart", options: { ...OPTIONS, ...options } }, fields, names: ["k", "v"], zone: undefined });
  } finally {
    chartCore.create = create;
  }
  for (const c of made) c.flush();
  const plot = made.length ? made[0].root.dataset.plot.split(" ").map(Number) : null;
  return { ops: ops.slice(), plot, made, host, handle };
}

// The fills and the strokes of the two bars (a path of two rects about 270 px wide), and the rgb and alpha of a canvas colour.
const barOps = (list, k) => list.filter((o) => o.k === k && o.rects.length === 2 && o.rects.every((r) => r[2] > 100));
const rgba = (css) => {
  const m = /^rgba\((\d+), (\d+), (\d+), ([\d.]+)\)$/.exec(css);
  assert.ok(m, `${css} is an rgba colour`);
  return { rgb: m.slice(1, 4).join(","), a: Number(m[4]) };
};
const near = (a, b, eps = 1e-6) => assert.ok(Math.abs(a - b) <= eps, `${a} is not ${b}`);

test("P03: the fill has the alpha fill_opacity / 100 and the line has the plain colour at line_width, as Grafana 12.4.2", () => {
  for (const [fill, line] of [[61, 1], [61, 3], [100, 1], [10, 10], [80, 2]]) {
    const { ops: list } = draw({ fill_opacity: fill, line_width: line });
    const fills = barOps(list, "fill");
    const strokes = barOps(list, "stroke");
    assert.equal(fills.length, 1, `${fill}/${line}: one fill of the bars`);
    assert.equal(strokes.length, 1, `${fill}/${line}: one stroke of the bars`);
    const f = rgba(fills[0].style);
    const s = rgba(strokes[0].style);
    near(f.a, fill / 100);
    near(s.a, 1);
    assert.equal(f.rgb, s.rgb, "the fill and the line have one colour");
    assert.equal(strokes[0].width, line);
  }
});

test("P03: with no member, the bars have the Grafana defaults: fill alpha 0.8 and a line of 1 px", () => {
  const { ops: list } = draw({});
  near(rgba(barOps(list, "fill")[0].style).a, 0.8);
  assert.equal(barOps(list, "stroke")[0].width, 1);
});

test("P03: fill_opacity 0 gives no fill call and line_width 0 gives no stroke call, as Grafana (E34)", () => {
  const noFill = draw({ fill_opacity: 0, line_width: 4 }).ops;
  assert.equal(barOps(noFill, "fill").length, 0, "no fill at the opacity 0");
  assert.equal(barOps(noFill, "stroke").length, 1);
  assert.equal(barOps(noFill, "stroke")[0].width, 4);
  const noLine = draw({ fill_opacity: 100, line_width: 0 }).ops;
  assert.equal(barOps(noLine, "stroke").length, 0, "no stroke at the width 0");
  near(rgba(barOps(noLine, "fill")[0].style).a, 1);
});

test("P03: the line stays inside the bar on its four sides: each side of the box moves in by half the line width (Grafana: 22, 17, 352, 190 at 0; 24, 19, 348, 186 at 4)", () => {
  const flat = barOps(draw({ fill_opacity: 100, line_width: 0 }).ops, "fill")[0].rects;
  for (const w of [1, 4, 10]) {
    const boxes = barOps(draw({ fill_opacity: 100, line_width: w }).ops, "fill")[0].rects;
    boxes.forEach((b, i) => {
      const [x, y, bw, bh] = flat[i];
      near(b[0], x + w / 2);
      near(b[1], y + w / 2);
      near(b[2], bw - w);
      near(b[3], bh - w);
    });
  }
});

test("P03: axis_grid false removes the grid lines of the value axis; true and an absent member keep them (Grafana: Off, On, Auto)", () => {
  // A grid line runs from the left of the plot to its right at one y.
  const gridLines = ({ ops: list, plot }) => list.filter((o) => o.k === "stroke").flatMap((o) => o.segs.filter((s, i) => s[0] === "M" && o.segs[i + 1] && Math.abs(s[1] - plot[0]) < 1 && Math.abs(o.segs[i + 1][1] - (plot[0] + plot[2])) < 1 && o.segs[i + 1][2] === s[2]));
  const auto = gridLines(draw({}));
  const on = gridLines(draw({}, { set: { axis_grid: true } }));
  const off = gridLines(draw({}, { set: { axis_grid: false } }));
  assert.ok(auto.length >= 2, `${auto.length} grid lines with no member`);
  assert.equal(on.length, auto.length);
  assert.equal(off.length, 0);
});

test("P03: the rule of the orientation auto is the rule of Grafana 12.4.2, width < height", () => {
  assert.equal(autoHorizontal(252, 448), true);
  for (const [w, h] of [[773, 220], [382, 372], [447, 372], [512, 372], [300, 300]]) assert.equal(autoHorizontal(w, h), false, `${w} x ${h}`);
});

// The DOM chart of horizontal bars in the host, or undefined.
const hbarOf = (host) => host.children.find((c) => c && c.className === "hbar");
const all = (node, cls, out = []) => {
  for (const c of (node && node.children) || []) {
    if (c && c.className === cls) out.push(c);
    all(c, cls, out);
  }
  return out;
};

test("P03: orientation auto draws vertical bars in a wide host and horizontal bars in a narrow host", () => {
  const wide = draw({ orientation: "auto" }, { size: [512, 372] });
  assert.equal(wide.made.length, 1, "the engine draws the vertical bars");
  assert.equal(hbarOf(wide.host), undefined);
  wide.handle.destroy();
  const narrow = draw({ orientation: "auto" }, { size: [252, 448] });
  assert.equal(narrow.made.length, 0, "the engine draws nothing");
  assert.ok(hbarOf(narrow.host), "the DOM chart of horizontal bars");
  narrow.handle.destroy();
  // `vertical` and `horizontal` do not look at the size.
  assert.equal(draw({ orientation: "vertical" }, { size: [252, 448] }).made.length, 1);
  assert.equal(draw({ orientation: "horizontal" }, { size: [512, 372] }).made.length, 0);
});

test("P03: a resize that crosses the rule draws the chart again in the other orientation, and destroy stops the observer", () => {
  observers.length = 0;
  const { host, made, handle } = draw({ orientation: "auto" }, { size: [512, 372] });
  assert.equal(made.length, 1);
  const chartRoot = made[0].root;
  assert.ok(host.children.includes(chartRoot));
  const fire = () => { for (const o of observers.slice()) if (!o.gone && o.targets.includes(host)) o.cb([]); };
  // The same side of the rule: no new chart.
  host.clientWidth = 400;
  fire();
  assert.ok(host.children.includes(chartRoot) && !hbarOf(host));
  // Narrower than high: the vertical chart leaves and the horizontal bars come.
  host.clientWidth = 200;
  fire();
  assert.ok(!host.children.includes(chartRoot), "the engine chart left the host");
  assert.ok(hbarOf(host), "the horizontal bars are in the host");
  // Wider again: back to the engine.
  const before = observers.length;
  const create = chartCore.create;
  let again = 0;
  chartCore.create = (h, o) => { again++; return create(h, o); };
  try {
    host.clientWidth = 600;
    fire();
  } finally {
    chartCore.create = create;
  }
  assert.equal(again, 1);
  assert.ok(!hbarOf(host));
  assert.ok(observers.length > before);
  handle.destroy();
  assert.ok(observers.every((o) => o.gone || !o.targets.includes(host)), "no observer of the host stays after destroy");
});

test("P03: the horizontal bars read fill_opacity, line_width and axis_grid", () => {
  const css = readFileSync(new URL("../../web/css/panel-chart.css", import.meta.url), "utf8");
  assert.match(css, /\.hbar__bar \{[^}]*border: var\(--lw, 1px\) solid var\(--c\);[^}]*color-mix\(in srgb, var\(--c\) var\(--fa, 80%\), transparent\)/);
  assert.match(css, /\.hbar__bar \{[^}]*box-sizing: border-box;/);
  const set = draw({ orientation: "horizontal", fill_opacity: 61, line_width: 3 });
  const root = hbarOf(set.host);
  assert.equal(root.style.props["--fa"], "61%");
  assert.equal(root.style.props["--lw"], "3px");
  const defaults = hbarOf(draw({ orientation: "horizontal" }).host);
  assert.equal(defaults.style.props["--fa"], "80%");
  assert.equal(defaults.style.props["--lw"], "1px");
  // Grid off: no grid line, and the ticks of the value axis stay.
  const on = hbarOf(draw({ orientation: "horizontal" }).host);
  const off = hbarOf(draw({ orientation: "horizontal" }, { set: { axis_grid: false } }).host);
  assert.ok(all(on, "hbar__line").length >= 2);
  assert.equal(all(off, "hbar__line").length, 0);
  assert.equal(all(off, "hbar__tick").length, all(on, "hbar__tick").length);
});

test("9.19: the value label of a horizontal bar is 10 px at least, as in the vertical bars", () => {
  // 60 rows in a host of 228 px: a bar is 3.7 px thick, so two thirds of it is far below the least size.
  const n = 60;
  const keys = Array.from({ length: n }, (_, i) => `k${i}`);
  const values = Float64Array.from(keys.map((_, i) => i + 1));
  const host = Object.assign(fakeElement(), { clientWidth: WIDTH, clientHeight: HEIGHT + 28 });
  const fields = [field("k", "text", keys), field("v", "number", values)];
  const handle = render(host, { panel: { id: 1, title: "t", type: "barchart", options: { ...OPTIONS, orientation: "horizontal", show_values: "always" } }, fields, names: ["k", "v"], zone: undefined });
  const labels = all(hbarOf(host), "hbar__value").filter((e) => !e.hidden);
  assert.equal(labels.length, n, "a value for each row");
  for (const e of labels) assert.equal(e.style.fontSize, "10px");
  handle.destroy();
});

test("E34: barInset true (E26) and no option keep their boxes: only the value \"box\" moves the sides", () => {
  const rectsOf = (extra) => {
    ops = [];
    const chart = chartCore.create(fakeElement(), {
      height: HEIGHT, xKind: "category", xs: Float64Array.of(0, 1), categories: ["a", "b"], type: "bar", xGrid: false, barMaxWidth: 1e5,
      series: [{ id: "a", label: "A", color: "#f00", values: Float64Array.of(10, 5), nulls: null, group: "all" }],
      barLineWidth: 4, ...extra,
    });
    chart.flush();
    chart.destroy();
    return barOps(ops, "fill")[0].rects;
  };
  const plain = rectsOf({});
  const ends = rectsOf({ barInset: true });
  const box = rectsOf({ barInset: "box" });
  ends.forEach((r, i) => {
    near(r[0], plain[i][0]);
    near(r[2], plain[i][2]);
    near(r[1], plain[i][1] + 2);
    near(box[i][0], plain[i][0] + 2);
    near(box[i][2], plain[i][2] - 4);
    near(box[i][1], r[1]);
    near(box[i][3], r[3]);
  });
});
