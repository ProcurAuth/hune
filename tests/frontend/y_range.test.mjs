// The y range of a time series as Grafana 12.4.2 draws it (9.19, decision ID-K316), the data ends that the engine gives to a yAxis hook (E30) and
// the cut of a long axis title (E31, item P6 of the security review). The expected values are the ranges and the ticks that the worker y-range
// measured on Grafana 12.4.2 on 2026-11-16: 100 time series panels with bars, stacks, soft ends and hard ends, with plots of 57 to 817 px.
// The engine runs here on a small fake of the DOM and of the canvas.
import test from "node:test";
import assert from "node:assert/strict";
import { autoYRange, tickLabel } from "../../web/js/panels/series.js";
import { scaleAxis } from "../../web/js/panels/timeseries.js";

// ---- The data of the measure fixture: 61 points, n = 0 to 60 ----

const N = 61;
const A = (n) => Math.sin(n / 6) * 45 + 50; // 5.05 to 94.99
const NP = (n) => Math.sin(n / 6) * 65 + 15; // -49.93 to 79.99
const FAR = (n) => Math.sin(n / 6) * 5 + 1005;
const NFAR = (n) => Math.sin(n / 6) * 5 - 1005;
const NEG = (n) => Math.sin(n / 6) * 45 - 50;
const B = (n) => Math.cos(n / 6) * 30 + 40;
const FARB = (n) => Math.cos(n / 6) * 5 + 1005;
/** The lowest and the highest value of the series, or of the tops of their stack. */
function ends(fns, stack = false) {
  let lo = Infinity;
  let hi = -Infinity;
  for (let n = 0; n < N; n++) {
    let acc = 0;
    for (const f of fns) {
      const v = stack ? (acc += f(n)) : f(n);
      lo = Math.min(lo, v);
      hi = Math.max(hi, v);
    }
  }
  return [lo, hi];
}
const none = { min: null, max: null, softMin: null, softMax: null };
const scale = (set) => ({ ...none, ...set });
/** The axis of the hook of a time series scale for the plot height `h`, with the data ends `raw` (E30). */
function axisOf(set, raw, h, percent = false) {
  const { hook } = scaleAxis({ ends: scale(set), binary: false, tickText: tickLabel, plainTick: tickLabel, percent, log: false });
  return hook(0, 1, h, raw);
}
const range = (ax) => [ax.min, ax.max];
const labels = (ax) => ax.ticks.map((t) => t.label);

test("ID-K311b: bars use the automatic range of Grafana, and do not start at 0 when the data stays far from 0", () => {
  assert.deepEqual(autoYRange(...ends([A])), [0, 104]); // the bar axis of before gave 0 to 104.5
  assert.deepEqual(autoYRange(...ends([NP])), [-70, 100]); // before: -63 to 93
  assert.deepEqual(autoYRange(...ends([FAR])), [999, 1011]); // before: 0 to 1,111
  assert.deepEqual(autoYRange(...ends([NFAR])), [-1011, -999]);
  // The ticks that Grafana draws: h8 (plot 209 px) -50 to 100 by 25, h12 (361 px) 999 to 1,011 by 1, h24 (817 px) by 0.5.
  assert.deepEqual(labels(axisOf({}, ends([NP]), 209)), ["-50", "-25", "0", "25", "50", "75", "100"]);
  assert.deepEqual(labels(axisOf({}, ends([FAR]), 361)), ["999", "1000", "1001", "1002", "1003", "1004", "1005", "1006", "1007", "1008", "1009", "1010", "1011"]);
  assert.equal(axisOf({}, ends([FAR]), 817).step, 0.5);
  assert.deepEqual(range(axisOf({}, ends([NFAR]), 133)), [-1011, -999]);
});

