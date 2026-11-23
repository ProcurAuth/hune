// SPDX-License-Identifier: MIT OR Apache-2.0
// The stacks of a time series as Grafana 12.4.2 draws them (9.19, engine edit E32): a series with more values below 0
// stacks down from 0 in a group of its own, its values add as they are, and a NULL takes the top of its group. The expected values are those that
// the worker y-stackneg measured on Grafana 12.4.2 on 2026-11-21: 100 time series panels (20 cases at 5 heights, plots of 57 to 817 px), the tick
// labels and the drawn points of the lines. The engine runs here on a small fake of the DOM and of the canvas, which records the lines.
import test from "node:test";
import assert from "node:assert/strict";
import { tickLabel } from "../../web/js/panels/series.js";
import { scaleAxis } from "../../web/js/panels/timeseries.js";

// ---- The data of the measure fixture: 61 points, n = 0 to 60 (NaN is a NULL) ----

const N = 61;
const DATA = {
  A: (n) => Math.sin(n / 6) * 45 + 50, // 5 to 95
  B: (n) => Math.cos(n / 6) * 30 + 40, // 10 to 70
  NEGB: (n) => -(Math.cos(n / 6) * 30 + 40), // -70 to -10
  NEGC: (n) => Math.sin(n / 6) * 20 - 40, // -60 to -20
  MIXP: (n) => Math.sin(n / 6) * 40 + 15, // -25 to 55, 45 values above 0
  MIXN: (n) => Math.sin(n / 6) * 40 - 15, // -55 to 25, 33 values below 0
  FIRSTPOS: (n) => (n < 10 ? 30 : -20), // the first 10 values above 0, then 51 values below 0
  TIE: (n) => (n < 30 ? 10 : n < 60 ? -10 : 0), // 30 values above 0, 30 below 0
  FARN: (n) => (n % 7 === 3 ? NaN : Math.sin(n / 6) * 5 + 1005),
  FARB: (n) => Math.cos(n / 6) * 5 + 1005,
  FARBN: (n) => (n % 7 === 3 ? NaN : Math.cos(n / 6) * 5 + 1005),
};
// The cases: the two series, the engine type, the percent mode, a negated second series (`transform` = `negative_y`).
const CASES = {
  "neg line": ["A", "NEGB"], "neg area": ["A", "NEGB"], "neg bars": ["A", "NEGB", "bar"], "all neg": ["NEGB", "NEGC"],
  "mix pos": ["A", "MIXP"], "mix neg": ["A", "MIXN"], "first pos": ["A", "FIRSTPOS"], tie: ["B", "TIE"],
  "null one": ["FARN", "FARB"], "null both": ["FARN", "FARBN"], "mix bars": ["B", "MIXP", "bar"], "neg first": ["NEGB", "A"],
  negate: ["A", "B", "area", false, true], "pct neg": ["A", "NEGB", "area", true], "mix line": ["B", "MIXN"], "neg bars2": ["NEGB", "NEGC", "bar"],
  "null span": ["FARN", "FARBN"], "pct mix": ["MIXP", "B", "area", true], "pct allneg": ["NEGB", "NEGC", "area", true], "mix neg bars": ["A", "MIXN", "bar"],
};
// The tick labels that Grafana 12.4.2 drew for each case on the plots of 57, 133, 209, 361 and 817 px.
const GRAFANA_TICKS = {
  "neg line": ["0 100", "-75 -50 -25 0 25 50 75 100", "-50 0 50 100", "-80 -60 -40 -20 0 20 40 60 80 100 120", "-90 120 10"],
  "all neg": ["-100 -50 0", "-120 -100 -80 -60 -40 -20 0", "-120 -100 -80 -60 -40 -20 0", "-120 -100 -80 -60 -40 -20 0", "-130 0 5"],
  "mix pos": ["0 100", "-25 0 25 50 75 100 125 150", "0 50 100 150", "-40 -20 0 20 40 60 80 100 120 140 160", "-40 170 10"],
  "mix neg": ["-50 0 50 100", "-50 -25 0 25 50 75 100", "-50 0 50 100", "-60 -40 -20 0 20 40 60 80 100", "-70 110 10"],
  "first pos": ["0 50 100", "-40 -20 0 20 40 60 80 100", "-25 0 25 50 75 100", "-40 -20 0 20 40 60 80 100", "-40 110 10"],
  tie: ["0 25 50 75", "0 10 20 30 40 50 60 70 80", "0 20 40 60 80", "0 10 20 30 40 50 60 70 80", "0 85 5"],
  "null one": ["0 1000 2000", "0 500 1000 1500 2000", "0 500 1000 1500 2000", "0 200 400 600 800 1000 1200 1400 1600 1800 2000 2200", "0 2300 100"],
  "null both": ["1000 1500 2000", "800 1000 1200 1400 1600 1800 2000 2200", "1000 1250 1500 1750 2000", "800 1000 1200 1400 1600 1800 2000 2200", "800 2200 100"],
  "mix bars": ["0 50 100", "0 20 40 60 80 100", "0 20 40 60 80 100", "0 10 20 30 40 50 60 70 80 90 100 110", "0 115 5"],
  "mix line": ["-50 0 50", "-60 -40 -20 0 20 40 60 80", "-50 -25 0 25 50 75", "-60 -40 -20 0 20 40 60 80", "-70 90 10"],
  "neg bars2": ["-100 -50 0", "-120 -100 -80 -60 -40 -20 0", "-120 -100 -80 -60 -40 -20 0", "-120 -100 -80 -60 -40 -20 0", "-130 0 5"],
  "pct neg": ["-100% 0% 100%", "-100% -75% -50% -25% 0% 25% 50% 75% 100%", "-100% -50% 0% 50% 100%",
    "-120% -100% -80% -60% -40% -20% 0% 20% 40% 60% 80% 100%", "-120 100 10"],
  "pct mix": ["-400% -200% 0%", "-400% -300% -200% -100% 0% 100%", "-400% -300% -200% -100% 0% 100%",
    "-400% -350% -300% -250% -200% -150% -100% -50% 0% 50% 100%", "-420 100 20"],
  "pct allneg": ["-100% 0% 100%", "-100% -75% -50% -25% 0% 25% 50% 75% 100%", "-100% -50% 0% 50% 100%",
    "-100% -80% -60% -40% -20% 0% 20% 40% 60% 80% 100%", "-100 100 10"],
  "mix neg bars": ["-50 0 50 100", "-50 -25 0 25 50 75 100", "-50 0 50 100", "-60 -40 -20 0 20 40 60 80 100", "-70 110 10"],
};
// These cases have the ticks of "neg line" (the same data in the same groups).
for (const name of ["neg area", "neg bars", "neg first", "negate"]) GRAFANA_TICKS[name] = GRAFANA_TICKS["neg line"];
GRAFANA_TICKS["null span"] = GRAFANA_TICKS["null both"];
const PLOTS = [57, 133, 209, 361, 817];
// The drawn values of the lines of Grafana on the plot of 817 px at the points 0, 10, ..., 60, and the value of one pixel there.
const GRAFANA_DRAWN = {
  "neg area": [0.257, [50.086, 94.81, 41.346, 6.903, 66.793, 89.927, 25.41], [-69.951, -37.05, -10.575, -48.617, -67.895, -26.255, -14.945]],
  "all neg": [0.159, [-70.012, -37.075, -10.502, -48.531, -67.785, -26.095, -14.798], [-109.951, -57.283, -54.419, -107.723, -100.404, -48.372, -65.716]],
  "mix pos": [0.257, [49.963, 94.688, 41.481, 6.781, 66.928, 89.804, 25.545], [64.871, 149.694, 48.678, -16.61, 96.744, 140.441, 18.862]],
  "mix neg": [0.22, [50.073, 94.798, 41.481, 6.891, 66.818, 89.951, 25.618], [-14.92, 24.737, -22.632, -53.256, 0.061, 20.551, -36.732]],
  "first pos": [0.184, [49.963, 94.761, 41.334, 6.818, 66.854, 89.988, 25.545], [29.951, -19.988, -19.988, -19.988, -19.988, -19.988, -19.988]],
  tie: [0.108, [70.025, 37.167, 10.558, 48.479, 67.871, 26.179, 14.867], [80.044, 47.186, 20.577, 38.568, 57.852, 16.16, 14.867]],
  "null one": [2.815, [1005.018, 0, 1005.018, 999.388, 1007.834, 1010.649, 1002.203], [2015.667, 1005.018, 2004.406, 2007.222, 2015.667, 2012.852, 2004.406]],
  "neg first": [0.257, [-69.951, -37.05, -10.575, -48.617, -67.895, -26.255, -14.945], [50.086, 94.81, 41.346, 6.903, 66.793, 89.927, 25.41]],
  negate: [0.257, [50.086, 94.81, 41.346, 6.903, 66.793, 89.927, 25.41], [-69.951, -37.05, -10.575, -48.617, -67.895, -26.255, -14.945]],
  "mix line": [0.196, [70.024, 37.124, 10.49, 48.482, 67.87, 26.157, 14.798], [-14.969, 24.786, -22.607, -53.354, -0.086, 20.477, -36.707]],
  // Grafana draws a percent stack in shares of 1. Hune draws it in percent.
  "pct neg": [0.269, [100, 100, 100, 100, 100, 100, 100], [-100.1, -100.1, -100.1, -100.1, -100.1, -100.1, -100.1]],
  "pct mix": [0.636, [17.9, 59.9, 41.4, -92.9, 30.6, 65.6, -83.9], [100, 100, 100, 100, 100, 100, 100]],
  "pct allneg": [0.256, [-63.7, -65, -19.4, -45, -67.5, -54, -22.5], [-100, -100, -100, -100, -100, -100, -100]],
};

