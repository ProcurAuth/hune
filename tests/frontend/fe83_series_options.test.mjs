// FE-83: the gradient modes (wave H, item P10) and the cosmetic series options (item P14) of a time series and a bar chart (4.18, 4.28, 4.30,
// 9.19, engine edits E43 to E46 of 9.24). The expected values are those of Grafana 12.4.2 in `vectors/FE-83-series-options.json`, measured on
// 2026-11-22 with probes of the canvas calls. The pure rules run first. Then the engine and the time series run on a small fake of the DOM and
// of the canvas, which records each fill, each stroke, each text and each gradient.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { autoYRange, tickLabel } from "../../web/js/panels/series.js";
import { cssGradient, hueStops, partGradient, rgbaText, scaleGradient, schemeRange, seriesPaint } from "../../web/js/panels/gradient.js";
import { BAR_ALIGN, centredEnds, dashOf, joinShortGaps, log2Axis } from "../../web/js/panels/series_opts.js";

const V = JSON.parse(readFileSync(new URL("./vectors/FE-83-series-options.json", import.meta.url), "utf8"));
const hex = (c) => `#${[c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, "0")).join("")}`;
const rgbOf = (h) => ({ r: parseInt(h.slice(1, 3), 16), g: parseInt(h.slice(3, 5), 16), b: parseInt(h.slice(5, 7), 16), a: 1 });
const parts = (css) => (css.match(/[\d.]+/g) || []).map(Number);
const near = (a, b, tol, what) => assert.ok(Math.abs(a - b) <= tol, `${what}: ${a} is not ${b} (tolerance ${tol})`);

// ---- The pure rules ----

test("FE-83 hue: the two colours of Grafana 12.4.2 in both themes (tinycolor2 steps, to the last digit)", () => {
  for (const h of V.hue) {
    for (const theme of ["dark", "light"]) {
      const got = hueStops(rgbOf(h.color), theme === "dark").map(hex);
      assert.deepEqual(got, h[theme], `${h.color} ${theme}`);
    }
  }
});

test("FE-83 gradients: the place, the offsets and the colours of each stop, for lines, areas and bars", () => {
  for (const g of V.gradients) {
    const k = 1;
    const scale = g.scale;
    const [top, bottom] = g.ends;
    const pos = (v) => Math.round((bottom - ((v - scale[0]) / (scale[1] - scale[0])) * (bottom - top)) * k) / k;
    const got = partGradient(g.mode, g.part, {
      c: rgbOf(g.color), alpha: g.alpha, dark: g.dark, config: g.config || { color: { mode: "fixed", fixed: g.color }, set: {} }, text: null,
      ends: g.ends, scale, raw: g.raw || null, pos,
    });
    if (g.plain) {
      assert.equal(got, null, `${g.name}: the plain colour`);
      continue;
    }
    assert.ok(got && typeof got === "object", `${g.name}: a gradient`);
    assert.equal(got.from, g.from, `${g.name}: the start`);
    assert.equal(got.to, g.to, `${g.name}: the end`);
    assert.equal(got.stops.length, g.stops.length, `${g.name}: the number of stops`);
    got.stops.forEach(([f, css], i) => {
      near(f, g.stops[i][0], 0.005, `${g.name}: offset ${i}`);
      const [r, gg, b, a] = parts(css);
      assert.deepEqual([r, gg, b], g.stops[i][1].slice(0, 3), `${g.name}: the colour of stop ${i}`);
      near(a, g.stops[i][1][3], 0.002, `${g.name}: the alpha of stop ${i}`);
    });
  }
});

test("FE-83 gradients of horizontal bars: one CSS gradient from the right edge to the left edge of the track", () => {
  for (const h of V.horizontal) {
    const g = partGradient(h.mode, "fill", { c: rgbOf(h.color), alpha: h.alpha, dark: true, config: null, text: null, ends: h.ends, scale: [0, 1], raw: null, pos: (v) => v });
    assert.equal(cssGradient(g, 500), h.css, h.name);
  }
});