test("ID-K311c: a stack uses the automatic range of its tops, with the tick rule of 15 and 30 px", () => {
  assert.deepEqual(autoYRange(...ends([A, B], true)), [0, 160]); // the engine gave 0 to 152.7
  assert.deepEqual(autoYRange(...ends([FAR, FARB], true)), [800, 2200]); // the base of the stack does not count
  // h4 (57 px) 0 to 150 by 50; h6 (133 px) by 20; h8 (209 px) by 25; h24 (817 px) by 10.
  assert.deepEqual(labels(axisOf({}, ends([A, B], true), 57)), ["0", "50", "100", "150"]);
  assert.deepEqual([57, 133, 209, 361, 817].map((h) => axisOf({}, ends([A, B], true), h).step), [50, 20, 25, 20, 10]);
  assert.deepEqual(labels(axisOf({}, ends([FAR, FARB], true), 133)), ["800", "1000", "1200", "1400", "1600", "1800", "2000", "2200"]);
  assert.deepEqual([57, 209, 817].map((h) => axisOf({}, ends([FAR, FARB], true), h).step), [500, 250, 100]);
});

test("ID-K311c: a percent stack goes from 0 to 100 % with percent labels, for each unit", () => {
  const p57 = axisOf({}, [5, 100], 57, true);
  assert.deepEqual([range(p57), labels(p57)], [[0, 100], ["0%", "50%", "100%"]]);
  assert.deepEqual(labels(axisOf({}, [5, 100], 133, true)), ["0%", "20%", "40%", "60%", "80%", "100%"]);
  assert.equal(axisOf({}, [5, 100], 361, true).step, 10);
  const p817 = axisOf({}, [5, 100], 817, true);
  assert.deepEqual([p817.step, p817.ticks.length, labels(p817)[1]], [5, 21, "5%"]);
  // 0 and 100 % are soft ends (ID-K321, measure of 2026-11-21): a group below 0 (tops -100 % to 100 %) gives -120 % to 100 %.
  assert.deepEqual(range(axisOf({}, [-100, 100], 300, true)), [-120, 100]);
  const { hook } = scaleAxis({ ends: none, binary: false, tickText: (v) => `${v} ms`, plainTick: tickLabel, percent: true, log: false });
  assert.equal(hook(0, 100, 57, [5, 100]).ticks[1].label, "50%");
});

test("ID-K311d: a soft end is the end of the axis when the data stays inside it, and it never cuts the data", () => {
  const data = ends([A]);
  assert.deepEqual(autoYRange(...data, scale({ softMax: 200 })), [0, 200]); // before: 0 to 220
  assert.deepEqual(autoYRange(...data, scale({ softMax: 100 })), [0, 100]); // a soft end in the room: no room above it
  assert.deepEqual(autoYRange(...data, scale({ softMax: 50 })), [0, 104]); // inside the data: no effect
  assert.deepEqual(autoYRange(...data, scale({ softMin: -50 })), [-50, 104]);
  assert.deepEqual(autoYRange(...data, scale({ softMin: 10 })), [-4, 104]); // inside the data: the room crosses 0
  assert.deepEqual(autoYRange(...data, scale({ softMin: 4 })), [4, 104]);
  assert.deepEqual(autoYRange(...ends([NP]), scale({ softMin: -100, softMax: 200 })), [-100, 200]);
  assert.deepEqual(autoYRange(...ends([NEG]), scale({ softMax: -50 })), [-104, 4]);
  // The ticks of Grafana: h24 (817 px) 0 to 200 by 10, h6 (133 px) 0 to 200 by 25, h12 (361 px) with soft_min 10: 0 to 100 by 10.
  const s817 = axisOf({ softMax: 200 }, data, 817);
  assert.deepEqual([s817.step, s817.ticks.length, s817.ticks.at(-1).v], [10, 21, 200]);
  assert.deepEqual(labels(axisOf({ softMax: 200 }, data, 133)), ["0", "25", "50", "75", "100", "125", "150", "175", "200"]);
  const s361 = axisOf({ softMin: 10 }, data, 361);
  assert.deepEqual([range(s361), s361.ticks[0].v, s361.step], [[-4, 104], 0, 10]);
});

