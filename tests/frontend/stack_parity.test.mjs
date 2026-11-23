// The stacks of the time series, the bar chart and the histogram as Grafana 12.4.2 draws them (9.19, 9.19f, decision ID-K325 point 3, engine
// edit E33). The expected values are those that the worker stack-parity measured on Grafana 12.4.2 on 2026-11-22: 24 time series panels, 54 bar
// chart panels and 24 histogram panels (temporary dashboards in the folder hune-parity), the tick labels and the drawn marks. The engine runs here
// on a small fake of the DOM and of the canvas, which records the lines and the bars.
import test from "node:test";
import assert from "node:assert/strict";
import { tickLabel } from "../../web/js/panels/series.js";
import { scaleAxis } from "../../web/js/panels/timeseries.js";
import { barValueAxis } from "../../web/js/panels/barchart.js";
import { histAxis } from "../../web/js/panels/histogram.js";
import { yTickLabeller } from "../../web/js/panels/common.js";

// ---- The engine on a fake DOM that records the lines and the rectangles of the bars ----

const WIDTH = 600;
let strokes = [];
let fills = [];

function fakeContext() {
  const state = { path: [], rects: [] };
  const methods = {
    measureText: (s) => ({ width: 6.5 * String(s).length }),
    beginPath: () => { state.path = []; state.rects = []; },
    moveTo: (x, y) => { state.path.push([0, x, y]); },
    lineTo: (x, y) => { state.path.push([1, x, y]); },
    rect: (x, y, w, h) => { state.rects.push([x, y, w, h]); },
    stroke: () => { strokes.push({ color: state.strokeStyle, path: state.path.slice() }); },
    fill: () => { fills.push({ color: state.fillStyle, rects: state.rects.slice() }); },
    createLinearGradient: () => ({ addColorStop() {} }),
  };
  return new Proxy(state, {
    get: (t, k) => (k in methods ? methods[k] : k in t ? t[k] : () => undefined),
    set: (t, k, v) => { t[k] = v; return true; },
  });
}

function fakeElement(height) {
  const kids = new Map();
  const own = { style: { setProperty() {} }, classList: { add() {}, remove() {}, contains: () => false, toggle() {} }, dataset: {}, children: [], hidden: false };
  return new Proxy(own, {
    get(t, k) {
      if (k in t) return t[k];
      if (typeof k === "symbol") return undefined;
      if (k === "getContext") return () => (t.ctx ??= fakeContext());
      if (k === "getBoundingClientRect") return () => ({ width: WIDTH, height, left: 0, top: 0, right: WIDTH, bottom: height });
      if (k === "querySelector") return (sel) => (kids.has(sel) ? kids.get(sel) : kids.set(sel, fakeElement(height)).get(sel));
      if (k === "querySelectorAll") return () => [];
      if (/^(offset|client)(Width|Height)$/.test(k)) return k.endsWith("Width") ? WIDTH : height + 28;
      return () => undefined;
    },
    set(t, k, v) { t[k] = v; return true; },
  });
}