test("FE-83 scheme: the range, the scale gradient and its limits", () => {
  // min, else soft_min, else the drawn data; one value gives the ends of the scale.
  assert.deepEqual(schemeRange({}, [10, 90], [0, 100]), [10, 90]);
  assert.deepEqual(schemeRange({ min: 20 }, [10, 90], [0, 100]), [20, 90]);
  assert.deepEqual(schemeRange({ soft_max: 200 }, [10, 90], [0, 220]), [10, 200]);
  assert.deepEqual(schemeRange({}, [5, 5], [0, 10]), [0, 10]);
  assert.deepEqual(schemeRange({}, null, [0, 10]), [0, 10]);
  // One stop is a plain colour. A step above the scale still ends the gradient, as in uPlot. A scale with no height gives the last stop.
  assert.equal(scaleGradient([[-Infinity, "a"]], true, 0, 100, (v) => 100 - v), "a");
  assert.deepEqual(scaleGradient([[-Infinity, "a"], [500, "b"]], true, 0, 100, (v) => 100 - v), { from: 100, to: -400, stops: [[0, "a"], [1, "a"], [1, "b"]] });
  assert.equal(scaleGradient([[-Infinity, "a"], [50, "b"]], true, 0, 100, () => 7), "b");
  const g = scaleGradient([[-Infinity, "a"], [50, "b"], [80, "c"]], true, 0, 100, (v) => 100 - v);
  assert.deepEqual(g, { from: 100, to: 20, stops: [[0, "a"], [0.625, "a"], [0.625, "b"], [1, "b"], [1, "c"]] });
  assert.equal(rgbaText({ r: 1.4, g: 2.6, b: 3 }, 1.5), "rgba(1, 3, 3, 1)");
});

test("FE-83 log base 2: the range, the grid lines and the labels of Grafana 12.4.2", async () => {
  const { yTickLabeller } = await import("../../web/js/panels/common.js");
  for (const c of V.log2) {
    const label = c.unit ? yTickLabeller({ config: { unit: c.unit, decimals: null, set: {} } }) : tickLabel;
    const ax = log2Axis(c.lo, c.hi, c.ends || null, c.plotH, label);
    assert.equal(ax.min, c.min, `${c.name}: min`);
    assert.equal(ax.max, c.max, `${c.name}: max`);
    assert.equal(ax.ticks.length, c.grid, `${c.name}: one grid line for each power of 2`);
    assert.deepEqual(ax.ticks.map((t) => t.label).filter((l) => l !== ""), c.labels, `${c.name}: the labels`);
    assert.ok(ax.ticks.every((t, i) => i === 0 || t.v === ax.ticks[i - 1].v * 2), `${c.name}: the powers of 2 in order`);
  }
  // No value above 0 gives 1 to 2. A plot of 150 px or less keeps 15 px between two labels (uPlot keeps a label when the space is not less
  // than 15 px). `decimals` = 0 starts the range at 1.
  assert.deepEqual([log2Axis(null, null, null, 171, tickLabel).min, log2Axis(null, null, null, 171, tickLabel).max], [1, 2]);
  assert.equal(log2Axis(1, 1024, null, 150, tickLabel).ticks.filter((t) => t.label).length, 11);
  assert.equal(log2Axis(1, 1024, null, 140, tickLabel).ticks.filter((t) => t.label).length, 6);
  assert.equal(log2Axis(0.1, 1024, null, 171, tickLabel, true).min, 1);
  // The widest range of doubles stays short: the grid keeps the labelled powers only.
  const wide = log2Axis(Number.MIN_VALUE, Number.MAX_VALUE, null, 400, tickLabel);
  assert.ok(Number.isFinite(wide.max) && wide.ticks.length <= 400 && wide.ticks.every((t) => t.label !== ""), `${wide.ticks.length} ticks`);
});

test("FE-83 null threshold: a run of nulls joins when the gap is less than the threshold (strict)", () => {
  const t = V.nullThreshold;
  const xs = Float64Array.from({ length: t.n }, (_, i) => i * t.step);
  const nulls = new Uint8Array(t.n);
  for (const i of t.nulls) nulls[i] = 1;
  for (const c of t.cases) {
    const out = joinShortGaps(xs, nulls, c.ms);
    const joined = t.nulls.filter((i) => out[i] === 0);
    assert.deepEqual(joined, c.joined, `${c.ms} ms`);
  }
  // A run at the start or at the end stays a break; no threshold keeps the flags.
  const edge = Uint8Array.from([1, 0, 0, 1]);
  assert.deepEqual([...joinShortGaps([0, 1, 2, 3], edge, 1e12)], [1, 0, 0, 1]);
  assert.equal(joinShortGaps(xs, nulls, undefined), nulls);
});

