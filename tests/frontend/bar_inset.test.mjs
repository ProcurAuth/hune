// The line of a bar of a histogram stays inside the bar at its top and its bottom (9.19f section 3, E26 of the copied chart engine).
// The engine runs here on a small fake of the DOM and of the canvas. The fake context records each `rect` of a path with the colour of the
// next `fill`. Without the option `barInset`, the engine draws the bars as before.
import test from "node:test";
import assert from "node:assert/strict";

const WIDTH = 600;
const HEIGHT = 200;

let rects = []; // each rect of the last chart: [x, y, w, h]

function fakeContext() {
  const state = {};
  const methods = {
    measureText: (s) => ({ width: String(s).length * 6.5 }),
    rect: (x, y, w, h) => rects.push([x, y, w, h]),
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

// Three buckets of the size 10 with the counts 2, 10 and 6 on an axis from 0 to 10. Gives the rects of the bars and the plot [left, top, w, h].
function draw(extra) {
  rects = [];
  const chart = chartCore.create(fakeElement(), {
    height: HEIGHT, xKind: "number", xs: Float64Array.of(5, 15, 25), xDomain: [0, 30], type: "bar", xGrid: false, barSlot: 10, barWidthRatio: 1, barMaxWidth: 1e5, barOverlap: true,
    series: [{ id: "a", label: "A", color: "#f00", values: Float64Array.of(2, 10, 6), nulls: null, group: "all" }],
    yAxis: () => ({ min: 0, max: 10, step: 5, ticks: [0, 5, 10].map((v) => ({ v, label: String(v) })) }),
    ...extra,
  });
  chart.flush();
  const plot = chart.root.dataset.plot.split(" ").map(Number);
  chart.destroy();
  // The bars are about 150 px wide. The clip of the plot is a rect too.
  return { bars: rects.filter((r) => r[2] > 20 && r[2] < 300), plot };
}

const near = (a, b, eps = 1e-6) => assert.ok(Math.abs(a - b) <= eps, `${a} is not ${b}`);

test("E26: with barInset, a bar loses half the line width at its top and at its bottom; x and width do not change", () => {
  for (const lw of [1, 3, 10]) {
    const flat = draw({ barLineWidth: lw });
    const inset = draw({ barLineWidth: lw, barInset: true });
    assert.equal(flat.bars.length, 3);
    assert.equal(inset.bars.length, 3);
    const [, top, , h] = inset.plot;
    [2, 10, 6].forEach((count, i) => {
      const yTop = top + h - (count / 10) * h;
      // Without the option: the box runs from the count to the axis.
      near(flat.bars[i][1], yTop);
      near(flat.bars[i][3], top + h - yTop);
      // With the option: the line of width lw, drawn on the edge of the box, ends at the count and at the axis.
      near(inset.bars[i][1], yTop + lw / 2);
      near(inset.bars[i][3], Math.max(1, top + h - yTop - lw));
      near(inset.bars[i][0], flat.bars[i][0]);
      near(inset.bars[i][2], flat.bars[i][2]);
    });
  }
});

test("E26: barInset with no line, or no option, gives the boxes of before", () => {
  assert.deepEqual(draw({ barLineWidth: 0, barInset: true }).bars, draw({ barLineWidth: 0 }).bars);
  assert.deepEqual(draw({}).bars, draw({ barInset: false }).bars);
});

test("the histogram panel asks the engine for barInset", async () => {
  const { readFileSync } = await import("node:fs");
  const source = readFileSync(new URL("../../web/js/panels/histogram.js", import.meta.url), "utf8");
  assert.match(source, /barLineWidth: o\.line_width,\s*barInset: true,/);
});
