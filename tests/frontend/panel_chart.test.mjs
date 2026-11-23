// SPDX-License-Identifier: MIT OR Apache-2.0
// The chart panels (9.19, 9.24): the helpers of web/js/panels and the rules of web/css/panel-chart.css.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { fallbackColor, lastFinite } from "../../web/js/panels/series.js";
import { fitHeight, hiddenIds, hideFlags, legendCalc, legendFormatter, legendOptions, overlapFill, timeFractionDigits, tooltipFormat, trimZeros } from "../../web/js/panels/common.js";
import { styleParts, thresholdShapes, stepValues } from "../../web/js/panels/thresholds.js";
import { markColor, markCss, NAMED, relativeLuminance } from "../../web/js/colors.js";
import { sigDecimals } from "../../web/js/units.js";
import { makeDisplay } from "../../web/js/display.js";

const WEB = join(dirname(fileURLToPath(import.meta.url)), "../../web");
const css = readFileSync(join(WEB, "css/panel-chart.css"), "utf8");

test("lastFinite skips NaN and gives null when no value is finite", () => {
  assert.equal(lastFinite(Float64Array.of(1, 2, NaN)), 2);
  assert.equal(lastFinite(Float64Array.of(NaN, NaN)), null);
  assert.equal(lastFinite(new Float64Array(0)), null);
});

test("fallbackColor walks the 18 tokens of the chart palette, and wraps", () => {
  assert.equal(fallbackColor(0), "var(--qchart-1)");
  assert.equal(fallbackColor(17), "var(--qchart-18)");
  assert.equal(fallbackColor(18), "var(--qchart-1)");
  assert.equal(fallbackColor(-1), "var(--qchart-18)");
  assert.equal(fallbackColor(undefined), "var(--qchart-1)");
});

test("legendOptions maps the format to the engine options", () => {
  const o = legendOptions({ legend: { show: true, placement: "right", mode: "table", values: ["max", "last", "bogus"] }, tooltip: { mode: "multi" } });
  assert.equal(o.legend, "always");
  assert.deepEqual(o.legendColumns, ["max", "last"]);
  assert.equal(o.tooltip, "all");
  assert.equal(o.mode, "table");
  assert.equal(o.placement, "right");
  assert.equal(legendOptions({ legend: { show: false, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "hidden" } }).legend, false);
  assert.equal(legendOptions({ legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "hidden" } }).tooltip, "none");
  assert.equal(legendOptions({ legend: { show: true, placement: "bottom", mode: "list", values: [] } }).tooltip, "single");
});

// A fake chart: the legend is beside the plot when its left edge is at or after the right edge of the plot.
function fakeChart({ beside, legendHidden = false }) {
  const plot = { offsetLeft: 0, offsetWidth: 300 };
  const legend = { hidden: legendHidden, offsetLeft: beside ? 300 : 0, offsetHeight: 40 };
  const calls = [];
  return {
    calls,
    root: { querySelector: (sel) => (sel === ".chartCore__legend" ? legend : sel === ".chartCore__plot" ? plot : null) },
    update: (o) => calls.push(o.height),
  };
}

test("fitHeight takes the legend height off the plot only when the legend is under it", () => {
  const seen = [];
  globalThis.ResizeObserver = class {
    observe(node) { seen.push(node); }
    disconnect() {}
  };
  // fitHeight listens to the event "hune-shown" of the page (edit_flash.test.mjs).
  globalThis.document = { addEventListener() {}, removeEventListener() {} };
  const host = { clientHeight: 200 };
  const under = fakeChart({ beside: false });
  fitHeight(host, under)();
  assert.deepEqual(under.calls, [160]);
  const right = fakeChart({ beside: true });
  fitHeight(host, right)();
  assert.deepEqual(right.calls, [200]);
  const off = fakeChart({ beside: false, legendHidden: true });
  fitHeight(host, off)();
  assert.deepEqual(off.calls, [200]);
  const small = fakeChart({ beside: false });
  fitHeight({ clientHeight: 70 }, small)();
  assert.deepEqual(small.calls, [60]);
  delete globalThis.ResizeObserver;
  delete globalThis.document;
});