test("the hard ends follow Grafana: alone on a side, the end and the data end of that side; with a soft end, a limit", () => {
  const data = ends([A]);
  assert.deepEqual(autoYRange(...data, scale({ min: 0 })), [0, 105]); // before: 0 to 104
  assert.deepEqual(autoYRange(...data, scale({ max: 80 })), [0, 80]);
  assert.deepEqual(autoYRange(...data, scale({ min: 0, max: 100 })), [0, 100]);
  assert.deepEqual(autoYRange(...data, scale({ min: -50, softMin: 0 })), [0, 104]); // before: -50 to 105
  assert.deepEqual(autoYRange(...data, scale({ min: 50, softMin: 0, softMax: 400 })), [50, 400]);
  assert.deepEqual(axisOf({ min: 0 }, data, 817).ticks.at(-1).v, 105);
  // Without ends, the range is the one of before (the histogram and the xy chart call autoYRange with two arguments).
  assert.deepEqual(autoYRange(16, 201), [0, 220]);
  assert.deepEqual(autoYRange(0, 0), [0, 1]);
});

test("a hook with no data ends from the engine reads the first two arguments", () => {
  const { hook } = scaleAxis({ ends: none, binary: false, tickText: tickLabel, plainTick: tickLabel, percent: false, log: false });
  assert.deepEqual(range(hook(5, 95, 300, null)), [0, 104]);
  assert.equal(scaleAxis({ ends: none, binary: false, tickText: tickLabel, plainTick: tickLabel, percent: false, log: true }).hook, undefined);
});

// ---- The engine on a fake DOM ----

const WIDTH = 600;
// The width of a text in the fake: 6.5 px a character, 3 px for i, l and 1, 10 px for m and W, 12 px for the ellipsis.
const widthOf = (s) => [...String(s)].reduce((w, c) => w + (c === "…" ? 12 : "il1".includes(c) ? 3 : "mW".includes(c) ? 10 : 6.5), 0);
let measures = [];
let fills = [];

function fakeContext() {
  const state = {};
  const methods = {
    measureText: (s) => { measures.push(String(s)); return { width: widthOf(s) }; },
    fillText: (s) => { fills.push(String(s)); },
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

globalThis.document = { createElement: () => fakeElement(200), documentElement: fakeElement(200), addEventListener() {}, removeEventListener() {}, fonts: null };
globalThis.window = { devicePixelRatio: 1, matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }), getSelection: () => null };
globalThis.MutationObserver = class { observe() {} disconnect() {} };
globalThis.ResizeObserver = class { observe() {} disconnect() {} };
globalThis.getComputedStyle = () => ({ color: "rgb(204, 204, 220)", fontFamily: "Inter", getPropertyValue: () => "" });
globalThis.requestAnimationFrame = () => 0;

const { chartCore } = await import("../../web/js/vendor/chdash/app_chart_core.js");

const xs = Float64Array.from({ length: N }, (_, i) => 1794528000000 + i * 300000);
const series = (id, f) => ({ id, label: id, color: "#f00", values: Float64Array.from(xs, (_, i) => f(i)), nulls: null });

/** Draw a chart once and give the arguments of each call of the yAxis hooks, by scale. */
function hookCalls(extra) {
  const calls = { y: [], y2: [] };
  const spy = (key) => (dataMin, dataMax, plotH, raw) => { calls[key].push({ dataMin, dataMax, plotH, raw }); return null; };
  const y2 = extra.y2 ? { ...extra.y2, yAxis: spy("y2") } : undefined;
  const chart = chartCore.create(fakeElement(200), { height: 200, xKind: "time", xs, type: "line", ...extra, yAxis: spy("y"), y2 });
  chart.flush();
  chart.destroy();
  return calls;
}
const close = (a, b) => Math.abs(a - b) < 1e-9;