test("FE-83 centred zero: the range of Grafana 12.4.2 from the ends [-m, m]", () => {
  for (const c of V.centredZero) {
    const [lo, hi] = centredEnds(c.lo, c.hi);
    const ends = { min: null, max: null, softMin: null, softMax: null, ...(c.ends || {}) };
    if (c.bar) {
      ends.softMin = 0;
      ends.softMax = 0;
    }
    assert.deepEqual(autoYRange(lo, hi, ends), c.range, c.name);
  }
});

test("FE-83 dash: the array of the set, the defaults of dash and dot, and the cap", () => {
  for (const d of V.dash) assert.deepEqual(dashOf(d.set), { dash: d.dash, cap: d.cap }, JSON.stringify(d.set));
  assert.deepEqual(BAR_ALIGN, { before: -1, center: 0, after: 1 });
});

// ---- The engine on a fake DOM (E43 to E46) ----

const WIDTH = 499;
let ops = [];

function fakeGradient(args) {
  return { args, stops: [], addColorStop(f, css) { this.stops.push([f, css]); } };
}

function fakeContext(dpr) {
  const state = { path: [], rects: [], dash: [] };
  const style = (v) => (v && typeof v === "object" && v.stops ? { g: v.args, stops: v.stops } : String(v));
  const methods = {
    measureText: (s) => ({ width: 6.5 * String(s).length }),
    beginPath: () => { state.path = []; state.rects = []; },
    moveTo: (x, y) => state.path.push(["M", x, y]),
    lineTo: (x, y) => state.path.push(["L", x, y]),
    rect: (x, y, w, h) => state.rects.push([x, y, w, h]),
    setLineDash: (d) => { state.dash = d.slice(); },
    getTransform: () => ({ a: dpr }),
    fill: () => ops.push({ k: "fill", style: style(state.fillStyle), rects: state.rects.slice(), path: state.path.slice() }),
    stroke: () => ops.push({ k: "stroke", style: style(state.strokeStyle), width: state.lineWidth, dash: state.dash.slice(), cap: state.lineCap, rects: state.rects.slice(), path: state.path.slice() }),
    fillText: (text, x, y) => ops.push({ k: "text", text: String(text), x, y, style: style(state.fillStyle), align: state.textAlign }),
    createLinearGradient: (...a) => fakeGradient(a),
  };
  return new Proxy(state, {
    get: (t, k) => (k in methods ? methods[k] : k in t ? t[k] : () => undefined),
    set: (t, k, v) => { t[k] = v; return true; },
  });
}