test("panel-chart.css uses tokens: no literal colour, no outline none, no literal font size", () => {
  const body = css.replace(/\/\*[\s\S]*?\*\//g, "");
  assert.ok(!/#[0-9a-fA-F]{3,8}\b/.test(body), "a hex colour");
  assert.ok(!/\brgba?\(/.test(body), "an rgb colour");
  assert.ok(!/outline\s*:\s*(none|0)\b/.test(body), "outline none");
  assert.ok(!/font-size\s*:\s*\d/.test(body), "a literal font size");
  assert.ok(!/!important/.test(body), "important");
});

test("panel-chart.css gives the right legend a column and the narrow fallback", () => {
  assert.match(css, /\[data-legend-placement="right"\] > \.chartCore \{[^}]*display: grid/);
  assert.match(css, /\[data-legend-fallback\] > \.chartCore/);
  assert.match(css, /\.chartCore__legendItem:not\([^)]*\)\[aria-pressed="false"\] i \{ opacity/);
});

const field = (unit, decimals = null) => ({ config: { unit, decimals } });
const ser = (id, f, values) => ({ id, field: f, values: Float64Array.from(values) });

test("the legend table keeps one precision in a column, and the total of a percentage has no unit", () => {
  const f1 = field("percent", 1);
  const f2 = field("percent", 2);
  const f3 = field("percent", null);
  const fmt = legendFormatter([ser("a", f1, [73.4, 20]), ser("b", f2, [74.13, 18.93]), ser("c", f3, [73.5, 21])]);
  // The largest number of decimals of the column wins: 2.
  assert.equal(fmt(74.13, "max", { field: f2 }), "74.13%");
  assert.equal(fmt(73.4, "max", { field: f1 }), "73.40%");
  assert.equal(fmt(73.5, "max", { field: f3 }), "73.50%");
  // The total is a plain number.
  assert.ok(!fmt(15398, "total", { field: f1 }).includes("%"));
  assert.equal(fmt(NaN, "last", { field: f1 }), "\u2014");
  // A unit with no percent keeps its unit in the total.
  const b = field("bytes");
  const fb = legendFormatter([ser("x", b, [1024, 2048])]);
  assert.equal(fb(3072, "total", { field: b }), "3 KiB");
});

test("the tooltip gives 3 significant digits unless the field sets decimals", () => {
  const f = field("short");
  const fmt = tooltipFormat(f);
  assert.equal(sigDecimals(31200, "short"), 1);
  assert.equal(fmt(31200, { field: f }), "31.2 K");
  assert.equal(fmt(31100, { field: f }), "31.1 K");
  assert.equal(fmt(NaN, { field: f }), "NULL");
  assert.equal(tooltipFormat(field("percent", 2))(66.7412, { field: field("percent", 2) }), "66.74%");
  assert.equal(fmt(7, { field: field("none") }), "7");
  assert.equal(sigDecimals(0, "none"), 0);
});

test("the tooltip head shows milliseconds only for a step under one second", () => {
  assert.equal(timeFractionDigits(Float64Array.of(0, 60000, 120000)), 0);
  assert.equal(timeFractionDigits(Float64Array.of(0, 1000, 2000)), 0);
  assert.equal(timeFractionDigits(Float64Array.of(0, 100, 200)), 3);
  assert.equal(timeFractionDigits(Float64Array.of(5)), 0);
});

test("overlapping fills keep the configured opacity, as Grafana does (C2)", () => {
  assert.equal(overlapFill(0.3, 1, false), 0.3);
  assert.equal(overlapFill(0.07, 4, false), 0.07);
  assert.equal(overlapFill(0.3, 3, true), 0.3);
  assert.equal(overlapFill(0, 4, false), 0);
});

test("a whole number in the tooltip has no decimals, a fraction keeps them (C4)", () => {
  const f = field("none");
  const fmt = tooltipFormat(f);
  assert.equal(fmt(69, { field: f }), "69");
  assert.equal(fmt(7, { field: f }), "7");
  assert.equal(fmt(66.74, { field: f }), "66.7");
  assert.equal(fmt(31000, { field: field("short") }), "31 K");
  assert.equal(fmt(1024, { field: field("bytes") }), "1 KiB");
  assert.equal(trimZeros("100.0%"), "100%");
  assert.equal(trimZeros("1.05"), "1.05");
});

test("a mark colour of a named token is a theme variable, another colour resolves as before", () => {
  assert.equal(markCss("yellow"), "var(--viz-yellow)");
  assert.equal(markCss("#abcdef"), "#abcdef");
  assert.equal(markCss("text"), "var(--text)");
  // The mode `palette` draws the series palette of ClickHouse Dash: a hex of the theme, from slot i mod 18.
  const cfg = { color: { mode: "palette" } };
  assert.equal(markColor(cfg, 1, { seriesIndex: 1 }), "#e86a34");
  assert.equal(markColor(cfg, 1, { seriesIndex: 1, dark: false }), "#c14802");
  assert.equal(markColor(cfg, 1, { seriesIndex: 7 }), "#49c1ea");
  assert.equal(markColor(cfg, 1, { seriesIndex: 18 }), "#4296fb");
  assert.equal(markColor({ color: { mode: "fixed", fixed: "dark-yellow" } }, 1, { seriesIndex: 7 }), "var(--viz-dark-yellow)");
  assert.equal(markColor({ color: { mode: "thresholds" }, thresholds: { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] } }, 90), "var(--viz-red)");
  const d = makeDisplay({ kind: "number", values: Float64Array.of(90), nulls: [false], config: { unit: "none", decimals: null, color: { mode: "fixed", fixed: "blue" } } })(0);
  assert.equal(d.color, NAMED.blue);
  assert.equal(d.mark, "var(--viz-blue)");
});

// The light theme tokens of tokens.css: each --viz-* has 3:1 on the panel and on the page, and its five steps keep their order.
test("the light --viz tokens have 3:1 on the white panel and keep the order of each hue", () => {
  const tokens = readFileSync(join(WEB, "css/tokens.css"), "utf8");
  const light = tokens.slice(tokens.lastIndexOf('html[data-theme="light"] {\n  --viz-super'));
  const values = new Map([...light.matchAll(/--viz-([a-z-]+): (#[0-9a-f]{6});/g)].map((m) => [m[1], m[2]]));
  assert.equal(values.size, 30);
  const lum = (c) => relativeLuminance(c);
  const ratio = (a, b) => (Math.max(lum(a), lum(b)) + 0.05) / (Math.min(lum(a), lum(b)) + 0.05);
  for (const [name, c] of values) {
    assert.ok(ratio(c, "#ffffff") >= 3, `${name} ${c} on the panel`);
    assert.ok(ratio(c, "#fbfbfb") >= 3, `${name} ${c} on the page`);
  }
  for (const hue of ["red", "orange", "yellow", "green", "blue", "purple"]) {
    const steps = [`super-light-${hue}`, `light-${hue}`, hue, `semi-dark-${hue}`, `dark-${hue}`].map((n) => lum(values.get(n)));
    for (let i = 1; i < steps.length; i++) assert.ok(steps[i] < steps[i - 1], `${hue} step ${i}`);
  }
});

test("the copied light chart palette has 3:1 on the white panel", () => {
  const vendor = readFileSync(join(WEB, "css/vendor/chdash/00-tokens.css"), "utf8");
  const light = vendor.slice(vendor.indexOf('html[data-theme="light"]'));
  const values = [...light.matchAll(/--qchart-(\d+|other): (#[0-9a-f]{6});/g)];
  assert.ok(values.length >= 19);
  for (const [, n, c] of values) {
    const l = relativeLuminance(c);
    assert.ok(1.05 / (l + 0.05) >= 3, `--qchart-${n} ${c}`);
  }
});

test("the legend has no scroll area and no mode button, and the swatch is a 14x4 pill (C3)", () => {
  assert.match(css, /\.chartCore__legendList \{[^}]*max-height: none;[^}]*overflow: visible/);
  assert.match(css, /\.chartCore__legendItem i \{ width: 14px; height: 4px; border-radius: var\(--r-pill\)/);
  assert.match(css, /\.chartCore__legendMode \{ display: none; \}/);
  // E41: the engine has no button of the mode and no stored preference of the mode.
  const engine = readFileSync(join(WEB, "js/vendor/chdash/app_chart_core.js"), "utf8").replace(/\/\/[^\n]*/g, "");
  assert.ok(!/chartCore__legendMode|legendModeButton|storage\.pref/.test(engine));
});

test("the engine edits of wave G are marked and set the Grafana plot margins (C1)", () => {
  const core = readFileSync(join(WEB, "js/vendor/chdash/app_chart_core.js"), "utf8");
  assert.match(core, /const FONT_SIZE = 12;/);
  assert.match(core, /const PAD_TOP = 8;/);
  assert.match(core, /const PAD_RIGHT = 8;/);
  assert.match(core, /const TICK_LEN = 0;/);
  assert.match(core, /const bottom = 21;/);
  assert.match(core, /\/\/ hune: the x axis has no tick marks/);
});

test("a dense line is one polyline through first, extremes in order and last of each column, as uPlot draws it (P5)", () => {
  const core = readFileSync(join(WEB, "js/vendor/chdash/app_chart_core.js"), "utf8");
  assert.match(core, /\/\/ hune: one continuous polyline goes through every column/);
  assert.match(core, /function columnsPath\(ctx, a, b\)/);
  assert.match(core, /colLoFirst\[k\] \? colMn\[k\] : colMx\[k\]/);
  // The old drawing of a tall column as a rect is gone: the columns are not drawn as separate bars.
  const draw = core.slice(core.indexOf("function drawColumns"), core.indexOf("const runEnd = "));
  assert.doesNotMatch(draw, /fillRect/);
  // A lone value is a mark of the width of the line, not a 4.4 px dot.
  assert.match(core, /\/\/ hune: a lone value is a mark of the width of the line/);
  assert.doesNotMatch(core, /ctx\.arc\(vx\[a\], vy\[a\], 2\.2/);
});

test("the time series panel breaks a line at a null value only, not at a missing time step (P5)", () => {
  const ts = readFileSync(join(WEB, "js/panels/timeseries.js"), "utf8");
  assert.match(ts, /connectGaps: true,/);
});

test("the tooltip card has the head band, the rule and the 14x4 pill (C4)", () => {
  assert.match(css, /\.chartCore__tooltip \{[^}]*background: var\(--surface-menu\)[^}]*box-shadow: var\(--shadow-tooltip\)/);
  assert.match(css, /\.chartCore__tooltip strong \{[^}]*border-bottom: 1px solid var\(--border\)/);
  assert.ok(!/is-nearest \{ background/.test(css));
});

test("panel-chart.css draws the cursor of Grafana: both dashed lines and one 4 px dot at 50 % on a time series", () => {
  assert.match(css, /\.chartCore__xline \{ border-left-color: var\(--chart-cursor-line\)/);
  assert.match(css, /\.chartCore__yline \{ border-top-color: var\(--chart-cursor-line\)/);
  assert.ok(!/\.chartCore__yline \{ display: none/.test(css), "the horizontal line shows");
  assert.match(css, /\[data-chart="timeseries"\] \.chartCore__dot:not\(\.is-nearest\) \{ display: none/);
  const rule = css.match(/\[data-chart="timeseries"\] \.chartCore__dot\.is-nearest \{([^}]*)\}/);
  assert.ok(rule, "the rule of the nearest dot");
  assert.match(rule[1], /width: 4px/);
  assert.match(rule[1], /height: 4px/);
  assert.match(rule[1], /opacity: 0\.5/);
});

test("legendOptions keeps the order of the columns of the document and passes the sort of the legend and of the tooltip (F20)", () => {
  const o = legendOptions({
    legend: { show: true, placement: "bottom", mode: "table", values: ["last", "min", "median", "change_count", "bogus", "max"], sort_by: "max", sort_desc: true },
    tooltip: { mode: "multi", sort: "desc", hide_zeros: true },
  });
  assert.deepEqual(o.legendColumns, ["last", "min", "median", "change_count", "max"]);
  assert.deepEqual(o.legendSort, { by: "max", desc: true });
  assert.equal(o.tooltipSort, "desc");
  assert.equal(o.tooltipHideZeros, true);
  const none = legendOptions({ legend: { show: true, placement: "bottom", mode: "table", values: [] }, tooltip: { mode: "multi", sort: "none", hide_zeros: false } });
  assert.equal(none.legendSort, null);
  assert.equal(none.tooltipSort, undefined);
  assert.equal(none.tooltipHideZeros, false);
});

const numField = (values, unit = "none", decimals = null) => ({
  kind: "number",
  values: Float64Array.from(values),
  nulls: Uint8Array.from(values.map((v) => (Number.isNaN(v) ? 1 : 0))),
  config: { unit, decimals },
});

test("the legend calculations that the engine does not do come from reduce.js (F20)", () => {
  const s = { id: "a", field: numField([1, 2, 3, 4, 10]), values: Float64Array.of(1, 2, 3, 4, 10) };
  assert.equal(legendCalc(s, "median"), 3);
  assert.equal(legendCalc(s, "range"), 9);
  assert.equal(legendCalc(s, "diff"), 9);
  assert.equal(legendCalc(s, "count"), 5);
  assert.equal(legendCalc(s, "distinct_count"), 5);
  assert.equal(legendCalc(s, "first"), 1);
  assert.ok(Number.isNaN(legendCalc({ id: "b" }, "median")));
});

test("legendFormatter writes the new columns: a count has no unit and no decimals, the difference in percent is a percentage", () => {
  const f = numField([10, 20, 30], "bytes", null);
  const s = { id: "a", field: f, values: Float64Array.of(10, 20, 30) };
  const fmt = legendFormatter([s], ["count", "diff_percent", "median"]);
  assert.equal(fmt(3, "count", s), "3");
  assert.ok(fmt(200, "diff_percent", s).endsWith("%"));
  assert.ok(fmt(20, "median", s).endsWith("B"));
});

test("a series is hidden from the plot, the legend or the tooltip by hide_from, and the plot hides it through the engine option hidden (F03)", () => {
  const f = (h) => ({ config: { hide_from: h } });
  assert.deepEqual(hideFlags(f({ legend: false, tooltip: true, viz: false })), { legendHidden: false, tooltipHidden: true, vizHidden: false });
  assert.deepEqual(hideFlags(f({ legend: true, tooltip: false, viz: true })), { legendHidden: true, tooltipHidden: false, vizHidden: true });
  assert.deepEqual(hideFlags({}), { legendHidden: false, tooltipHidden: false, vizHidden: false });
  const series = [{ id: "1", vizHidden: true }, { id: "2", vizHidden: false }, { id: "3", vizHidden: true }];
  assert.deepEqual([...hiddenIds(series)], ["1", "3"]);
});

const STEPS = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 40, color: "orange" }, { value: 60, color: "red" }] };

test("the lines of the thresholds skip the base step, and the bands go from step to step", () => {
  const line = thresholdShapes(STEPS, "line", [null, null], [10, 80]);
  assert.deepEqual(line.lines, [{ value: 40, color: "orange" }, { value: 60, color: "red" }]);
  assert.deepEqual(line.areas, []);
  const area = thresholdShapes(STEPS, "area", [null, null], [10, 80]);
  assert.deepEqual(area.areas, [{ from: 10, to: 40, color: "green" }, { from: 40, to: 60, color: "orange" }, { from: 60, to: 80, color: "red" }]);
  assert.deepEqual(area.lines, []);
  const both = thresholdShapes(STEPS, "dashed_area", [null, null], [10, 80]);
  assert.equal(both.lines.length, 2);
  assert.equal(both.areas.length, 3);
  assert.deepEqual(styleParts("dashed"), { lines: true, dashed: true, areas: false });
  assert.deepEqual(styleParts("off"), { lines: false, dashed: false, areas: false });
  assert.deepEqual(thresholdShapes(STEPS, "off", [null, null], [10, 80]), { lines: [], areas: [] });
});

test("a line outside the axis is not drawn, and a band is cut at the ends of the plot", () => {
  const s = thresholdShapes(STEPS, "line_area", [null, null], [50, 55]);
  assert.deepEqual(s.lines, []);
  assert.deepEqual(s.areas, [{ from: 50, to: 55, color: "orange" }]);
});

test("a transparent step draws nothing, and the lines up to it take the colour of the step before (9.19)", () => {
  const t = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 40, color: "transparent" }, { value: 60, color: "red" }] };
  const s = thresholdShapes(t, "line_area", [null, null], [10, 80]);
  assert.deepEqual(s.lines, [{ value: 40, color: "green" }, { value: 60, color: "red" }]);
  assert.deepEqual(s.areas.map((a) => a.color), ["green", "red"]);
});

test("in the mode percentage a step is lo + (hi - lo) * p / 100, with the bounds of the field, else the ends of the axis", () => {
  const t = { mode: "percentage", steps: [{ value: null, color: "green" }, { value: 50, color: "red" }] };
  assert.deepEqual(stepValues(t, [0, 200], [10, 80]).map((s) => s.value), [-Infinity, 100]);
  assert.deepEqual(stepValues(t, [null, null], [10, 80]).map((s) => s.value), [-Infinity, 45]);
});