test("E30: the hook gets the tops of a stack as its data ends, without the base", () => {
  const calls = hookCalls({ type: "area", stack: true, series: [series("a", FAR), series("b", FARB)] });
  const last = calls.y.at(-1);
  const [lo, hi] = ends([FAR, FARB], true);
  assert.ok(last.raw && close(last.raw[0], lo) && close(last.raw[1], hi), `raw ${last.raw}`);
  // The first two arguments are as before: the base of the stack (0) counts there.
  assert.equal(last.dataMin, 0);
  assert.ok(close(last.dataMax, hi));
});

test("E30: the soft ends do not enter the data ends of the hook, and the right scale gets its own data ends", () => {
  const calls = hookCalls({ series: [series("a", A), series("b", FAR)], ySoft: [-50, 300], scales: { b: "y2" }, y2: { shown: true, ySoft: [null, 5000] } });
  const y = calls.y.at(-1);
  const y2 = calls.y2.at(-1);
  const [lo, hi] = ends([A]);
  assert.deepEqual([y.dataMin, y.dataMax], [-50, 300]);
  assert.ok(close(y.raw[0], lo) && close(y.raw[1], hi), `left raw ${y.raw}`);
  const [lo2, hi2] = ends([FAR]);
  assert.equal(y2.dataMax, 5000);
  assert.ok(close(y2.raw[0], lo2) && close(y2.raw[1], hi2), `right raw ${y2.raw}`);
  // A negated series gives its drawn values.
  const neg = hookCalls({ series: [{ ...series("a", A), negate: true }] }).y.at(-1);
  assert.ok(close(neg.raw[0], -hi) && close(neg.raw[1], -lo), `negated raw ${neg.raw}`);
});

test("E30: the data ends are null when no series shows", () => {
  const calls = hookCalls({ series: [series("a", A)], hidden: ["a"] });
  assert.equal(calls.y.at(-1).raw, null);
});

/** The title that the engine draws for a y axis, the measures of its texts, and the plot height. */
function drawTitle(title, height = 200) {
  measures = [];
  fills = [];
  const chart = chartCore.create(fakeElement(height), { height, xKind: "time", xs, type: "line", series: [series("a", A)], yTitle: title });
  chart.flush();
  const plotH = Number(chart.root.dataset.plot.split(" ")[3]);
  chart.destroy();
  const drawn = fills.filter((s) => s === title || s.endsWith("…"));
  return { drawn: drawn.at(-1), titleMeasures: measures.filter((s) => s.length > 40 || s === title || s.endsWith("…")).length, plotH };
}

// The cut of before (upstream): one character at a time, one measure each time.
function oldFit(text, maxW) {
  const t = String(text);
  if (widthOf(t) <= maxW) return t;
  let cut = t;
  while (cut.length > 1 && widthOf(`${cut}…`) > maxW) cut = cut.slice(0, -1);
  return `${cut}…`;
}

test("E31: a title of 40,000 characters takes 20 measures or fewer and draws a cut of 256 characters at most", () => {
  const long = "W".repeat(20000) + "i".repeat(20000);
  const { drawn, titleMeasures } = drawTitle(long);
  assert.ok(titleMeasures <= 20, `measures of the title: ${titleMeasures}`);
  assert.ok(drawn.endsWith("…") && drawn.length <= 257, `drawn: ${drawn.length} characters`);
  // The upstream cut would measure one text for each character that it takes away.
  assert.ok(titleMeasures * 1000 < long.length);
});

test("E31: the cut of a normal title equals the cut of before", () => {
  const titles = ["Latency", "Requests per second", "lim", "W", "mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm", "iiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiii",
    "The latency of the requests of the service, by host and by region, in milliseconds", "x".repeat(255), "x".repeat(256), "Wi".repeat(90)];
  let cut = 0;
  for (const height of [90, 200, 420]) {
    for (const title of titles) {
      const { drawn, plotH } = drawTitle(title, height);
      assert.equal(drawn, oldFit(title, plotH), `"${title.slice(0, 20)}" (${title.length}) on ${plotH} px`);
      if (drawn !== title) cut += 1;
    }
  }
  assert.ok(cut >= 10, `the test cuts titles (${cut})`);
});