// ---- The engine on a fake DOM that records the lines ----

const WIDTH = 600;
let strokes = [];

function fakeContext() {
  const state = { path: [] };
  const methods = {
    measureText: (s) => ({ width: 6.5 * String(s).length }),
    beginPath: () => { state.path = []; },
    moveTo: (x, y) => { state.path.push([0, x, y]); },
    lineTo: (x, y) => { state.path.push([1, x, y]); },
    stroke: () => { strokes.push({ color: state.strokeStyle, path: state.path.slice() }); },
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
// A colour in hex (the colour of a series) stays as it is, so the test finds the line of each series by its colour.
globalThis.getComputedStyle = (el) => ({ color: /^#/.test(el?.style?.color ?? "") ? el.style.color : "rgb(204, 204, 220)", fontFamily: "Inter", getPropertyValue: () => "" });
globalThis.requestAnimationFrame = () => 0;

const { chartCore } = await import("../../web/js/vendor/chdash/app_chart_core.js");

const xs = Float64Array.from({ length: N }, (_, i) => 1794528000000 + i * 300000);
const COLORS = ["#ff0000", "#0000ff"];
const HEIGHT = 400;

/**
 * Draw a case once, as the time series asks the engine (`stackSign: "series"` unless `sign` is false). Gives the data ends of the hook (E30), the
 * range of the drawn axis and, for each series, the values of the drawn line (the y of each point, read back with the range of the axis).
 */
function draw(name, { sign = true, nulls = "flags", log = false, extra = {} } = {}) {
  const [a, b, type = "area", percent = false, negate = false] = CASES[name];
  const series = [a, b].map((key, k) => {
    const values = Float64Array.from({ length: N }, (_, n) => DATA[key](n));
    const flags = nulls === "span" ? new Uint8Array(N) : Uint8Array.from(values, (v) => (v === v ? 0 : 1));
    return { id: String(k), label: key, color: COLORS[k], values, nulls: flags, group: "all", negate: k === 1 && negate ? true : undefined };
  });
  const { hook } = scaleAxis({ ends: { min: null, max: null, softMin: null, softMax: null }, binary: false, tickText: tickLabel, plainTick: tickLabel, percent, log: false });
  let raw = null;
  let info = null;
  const spy = (dataMin, dataMax, plotH, r) => { raw = r; return hook(dataMin, dataMax, plotH, r); };
  strokes = [];
  const chart = chartCore.create(fakeElement(HEIGHT), {
    height: HEIGHT, xKind: "time", xs, series, type, stack: true, stackMode: percent ? "percent" : undefined, stackSign: sign ? "series" : undefined,
    connectGaps: true, yAxis: spy, onDraw: (i) => { info = i; }, ...extra,
  });
  chart.flush();
  const [, top, , plotH] = chart.root.dataset.plot.split(" ").map(Number);
  chart.destroy();
  const [l0, l1] = [Math.log(info.yMin), Math.log(info.yMax)];
  const valueOf = log ? (y) => Math.exp(l0 + ((top + plotH - y) / plotH) * (l1 - l0)) : (y) => info.yMax - ((y - top) / plotH) * (info.yMax - info.yMin);
  const lines = COLORS.map((c) => {
    const rgb = [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16)).join(",\\s*");
    const s = strokes.filter((o) => new RegExp(`rgba?\\(${rgb}`).test(String(o.color)) && o.path.length > 2).at(-1);
    return s ? { values: s.path.map((p) => valueOf(p[2])), runs: s.path.filter((p) => p[0] === 0).length } : null;
  });
  return { raw, range: [info.yMin, info.yMax], lines };
}

/** The tick labels of the hook of the time series for the data ends `raw` and the plot height `h`: all labels, or "first last step" on 817 px. */
function ticksOf(name, raw, h) {
  const percent = !!CASES[name][3];
  const { hook } = scaleAxis({ ends: { min: null, max: null, softMin: null, softMax: null }, binary: false, tickText: tickLabel, plainTick: tickLabel, percent, log: false });
  const ax = hook(0, 1, h, raw);
  return h === 817 ? `${ax.ticks[0].v} ${ax.ticks.at(-1).v} ${ax.step}` : ax.ticks.map((t) => t.label).join(" ");
}

test("E32: the ticks of each case equal the ticks of Grafana 12.4.2 on the plots of 57 to 817 px", () => {
  let checked = 0;
  for (const name of Object.keys(CASES)) {
    const { raw } = draw(name);
    PLOTS.forEach((h, k) => {
      assert.equal(ticksOf(name, raw, h), GRAFANA_TICKS[name][k], `${name}, plot ${h} px`);
      checked++;
    });
  }
  assert.equal(checked, 100);
});

test("E32: the drawn lines equal the lines of Grafana 12.4.2 within 1 px", () => {
  for (const [name, [px, ...want]] of Object.entries(GRAFANA_DRAWN)) {
    const { lines } = draw(name);
    want.forEach((values, k) => {
      const got = [0, 10, 20, 30, 40, 50, 60].map((i) => lines[k].values[i]);
      for (let j = 0; j < got.length; j++) {
        assert.ok(Math.abs(got[j] - values[j]) <= px * (CASES[name][3] ? 100 : 1), `${name}, series ${k}, point ${10 * j}: ${got[j]} and Grafana ${values[j]}`);
      }
    });
  }
});

test("E32: a series stacks down from 0 by the sign of most of its values, not of its first value; a tie stacks up", () => {
  // "first pos": 10 values above 0 first, then 51 below 0. The series is in the group below 0: its line is its own value, -20.
  assert.equal(draw("first pos").lines[1].values[30], -20);
  // "tie": 30 values above 0 and 30 below 0. The series adds to the stack of B.
  const tie = draw("tie").lines[1].values[40];
  assert.ok(Math.abs(tie - (DATA.B(40) + DATA.TIE(40))) < 1e-9, `tie ${tie}`);
  // A negated series (`transform` = `negative_y`) draws -B below 0 in its own group.
  assert.ok(Math.abs(draw("negate").lines[1].values[30] + DATA.B(30)) < 1e-9);
});

test("E32: a NULL is at the top of its group when another series of the group has a value there, else it is not drawn", () => {
  // n = 10 is a NULL of FARN. "null one": FARB has a value there, so the line of FARN goes to 0 and the range starts at 0.
  const one = draw("null one");
  assert.equal(one.lines[0].values[10], 0);
  // "null both": both series have a NULL at n = 3, 10, ..., 59. The tops there are not drawn and do not count for the range.
  const both = draw("null both");
  assert.ok(both.raw[0] > 1000 && both.raw[1] < 2018, `raw ${both.raw}`);
  assert.equal(both.lines[0].values.length, 52);
  // The line breaks at each of the 9 NULL values: 10 runs. With span_nulls (no NULL flags), it does not break.
  assert.equal(both.lines[0].runs, 10);
  assert.equal(draw("null span", { nulls: "span" }).lines[0].runs, 1);
});

test("E32: without the option the engine keeps its stack: the values add up, and only bars stack their values below 0 down", () => {
  const area = draw("neg area", { sign: false });
  assert.ok(Math.abs(area.lines[1].values[30] - (DATA.A(30) + DATA.NEGB(30))) < 1e-9);
  const both = draw("null both", { sign: false });
  assert.ok(both.raw[0] < 1, `a NULL counts as 0: raw ${both.raw}`);
});

test("9.19: a log scale keeps the rule of the engine: no stack, the axis from the lowest to the highest drawn value, no room", () => {
  const LOG = { yScale: "log", yAxis: undefined };
  const close = (a, b) => Math.abs(a - b) < 1e-6 * Math.abs(b);
  let lo = Infinity;
  let hi = -Infinity;
  for (let n = 0; n < N; n++) for (const v of [DATA.FARN(n), DATA.FARB(n)]) if (v === v) { lo = Math.min(lo, v); hi = Math.max(hi, v); }
  const far = draw("null one", { log: true, extra: LOG });
  assert.ok(close(far.range[0], lo) && close(far.range[1], hi), `range ${far.range}`);
  // The series do not stack: the second line is FARB itself (about 1,005), not the top of the stack (about 2,010).
  assert.ok(close(far.lines[1].values[0], DATA.FARB(0)), `${far.lines[1].values[0]}`);
  // A hard end has no effect. A soft end moves the data end of its side.
  assert.deepEqual(draw("null one", { log: true, extra: { ...LOG, yBounds: [1, 5000] } }).range, far.range);
  const soft = draw("null one", { log: true, extra: { ...LOG, ySoft: [100, null] } }).range;
  assert.ok(soft[0] === 100 && close(soft[1], hi), `soft ${soft}`);
  // A value at 0 or below is not drawn, and the axis then starts at a tenth of the highest value.
  const mix = draw("mix pos", { log: true, extra: LOG });
  const top = Math.max(...Array.from({ length: N }, (_, n) => Math.max(DATA.A(n), DATA.MIXP(n))));
  assert.ok(close(mix.range[0], top / 10) && close(mix.range[1], top), `range ${mix.range}`);
  assert.equal(mix.lines[1].values.length, Array.from({ length: N }, (_, n) => DATA.MIXP(n)).filter((v) => v > 0).length);
  // With percent, the lines are the values, not the shares.
  assert.ok(close(draw("pct mix", { log: true, extra: LOG }).lines[1].values[0], DATA.B(0)));
});