globalThis.document = { createElement: () => fakeElement(400), documentElement: fakeElement(400), addEventListener() {}, removeEventListener() {}, fonts: null };
globalThis.window = { devicePixelRatio: 1, matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }), getSelection: () => null };
globalThis.MutationObserver = class { observe() {} disconnect() {} };
globalThis.ResizeObserver = class { observe() {} disconnect() {} };
// A colour in hex (the colour of a series) stays as it is, so the test finds the marks of each series by its colour.
globalThis.getComputedStyle = (el) => ({ color: /^#/.test(el?.style?.color ?? "") ? el.style.color : "rgb(204, 204, 220)", fontFamily: "Inter", getPropertyValue: () => "" });
globalThis.requestAnimationFrame = () => 0;

const { chartCore } = await import("../../web/js/vendor/chdash/app_chart_core.js");

const COLORS = ["#ff0000", "#0000ff", "#00aa00"];
const HEIGHT = 400;
const rgbOf = (c) => new RegExp(`rgba?\\(${[1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16)).join(",\\s*")}`);

/** Draw once and read back, for each series, the values of its line and the [bottom, top] values of its bars (by x). */
function draw(options) {
  let raw = null;
  const hook = options.yAxis;
  strokes = [];
  fills = [];
  let info = null;
  const chart = chartCore.create(fakeElement(HEIGHT), { height: HEIGHT, ...options, yAxis: (a, b, h, r) => { raw = r; return hook(a, b, h, r); }, onDraw: (i) => { info = i; } });
  chart.flush();
  const [, top, , plotH] = chart.root.dataset.plot.split(" ").map(Number);
  chart.destroy();
  const valueOf = (y) => info.yMax - ((y - top) / plotH) * (info.yMax - info.yMin);
  const marks = options.series.map((s) => {
    const re = rgbOf(s.color);
    const line = strokes.filter((o) => re.test(String(o.color)) && o.path.length > 2).at(-1);
    const bars = fills.filter((o) => re.test(String(o.color)) && o.rects.length).at(-1);
    return {
      line: line ? line.path.map((p) => valueOf(p[2])) : null,
      bars: bars ? bars.rects.slice().sort((a, b) => a[0] - b[0]).map(([, y, , h]) => [valueOf(y + h), valueOf(y)]) : null,
    };
  });
  return { raw, marks, px: (info.yMax - info.yMin) / plotH };
}

const close = (got, want, tol, what) => assert.ok(Math.abs(got - want) <= tol, `${what}: ${got} and Grafana ${want} (tolerance ${tol})`);
const AT = [0, 10, 20, 30, 40, 50, 60];

// ---- (a) A time series: a line, bars and points stack in groups of their own (E33) ----

const N = 61;
const TS = {
  A: (n) => Math.sin(n / 6) * 45 + 50,
  B: (n) => Math.cos(n / 6) * 30 + 40,
  C: (n) => Math.sin(n / 4) * 20 + 30,
  NEGB: (n) => -(Math.cos(n / 6) * 30 + 40),
};
// The cases: the series with the `type` that the time series gives a series of another draw (timeseries.js), the chart type, percent.
const TS_CASES = {
  "line bars": [["A"], ["B", "bar"], "area"],
  "bars line": [["A"], ["B", "line"], "bar"],
  "line points": [["A"], ["B", "points"], "area"],
  "3 mixed": [["A"], ["B", "bar"], ["C"], "area"],
  "pct mixed": [["A"], ["B", "bar"], ["C"], "area", true],
  "neg mixed": [["A"], ["NEGB", "bar"], ["C"], "area"],
};
// The tick labels of Grafana 12.4.2 on the plots of 133, 247 and 437 px (the time series of Hune has the same plots).
const TS_TICKS = {
  "line bars": ["0 20 40 60 80 100", "0 20 40 60 80 100", "0 10 20 30 40 50 60 70 80 90 100"],
  "3 mixed": ["0 20 40 60 80 100 120 140 160", "0 20 40 60 80 100 120 140 160", "0 20 40 60 80 100 120 140 160"],
  "pct mixed": ["0% 20% 40% 60% 80% 100%", "0% 20% 40% 60% 80% 100%", "0% 10% 20% 30% 40% 50% 60% 70% 80% 90% 100%"],
  "neg mixed": ["-100 -50 0 50 100 150", "-100 -50 0 50 100 150", "-100 -80 -60 -40 -20 0 20 40 60 80 100 120 140 160"],
};
TS_TICKS["bars line"] = TS_TICKS["line points"] = TS_TICKS["line bars"];
const TS_PLOTS = [133, 247, 437];
// The drawn values of Grafana on the plot of 437 px at the points 0, 10, ..., 60: the value of one pixel, the lines in draw order, the tops of the
// bars. Grafana draws a percent stack in shares of 1, and Hune draws it in percent.
const TS_DRAWN = {
  "line bars": [0.238, [[50, 94.762, 41.429, 6.905, 66.905, 90, 25.476]], [70.032, 37.145, 10.555, 48.532, 67.852, 26.176, 14.835]],
  "bars line": [0.238, [[70, 37.143, 10.476, 48.571, 67.857, 26.19, 14.762]], [50.023, 94.837, 41.443, 6.851, 66.867, 89.97, 25.531]],
  "line points": [0.238, [[50, 94.762, 41.429, 6.905, 66.905, 90, 25.476]], null],
  "3 mixed": [0.366, [[50.16, 94.828, 41.373, 6.957, 67.002, 90.069, 25.629], [79.817, 136.934, 52.357, 55.652, 86.041, 118.627, 68.467]], [70, 37.128, 10.55, 48.51, 67.821, 26.164, 14.828]],
  "pct mixed": [0.229, [[62.5, 69.3, 79.2, 12.4, 77.8, 75.7, 37.3], [100, 100, 100, 100, 100, 100, 100]], [100, 100, 100, 100, 100, 100, 100]],
  "neg mixed": [0.618, [[50.071, 94.537, 41.425, 6.841, 66.746, 89.596, 25.368], [79.715, 136.532, 51.924, 55.629, 85.891, 118.622, 68.599]], [-69.395, -36.538, -9.971, -47.915, -67.218, -25.579, -14.248]],
};

function tsDraw(name, sign = "series") {
  const spec = TS_CASES[name];
  const parts = spec.filter(Array.isArray);
  const [type, percent = false] = spec.filter((x) => !Array.isArray(x));
  const xs = Float64Array.from({ length: N }, (_, i) => 1794528000000 + i * 300000);
  const series = parts.map(([key, kind], k) => ({ id: String(k), label: key, color: COLORS[k], values: Float64Array.from({ length: N }, (_, n) => TS[key](n)), nulls: new Uint8Array(N), group: "all", type: kind }));
  const { hook } = scaleAxis({ ends: { min: null, max: null, softMin: null, softMax: null }, binary: false, tickText: tickLabel, plainTick: tickLabel, percent, log: false });
  return { percent, ...draw({ xKind: "time", xs, series, type, stack: true, stackMode: percent ? "percent" : undefined, stackSign: sign, drawOrder: "series", connectGaps: true, yAxis: hook }) };
}

test("E33: a line, bars and points of one time series stack in groups of their own: the ticks of Grafana 12.4.2 on 3 plots", () => {
  let checked = 0;
  for (const name of Object.keys(TS_CASES)) {
    const { raw, percent } = tsDraw(name);
    const { hook } = scaleAxis({ ends: { min: null, max: null, softMin: null, softMax: null }, binary: false, tickText: tickLabel, plainTick: tickLabel, percent, log: false });
    TS_PLOTS.forEach((h, k) => {
      assert.equal(hook(0, 1, h, raw).ticks.map((t) => t.label).join(" "), TS_TICKS[name][k], `${name}, plot ${h} px`);
      checked++;
    });
  }
  assert.equal(checked, 18);
});

test("E33: the drawn lines and bars of the mixed time series equal those of Grafana 12.4.2 within 1.5 px", () => {
  for (const [name, [px, lines, bars]] of Object.entries(TS_DRAWN)) {
    const { marks } = tsDraw(name);
    const drawnLines = marks.filter((m) => m.line && m.line.length === N);
    lines.forEach((want, k) => AT.forEach((i, j) => close(drawnLines[k].line[i], want[j], 1.5 * px, `${name}, line ${k}, point ${i}`)));
    if (!bars) continue;
    const b = marks.find((m) => m.bars).bars;
    assert.equal(b.length, N, `${name}: bars`);
    AT.forEach((i, j) => close(Math.min(b[i][0], b[i][1]) < 0 ? b[i][0] : b[i][1], bars[j], 1.5 * px, `${name}, bar ${i}`));
  }
});

test("E33: without stackSign the engine keeps one group for the line and the bars", () => {
  const { marks } = tsDraw("line bars", null);
  const b = marks.find((m) => m.bars).bars;
  // The bars of B stack on A: the bar at point 0 starts at A(0) = 50.
  close(b[0][0], TS.A(0), 1e-6, "base of the bar");
});

// ---- (b) The bar chart: the stack of E32 and E33, the value axis of Grafana ----

const BAR = {
  P1: (n) => 10 + ((n * 37) % 70),
  P2: (n) => 5 + ((n * 23) % 40),
  P3: (n) => 8 + ((n * 11) % 30),
  N1: (n) => -(5 + ((n * 13) % 50)),
  MIX: (n) => ((n * 29) % 80) - 25,
  MIXN: (n) => 25 - ((n * 29) % 80),
  FAR1: (n) => 1000 + ((n * 3) % 10),
  FAR2: (n) => 1000 + ((n * 7) % 10),
};
// The cases: the series, the stacking, the extra ends of the field (soft_max) and the unit.
const BAR_CASES = {
  "normal pos": [["P1", "P2"], "normal"],
  "normal neg": [["P1", "N1"], "normal"],
  "normal mix": [["P1", "MIX"], "normal"],
  "normal mixneg": [["P1", "MIXN"], "normal"],
  "pct pos": [["P1", "P2"], "percent"],
  "pct neg": [["P1", "N1"], "percent"],
  "normal far": [["FAR1", "FAR2"], "normal"],
  "none pos": [["P1", "P2"], "none"],
  "pct unit": [["P1", "P2"], "percent", null, "percentunit"],
  "normal 3": [["P1", "N1", "P2"], "normal"],
  "pct mix": [["P1", "MIX", "P3"], "percent"],
  "none one": [["P1"], "none"],
  "normal soft": [["P1", "P2"], "normal", 200],
};
// The tick labels of Grafana 12.4.2 on the plots of 123, 237 and 427 px. With `showValue` = auto, Grafana keeps the same axis: the value labels
// take no room ("normal auto" and "none auto" of the measure have the ticks of "normal pos" and "none pos").
const BAR_TICKS = {
  "normal pos": ["0 20 40 60 80 100", "0 20 40 60 80 100", "0 10 20 30 40 50 60 70 80 90 100"],
  "normal neg": ["-60 -40 -20 0 20 40 60", "-60 -40 -20 0 20 40 60", "-60 -50 -40 -30 -20 -10 0 10 20 30 40 50 60 70"],
  "normal mix": ["-20 0 20 40 60 80 100", "-20 0 20 40 60 80 100", "-30 -20 -10 0 10 20 30 40 50 60 70 80 90 100 110"],
  "normal mixneg": ["-40 -20 0 20 40 60", "-40 -20 0 20 40 60", "-50 -40 -30 -20 -10 0 10 20 30 40 50 60"],
  "pct pos": ["0 0.2 0.4 0.6 0.8 1", "0 0.2 0.4 0.6 0.8 1", "0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1"],
  "pct neg": ["-1 -0.5 0 0.5 1", "-1 -0.5 0 0.5 1", "-1.2 -1 -0.8 -0.6 -0.4 -0.2 0 0.2 0.4 0.6 0.8 1 1.2"],
  "normal far": ["0 500 1000 1500 2000", "0 500 1000 1500 2000", "0 200 400 600 800 1000 1200 1400 1600 1800 2000 2200"],
  "none pos": ["0 10 20 30 40 50 60", "0 10 20 30 40 50 60", "0 5 10 15 20 25 30 35 40 45 50 55 60 65"],
  "pct unit": ["0% 20% 40% 60% 80% 100%", "0% 20% 40% 60% 80% 100%", "0% 10% 20% 30% 40% 50% 60% 70% 80% 90% 100%"],
  "normal 3": ["-50 -25 0 25 50 75 100", "-50 -25 0 25 50 75 100", "-60 -40 -20 0 20 40 60 80 100"],
  "pct mix": ["-1 0 1 2", "-1 0 1 2", "-1.5 -1 -0.5 0 0.5 1 1.5 2 2.5"],
  "none one": ["0 10 20 30 40 50 60", "0 10 20 30 40 50 60", "0 5 10 15 20 25 30 35 40 45 50 55 60"],
  "normal soft": ["0 25 50 75 100 125 150 175 200", "0 50 100 150 200", "0 20 40 60 80 100 120 140 160 180 200"],
};
const BAR_PLOTS = [123, 237, 427];
// The [bottom, top] of each bar of Grafana on the plot of 427 px, in the order of the series, and the value of one pixel. Shares of 1 in percent.
const BAR_DRAWN = {
  "normal mix": [0.328, [[0, 10], [0, 47], [0, 14], [0, 51], [0, 18], [0, 55], [0, 22], [0, 59]],
    [[-14.672, 10], [47.328, 51], [14.328, 47], [33.328, 51], [18.328, 29], [55.328, 95], [11.328, 22], [59.328, 77]]],
  "normal mixneg": [0.278, null, [[0, 24.944], [-3.756, 0], [-32.734, 0], [0, 17.949], [-10.751, 0], [-39.729, 0], [0, 10.954], [-17.745, 0]]],
  "normal 3": [0.422, null, [[-4.615, 0], [-17.633, 0], [-30.652, 0], [-43.672, 0], [-6.617, 0], [-19.637, 0], [-32.656, 0], [-45.675, 0]],
    [[10.408, 14.993], [47.462, 75.081], [14.414, 25.007], [51.467, 85.095], [18.419, 35.022], [55.474, 95.11], [22.425, 45.037], [59.479, 65.066]]],
  "pct mix": [0.0101, [[-1.421, 0], [0, 0.67], [0, 0.181], [0, 1.158], [0, 0.352], [0, 0.429], [0, 0.879], [0, 0.577]],
    [[-1.421, 2.143], [0.681, 0.728], [0.191, 0.609], [0.759, 1.158], [0.362, 0.568], [0.439, 0.741], [0.449, 0.879], [0.587, 0.754]],
    [[1.009, 2.143], [0.738, 1], [0.619, 1], [0.759, 1], [0.578, 1], [0.751, 1], [0.449, 1], [0.764, 1]]],
  "pct neg": [0.00562, [[0, 1], [0, 1], [0, 1], [0, 1], [0, 1], [0, 1], [0, 1], [0, 1]], [[-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0], [-1, 0]]],
};

function barDraw(name) {
  const [keys, stacking, softMax = null, unit = "none"] = BAR_CASES[name];
  const scale = stacking === "percent" ? 100 : 1;
  const hookOf = () => barValueAxis({ min: null, max: null, softMin: null, softMax }, scale, yTickLabeller({ config: { unit, decimals: null } }), false);
  const series = keys.map((key, k) => ({ id: String(k), label: key, color: COLORS[k], values: Float64Array.from({ length: 8 }, (_, n) => BAR[key](n)), nulls: null, group: "all" }));
  const xs = Float64Array.from({ length: 8 }, (_, i) => i);
  const out = draw({ xKind: "category", xs, categories: xs.map(String), series, type: "bar", stack: stacking !== "none", stackMode: stacking === "percent" ? "percent" : undefined, stackSign: "series", barMaxWidth: 1e5, yAxis: hookOf() });
  return { ...out, hookOf, scale };
}

test("9.19: the value axis of a bar chart has the ticks of Grafana 12.4.2: 13 cases on 3 plots, with stacks, values below 0, percent and soft ends", () => {
  let checked = 0;
  for (const name of Object.keys(BAR_CASES)) {
    const { raw, hookOf } = barDraw(name);
    BAR_PLOTS.forEach((h, k) => {
      assert.equal(hookOf()(0, 1, h, raw).ticks.map((t) => t.label).join(" "), BAR_TICKS[name][k], `${name}, plot ${h} px`);
      checked++;
    });
  }
  assert.equal(checked, 39);
});

test("E32, E33: the bars of a bar chart stack as in Grafana 12.4.2: a series by the sign of most values, each value with its sign", () => {
  for (const [name, [px, ...want]] of Object.entries(BAR_DRAWN)) {
    const { marks, scale } = barDraw(name);
    want.forEach((bars, k) => {
      if (!bars) return;
      bars.forEach(([lo, hi], i) => {
        close(marks[k].bars[i][0] / scale, lo, 1.5 * px, `${name}, series ${k}, bar ${i}, bottom`);
        close(marks[k].bars[i][1] / scale, hi, 1.5 * px, `${name}, series ${k}, bar ${i}, top`);
      });
    });
  }
});

test("9.19: the bar axis follows the range rule of Grafana: the room of a single value and a hard end", () => {
  const plain = yTickLabeller({ config: { unit: "none", decimals: null } });
  // One bar of 526,930 (Grafana, a capture of 2026-11-11): a flat range gets the value as room, 0 to 1,060,000, step 200,000 on a plot of 173 px.
  const one = barValueAxis({ min: null, max: null, softMin: null, softMax: null }, 1, plain, false)(0, 0, 173, [526930, 526930]);
  assert.deepEqual([one.min, one.max, one.step], [0, 1060000, 200000]);
  // Bars of 0 to 96,000: 10 % of room, rounded up to a tenth of the decade of the range: 0 to 106,000.
  const room = barValueAxis({ min: null, max: null, softMin: null, softMax: null }, 1, plain, false)(0, 0, 247, [0, 96000]);
  assert.deepEqual([room.min, room.max], [0, 106000]);
  assert.deepEqual(room.ticks.map((t) => t.v), [0, 20000, 40000, 60000, 80000, 100000]);
  // A hard end is the end of the axis, and it is also the data end of its side.
  const hard = barValueAxis({ min: null, max: 50, softMin: null, softMax: null }, 1, plain, false)(0, 0, 247, [5, 95]);
  assert.deepEqual([hard.min, hard.max], [0, 50]);
});

// ---- (c) The histogram: the lowest drawn top of a stack, and percent in shares of 1 ----

// The counts of the buckets of the measure fixture (the bucket rule of 9.19f, equal in both products; the values come from the ClickHouse queries).
const AB = [[1195, 690, 452, 242, 172, 101, 65, 38, 16, 9, 6, 5, 4, 2, 2, 0, 0, 1], [991, 1100, 487, 241, 107, 43, 15, 7, 5, 3, 0, 1, 0, 0, 0, 0, 0, 0]];
const C3 = [0, 1452, 1124, 321, 77, 17, 7, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const SPLIT = [[273, 237, 227, 223, 247, 221, 229, 242, 260, 238, 236, 255, 112, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  [0, 0, 0, 0, 0, 0, 0, 85, 202, 168, 178, 146, 165, 178, 167, 158, 168, 152, 178, 194, 177, 165, 177, 170, 172]];
const SMALL = [[11, 3, 4, 6, 5, 3, 3, 7, 6, 4, 6, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0, 0, 2, 9, 3, 2, 4, 4, 5, 1, 2, 2, 5, 4, 4, 2, 3, 7, 1]];
// The cases: the counts and the stacking. Then the tick labels of Grafana on the plots of 133, 247 and 437 px, and the end of its axis on these plots.
const HIST = {
  "normal ab": [AB, "normal", ["0 500 1000 1500 2000 2500", "0 500 1000 1500 2000 2500", "0 200 400 600 800 1000 1200 1400 1600 1800 2000 2200 2400"], [2500, 2500, 2497.1]],
  "normal split": [SPLIT, "normal", ["0 100 200 300 400 500", "0 100 200 300 400 500", "0 50 100 150 200 250 300 350 400 450 500"], [511.5, 510.3, 510.5]],
  "normal 3": [[...AB, C3], "normal", ["0 500 1000 1500 2000 2500 3000 3500", "0 500 1000 1500 2000 2500 3000 3500",
    "0 250 500 750 1000 1250 1500 1750 2000 2250 2500 2750 3000 3250 3500"], [3608.5, 3602.1, 3598.8]],
  "normal small": [SMALL, "normal", ["0 2 4 6 8 10 12 14 16", "0 2.5 5 7.5 10 12.5 15", "0 2 4 6 8 10 12 14 16"], [17.02, 17.0, 17.01]],
  "pct ab": [AB, "percent", ["0 0.2 0.4 0.6 0.8 1", "0 0.2 0.4 0.6 0.8 1", "0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1"], [1.0726, 1.0693, 1.0711]],
  "pct split": [SPLIT, "percent", ["0 0.2 0.4 0.6 0.8 1", "0 0.2 0.4 0.6 0.8 1", "0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1 1.1"], [1.0992, 1.0978, 1.1]],
  "pct 3": [[...AB, C3], "percent", ["0 0.2 0.4 0.6 0.8 1", "0 0.2 0.4 0.6 0.8 1", "0 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1"], [1.0813, 1.0786, 1.079]],
  "none ab": [AB, "none", ["0 200 400 600 800 1000 1200 1400", "0 200 400 600 800 1000 1200 1400",
    "0 100 200 300 400 500 600 700 800 900 1000 1100 1200 1300 1400"], [1400, 1400, 1400]],
};
const HIST_PLOTS = [133, 247, 437];

test("9.19f: the y axis of a stacked histogram has the ticks and the end of Grafana 12.4.2 on 3 plots, also in percent", () => {
  let checked = 0;
  for (const [name, [counts, stacking, ticks, ends]] of Object.entries(HIST)) {
    const vis = counts.map((c) => ({ counts: c }));
    // The highest drawn top: the sum of a bucket in a stack (100 in percent), else the highest count.
    const high = stacking === "none" ? Math.max(...counts.flat()) : stacking === "percent" ? 100 : Math.max(...counts[0].map((_, j) => counts.reduce((a, c) => a + c[j], 0)));
    const share = stacking === "percent" ? 100 : 1;
    HIST_PLOTS.forEach((h, k) => {
      const ax = histAxis(vis, stacking, high, h);
      assert.equal(ax.ticks.map((t) => t.label).join(" "), ticks[k], `${name}, plot ${h} px`);
      // The end of the axis is within 1 px of the end that Grafana drew.
      close(ax.max / share, ends[k], ends[k] / h, `${name}, end on ${h} px`);
      checked++;
    });
  }
  assert.equal(checked, 24);
});

test("9.19f: a bucket where the first series of a stack has no count gives the lowest top 0; a hidden series does not count", () => {
  // "normal split": the first series has no count from bucket 13, where the second has counts, so its top there is 0: the axis ends at 510.
  assert.equal(histAxis(SPLIT.map((c) => ({ counts: c })), "normal", 462, 247).max, 510);
  // With the first series hidden, the lowest top is the lowest count of the second series above 0 (85): the axis ends at 220.
  assert.equal(histAxis([{ counts: SPLIT[1] }], "normal", 202, 247).max, 220);
});