function fakeElement(height, dpr = 1) {
  const kids = new Map();
  const own = { style: { setProperty() {} }, classList: { add() {}, remove() {}, contains: () => false, toggle() {} }, dataset: {}, children: [], hidden: false };
  return new Proxy(own, {
    get(t, k) {
      if (k in t) return t[k];
      if (typeof k === "symbol") return undefined;
      if (k === "getContext") return () => (t.ctx ??= fakeContext(dpr));
      if (k === "getBoundingClientRect") return () => ({ width: WIDTH, height, left: 0, top: 0, right: WIDTH, bottom: height });
      if (k === "querySelector") return (sel) => (kids.has(sel) ? kids.get(sel) : kids.set(sel, fakeElement(height, dpr)).get(sel));
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
globalThis.getComputedStyle = (el) => ({ color: /^#/.test(el?.style?.color ?? "") ? el.style.color : "rgb(204, 204, 220)", fontFamily: "Inter", getPropertyValue: () => "" });
globalThis.requestAnimationFrame = () => 0;

const { chartCore } = await import("../../web/js/vendor/chdash/app_chart_core.js");

const N = 6;
const XS = Float64Array.from({ length: N }, (_, i) => 1767225600000 + i * 600000);
const red = "#f2495c";

/** One chart drawn once with the options `extra`. Gives the plot box [left, top, w, h] and the records. */
function draw(extra, { height = 200, dpr = 1, values = [10, 20, 30, 40, 50, 60] } = {}) {
  ops = [];
  window.devicePixelRatio = dpr;
  const series = [{ id: "0", label: "v", color: red, values: Float64Array.from(values), nulls: null, group: "all", ...(extra.series || {}) }];
  const chart = chartCore.create(fakeElement(height, dpr), { height, xKind: "time", xs: XS, xDomain: [XS[0], XS[0] + 3600000], series, type: "line", ...extra, series });
  chart.flush();
  const plot = chart.root.dataset.plot.split(" ").map(Number);
  chart.destroy();
  window.devicePixelRatio = 1;
  return { plot, ops: ops.slice() };
}

test("FE-83 E43: the paint function of a series gives the style of its fill and of its line (area, line, bars)", () => {
  const seen = [];
  const paint = (part, ctx, L, c, alpha) => {
    seen.push([part, +alpha.toFixed(3), [c.r, c.g, c.b]]);
    const g = ctx.createLinearGradient(0, L.top, 0, L.top + L.plotH);
    g.addColorStop(0, `${part}-top`);
    return g;
  };
  for (const type of ["area", "line", "bar"]) {
    seen.length = 0;
    const { plot, ops: o } = draw({ type, series: { paint, fill: 0.5 }, barFillAlpha: 0.61, barLineWidth: 1 });
    const fills = o.filter((x) => x.k === "fill" && x.style.g);
    const lines = o.filter((x) => x.k === "stroke" && x.style.g);
    assert.ok(fills.length >= 1, `${type}: the fill takes the paint`);
    assert.ok(lines.length >= 1, `${type}: the line takes the paint`);
    assert.deepEqual(fills[0].style.g, [0, plot[1], 0, plot[1] + plot[3]], `${type}: the gradient runs over the plot`);
    assert.ok(seen.some(([p, a, rgb]) => p === "fill" && a === 0.5 && rgb.join() === "242,73,92"), `${type}: the fill alpha of the series and the colour: ${JSON.stringify(seen)}`);
    assert.ok(seen.some(([p, a]) => p === "line" && a === 1), `${type}: the line alpha`);
  }
  // A paint that gives null keeps the plain colour.
  const { ops: plain } = draw({ type: "area", series: { paint: () => null, fill: 0.5 } });
  assert.ok(plain.some((x) => x.k === "fill" && x.style === "rgba(242, 73, 92, 0.5)"), "the plain fill");
});

test("FE-83 E43 through the time series paint: hue and scheme on a real chart", () => {
  const env = { config: { color: { mode: "thresholds", fixed: null }, thresholds: { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 40, color: "red" }] }, set: {} }, dark: () => true, text: () => null, raw: () => [10, 60] };
  const { plot, ops: o } = draw({ type: "area", series: { paint: seriesPaint("scheme", env), fill: 0.3 } });
  const fill = o.find((x) => x.k === "fill" && x.style.g);
  assert.ok(fill, "a scheme fill");
  assert.equal(fill.style.g[1], plot[1] + plot[3], "from the bottom of the scale (the first step starts at the scale minimum)");
  assert.deepEqual(fill.style.stops.map((s) => parts(s[1]).slice(0, 3).join()), ["115,191,105", "115,191,105", "242,73,92"], "the bands of the thresholds");
  const hue = draw({ type: "line", series: { paint: seriesPaint("hue", env) } });
  const line = hue.ops.find((x) => x.k === "stroke" && x.style.g);
  assert.deepEqual(line.style.stops.map((s) => s[1]), ["rgba(255, 166, 136, 1)", "rgba(223, 16, 125, 1)"], "the hue line of #F2495C");
});

test("FE-83 E44: bar alignment before, center and after, with the line width (Grafana 12.4.2)", () => {
  const one = (barAlign, lw) => {
    const { ops: o } = draw({ type: "bar", barAlign, barLineWidth: lw, barFillAlpha: 0.5 });
    const f = o.find((x) => x.k === "fill" && x.rects.length);
    return f.rects[1];
  };
  for (const lw of [0, 1, 3]) {
    const center = one(0, lw);
    const mid = center[0] + center[2] / 2;
    const before = one(-1, lw);
    const after = one(1, lw);
    near(before[0] + before[2], mid - lw / 2, 1e-6, `before, lw ${lw}: the bar ends half the line before its time`);
    near(after[0], mid + lw / 2, 1e-6, `after, lw ${lw}: the bar starts half the line after its time`);
    near(before[2], center[2], 1e-9, "the width does not change");
  }
  for (const c of V.barAlignment.cases) {
    const left = c.x - c.w / 2 + BAR_ALIGN[c.align] * (c.w / 2 + c.lw / 2);
    near(left, c.left, 0.5, `${c.align} w ${c.w} lw ${c.lw}: the rule gives the edge of Grafana within its rounding`);
  }
});

test("FE-83 E45: the dash array is in device pixels, as Grafana draws it", () => {
  for (const dpr of [1, 2]) {
    const { ops: o } = draw({ series: { dash: [10, 20], dashCap: "butt", width: 2 } }, { dpr });
    const s = o.find((x) => x.k === "stroke" && x.dash.length);
    assert.deepEqual(s.dash, [10 / dpr, 20 / dpr], `dpr ${dpr}`);
  }
});

test("FE-83 E46: the width, the border and the colour of a y axis (Grafana 12.4.2)", () => {
  for (const w of V.axes.width) {
    const extra = w.side === "left" ? { yWidth: w.width, yTitle: w.title ? "Rows" : undefined } : { yShown: false, scales: { 0: "y2" }, y2: { yWidth: w.width, shown: true } };
    const { plot } = draw(extra);
    const edge = w.side === "left" ? plot[0] : WIDTH - (plot[0] + plot[2]);
    assert.equal(edge, w.plotEdge, `${w.side} width ${w.width}${w.title ? " with a title" : ""}`);
  }
  const plain = draw({});
  const labels = (o) => o.ops.filter((x) => x.k === "text" && x.align === "right");
  assert.equal(labels(plain)[0].x, plain.plot[0] - V.axes.labelGap, "the labels end 5 px before the plot");
  const border = draw({ yBorder: true, yColor: red });
  const left = border.plot[0];
  const top = border.plot[1];
  const bottom = top + border.plot[3];
  assert.equal(labels(border)[0].x, left - V.axes.labelGapBorder, "with a border, the labels end 9 px before the plot");
  assert.equal(border.plot[0], plain.plot[0], "the border does not change the gutter");
  assert.ok(labels(border).every((t) => t.style === "rgba(242, 73, 92, 1)"), "the labels have the colour of the axis");
  const strokes = border.ops.filter((x) => x.k === "stroke" && x.style === "rgba(242, 73, 92, 1)" && x.width === 1);
  const segs = strokes.flatMap((s) => s.path.reduce((acc, p, i) => (p[0] === "L" ? [...acc, [s.path[i - 1], p]] : acc), []));
  assert.ok(segs.some(([a, b]) => Math.abs(a[1] - b[1]) < 1 && Math.abs(a[2] - top) <= 1 && Math.abs(b[2] - bottom) <= 1), "a line on the edge of the plot from its top to its bottom");
  const ticks = segs.filter(([a, b]) => Math.abs(a[2] - b[2]) < 1e-9 && Math.abs(Math.abs(a[1] - b[1]) - V.axes.tick) < 0.6);
  assert.equal(ticks.length, labels(border).length, "a tick of 4 px outside the plot at each label");
});

// ---- The rows of the editor (10.3h) ----

test("FE-83 editor: the rows of P10 and P14 write their optional members and follow the rules of display", async () => {
  const { TABLES } = await import("../../web/js/editor/forms/options/index.js");
  const { writeOption, shownIds } = await import("../../web/js/editor/forms/options/rows.js");
  const { parseDash } = await import("../../web/js/editor/forms/options/timeseries.js");
  const { checkProperty, PROPERTY_IDS } = await import("../../web/js/editor/props.js");
  const ts = { id: 1, type: "timeseries", title: "t", options: { draw: "line", interpolation: "linear", line_width: 1, fill_opacity: 0, show_points: "auto", point_size: 5, span_nulls: false, stacking: "none", y_scale: "linear", legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } }, fields: { defaults: {}, overrides: [] }, links: [] };
  const row = (type, id) => TABLES[type].rows.find((r) => r.id === id);
  const w = (panel, type, id, v) => writeOption(panel, row(type, id), v);
  // An optional member at its absent value leaves the panel; another value is written in the order of 4.38.
  assert.equal(w(ts, "timeseries", "gradient-mode", "hue").options.gradient_mode, "hue");
  assert.ok(!("gradient_mode" in w(w(ts, "timeseries", "gradient-mode", "hue"), "timeseries", "gradient-mode", "none").options));
  const all = ["gradient-mode", "log-base", "span-nulls-ms", "bar-alignment"].reduce((p, id, i) => w(p, "timeseries", id, ["scheme", 2, 150000, "after"][i]), ts);
  assert.deepEqual(Object.keys(all.options).slice(-4), ["gradient_mode", "log_base", "span_nulls_ms", "bar_alignment"]);
  assert.ok(!("log_base" in w(all, "timeseries", "log-base", 10).options), "10 is the absent value");
  assert.ok(!("span_nulls_ms" in w(all, "timeseries", "span-nulls-ms", null).options), "an empty field removes the threshold");
  const look = ["axis-width", "axis-color", "axis-border", "axis-centered-zero", "dash-array"].reduce((p, id, i) => w(p, "timeseries", id, [80, "series", true, true, [10, 20]][i]), ts);
  assert.deepEqual(look.fields.defaults, { line_dash_array: [10, 20], axis_centered_zero: true, axis_width: 80, axis_border: true, axis_color: "series" });
  assert.deepEqual(Object.keys(look.fields.defaults), ["line_dash_array", "axis_centered_zero", "axis_width", "axis_border", "axis_color"], "the order of 4.18");
  assert.deepEqual(w(look, "timeseries", "axis-color", "text").fields.defaults.axis_color, undefined, "text is the absent value");
  // The rules of display.
  const shown = (o) => shownIds(TABLES.timeseries, o);
  assert.ok(shown(ts.options).includes("gradient-mode") && shown(ts.options).includes("span-nulls-ms") && !shown(ts.options).includes("log-base"));
  assert.ok(shown({ ...ts.options, y_scale: "log" }).includes("log-base") && !shown({ ...ts.options, y_scale: "log" }).includes("axis-centered-zero"));
  assert.ok(!shown({ ...ts.options, span_nulls: true }).includes("span-nulls-ms"));
  assert.ok(shown({ ...ts.options, draw: "bars" }).includes("bar-alignment") && !shown(ts.options).includes("bar-alignment"));
  assert.ok(!shown({ ...ts.options, draw: "bars" }).includes("dash-array"));
  assert.ok(TABLES.barchart.rows.some((r) => r.id === "gradient-mode") && TABLES.barchart.rows.some((r) => r.id === "axis-centered-zero" && !r.when));
  // The parse of a dash array, and the check of the property of an override.
  assert.deepEqual(parseDash("10, 20"), { value: [10, 20] });
  assert.deepEqual(parseDash(" 30,3 , 3 "), { value: [30, 3, 3] });
  assert.deepEqual(parseDash(""), { value: null });
  for (const bad of ["0, 0", "10, 101", "1,2,3,4,5,6,7,8,9", "10 20", "10, -1", "1.5", "010"]) assert.ok(parseDash(bad).error, bad);
  assert.equal(checkProperty("line_dash_array", [10, 20]), "");
  assert.ok(checkProperty("line_dash_array", [0, 0]) && checkProperty("line_dash_array", []) && checkProperty("axis_width", 501));
  assert.equal(checkProperty("axis_color", "series"), "");
  assert.deepEqual(PROPERTY_IDS.slice(-5), ["line_dash_array", "axis_centered_zero", "axis_width", "axis_border", "axis_color"]);
});

test("FE-83 the dash of an override replaces the dash and the array of the defaults (4.18, 9.14)", async () => {
  const { computeFieldConfig } = await import("../../web/js/fields.js");
  const field = { name: "v", kind: "number", values: [1, 2], nulls: null, config: {}, labels: null };
  const spec = (set) => ({ defaults: { line_dash: "dash", line_dash_array: [20, 10] }, overrides: [{ match: { by: "name", value: "v" }, set }] });
  const setOf = (s) => computeFieldConfig([field], spec(s), "timeseries").fields[0].config.set;
  assert.deepEqual(dashOf(setOf({ line_dash: "dot" })), { dash: [0, 10], cap: "round" }, "a dot with no array has the default array of a dot");
  assert.deepEqual(dashOf(setOf({ line_dash_array: [5, 5] })), { dash: [5, 5], cap: "butt" }, "an array with no style draws dashes");
  assert.deepEqual(dashOf(setOf({ unit: "ms" })), { dash: [20, 10], cap: "butt" }, "another member keeps the dash of the defaults");
});
