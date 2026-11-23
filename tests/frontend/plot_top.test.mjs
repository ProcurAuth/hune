// The top margin of the plot (9.24 edit 2, E27 of the copied chart engine). A y axis that shows on the left or on the right keeps the margin of
// 8 px at the top. With no y axis on either side, the plot starts at the top of the canvas, as in Grafana: a heatmap with `y_axis` = `hidden`
// and a time series whose y axes are all hidden (9.19, 9.19e). The engine runs here on a small fake of the DOM and of the canvas.
import test from "node:test";
import assert from "node:assert/strict";

const WIDTH = 600;
const HEIGHT = 200;

function fakeContext() {
  const state = {};
  const methods = { measureText: (s) => ({ width: String(s).length * 6.5 }) };
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

const xs = Float64Array.from({ length: 20 }, (_, i) => 1794211200000 + i * 60000);
const line = (id, k) => ({ id, label: id, color: "#f00", values: Float64Array.from(xs, (_, i) => k * (10 + i)), nulls: null });

/** The plot `[left, top, w, h]` of a time series with two series: `a` on the left scale and `b` on the right scale when `y2` is given. */
function plotOf(extra) {
  const chart = chartCore.create(fakeElement(), { height: HEIGHT, xKind: "time", xs, type: "line", series: [line("a", 1), line("b", 100)], ...extra });
  chart.flush();
  const plot = chart.root.dataset.plot.split(" ").map(Number);
  chart.destroy();
  return plot;
}

test("E27: a y axis that shows keeps the top margin of 8 px", () => {
  assert.equal(plotOf({})[1], 8);
  assert.equal(plotOf({ scales: { b: "y2" }, y2: { shown: true } })[1], 8);
  // A hidden left axis and a right axis that shows: the right axis keeps the margin.
  assert.equal(plotOf({ yShown: false, scales: { b: "y2" }, y2: { shown: true } })[1], 8);
});

test("E27: with no y axis on either side, the plot starts at the top of the canvas and takes the 8 px", () => {
  const shown = plotOf({});
  const left = plotOf({ yShown: false });
  assert.equal(left[1], 0);
  assert.equal(left[3], shown[3] + 8);
  // Both scales hidden (a time series whose series all have `axis_placement` = `hidden`).
  const both = plotOf({ yShown: false, scales: { b: "y2" }, y2: { shown: false } });
  assert.equal(both[1], 0);
  assert.equal(both[3], shown[3] + 8);
  // The bottom band does not change: the plot ends where it ended.
  assert.equal(both[1] + both[3], shown[1] + shown[3]);
});

test("E27: a heatmap with a hidden value axis has no top margin", () => {
  const cells = { x0: Float64Array.of(xs[0]), x1: Float64Array.of(xs[5]), y0: Float64Array.of(0), y1: Float64Array.of(1), level: Uint16Array.of(1), palette: ["#f00"] };
  const chart = chartCore.create(fakeElement(), { height: HEIGHT, xKind: "time", xs, type: "line", series: [], cells, yShown: false });
  chart.flush();
  const plot = chart.root.dataset.plot.split(" ").map(Number);
  chart.destroy();
  assert.equal(plot[1], 0);
});
