// SPDX-License-Identifier: MIT OR Apache-2.0
// The loading preview of the bar chart (9.11): web/js/preview/barchart.js, vertical (the frame of the chart engine) and horizontal (the markup of the
// loaded `.hbar`). Each test follows one rule of the council file `barchart.md`. It names the JSON member that decides the rule. The browser cases
// B335 to B338 compare with the loaded panel.
import assert from "node:assert/strict";
import test from "node:test";
import { boundHolds, setMeasurer } from "../../web/js/preview/common.js";
import { build as buildBars, hLegendOf, orientationOf } from "../../web/js/preview/barchart.js";
import { HEAD_ROW, BODY_ROW, LINE, legendOf, rulesOf, setGutter, setSpacing, tick0OfBars } from "../../web/js/preview/timeseries.js";

setMeasurer((text) => text.length * 7);
setSpacing(0);
setGutter(0);


const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const query = (over = {}) => ({ source: "s", format: "time_series", sql: ["SELECT 1"], min_interval: null, max_data_points: null, ...over });
const panel = (type, options = {}, fields = {}, data = { query: query() }, extra = {}) => ({
  id: 1, type, title: "t", grid: { x: 0, y: 0, w: 12, h: 8 }, data, transforms: [], fields: { defaults: {}, overrides: [], ...fields }, options: { legend: legend(), ...options }, ...extra,
});
const ts = (options, fields, data, extra) => panel("timeseries", options, fields, data, extra);
const bars = (options, fields, data, extra) => panel("barchart", { orientation: "vertical", ...options }, fields, data, extra);

// ---- Tree helpers --------------------------------------------------------------------------------------------------------------------
const walk = (n, f) => {
  f(n);
  for (const k of n.k || []) walk(k, f);
};
const all = (out) => {
  const list = [];
  for (const e of out.elements) walk(e, (n) => list.push(n));
  return list;
};
const byClass = (out, c) => all(out).filter((n) => (n.c || "").split(" ").includes(c));
const parts = (out) => all(out).filter((n) => n.a && n.a["data-skel-part"]);
const part = (out, name) => parts(out).find((n) => n.a["data-skel-part"] === name);
const px = (v) => Number.parseFloat(v);
const box = { w: 670, h: 238 };

// ---- The bar chart ----------------------------------------------------------------------------------------------------------------------------------
test("the orientation: horizontal, vertical, and auto by the box (a tie gives the vertical form)", () => {
  const o = (orientation, w, h) => orientationOf(bars({ orientation }), { w, h });
  assert.equal(o("vertical", 100, 400), "vertical");
  assert.equal(o("horizontal", 400, 100), "horizontal");
  assert.equal(o("auto", 299, 300), "horizontal");
  assert.equal(o("auto", 300, 300), "vertical", "a tie");
  assert.equal(o("auto", 301, 300), "vertical");
  assert.equal(buildBars(bars({ orientation: "auto" }), { w: 299, h: 300 }).notes[0], "bar chart, horizontal frame");
  assert.equal(buildBars(bars({ orientation: "auto" }), { w: 300, h: 300 }).notes[0], "bar chart, vertical frame");
});

test("the vertical bar chart: top 8, right 8, band 21, and no bar, no category and no value label (B4, B5, B11, R17)", () => {
  const out = buildBars(bars(), box);
  const plot = part(out, "plot");
  assert.equal(px(plot.v["--y"]), 8);
  assert.equal(px(plot.v["--r"]), 8);
  assert.equal(px(plot.v["--h"]), 238 - 31 - 8 - 21);
  assert.deepEqual(parts(out).map((n) => n.a["data-skel-part"]), ["plot", "legend"]);
  assert.equal(out.bounds.band.kind, "exact");
  // Loaded, measured on the demo dashboard traffic: [47, 8, 391, 216] in 446 x 276 with a list legend of one line (31 px).
  const demo = buildBars(bars({ legend: legend() }), { w: 446, h: 276 });
  assert.ok(boundHolds(demo.bounds.plotLeft, 47) && boundHolds(demo.bounds.plotTop, 8) && boundHolds(demo.bounds.plotRight, 438) && boundHolds(demo.bounds.plotBottom, 224));
});

test("the vertical bar chart gives the least gutter: 9 + the label 0 when the bars start at 0, and the title room (B14, B15)", () => {
  const left = (fields) => buildBars(bars({ legend: legend({ show: false }) }, fields), box).bounds.gutterLeft.value;
  assert.equal(left({}), Math.ceil(9 + 7), "the bars start at 0: the tick 0 is certain");
  assert.equal(left({ defaults: { unit: "percent" } }), Math.ceil(9 + 14), "the label 0 of the unit");
  assert.equal(left({ defaults: { min: 5 } }), 9, "a range above 0");
  assert.equal(left({ defaults: { soft_min: 3 } }), 9);
  assert.equal(left({ defaults: { max: -1 } }), 9, "a range below 0");
  assert.equal(left({ defaults: { axis_label: "Weight" } }), 16 + 28);
  assert.equal(left({ defaults: { axis_width: 40, axis_label: "Weight" } }), 60);
  assert.equal(left({ overrides: [{ match: { by: "name", value: "a" }, set: { unit: "bytes" } }] }), 9, "the unit is not uniform: the label 0 is data");
  const tick = (fields) => tick0OfBars(rulesOf(bars({}, fields)));
  assert.equal(tick({ defaults: { unit: "bytes" }, overrides: [{ match: { by: "name", value: "a" }, set: { unit: "bytes" } }] }), true, "the same unit in an override");
});

test("a turned x label makes the band data: the preview takes 21 px and the bottom edge is a bound (B12)", () => {
  const flat = buildBars(bars({ x_tick_rotation: 0 }), box);
  const turned = buildBars(bars({ x_tick_rotation: -90 }), box);
  assert.equal(flat.bounds.band.kind, "exact");
  assert.equal(turned.bounds.band.kind, "least");
  assert.equal(turned.bounds.band.value, 21);
  assert.equal(part(turned, "plot").a["data-skel-class"], "contain:ltrb");
  assert.equal(boundHolds(turned.bounds.plotBottom, 8 + 100), true);
});

test("the horizontal bar chart uses the markup of the loaded chart: a plot at 22 px, a band of 21, and a legend line (B23 to B27)", () => {
  const out = buildBars(bars({ orientation: "horizontal", legend: legend() }), box);
  const root = byClass(out, "hbar")[0];
  assert.equal(out.elements[0].c, "skel__host", "the host gives the rules of the legend table");
  assert.equal(root.a["data-legend-placement"], "bottom");
  assert.equal(root.v["--hbar-label"], "22px");
  assert.equal(root.v["--band"], "21px");
  assert.ok(byClass(out, "hbar__grid").length === 1 && byClass(out, "hbar__axis").length === 1 && byClass(out, "hbar__legend").length === 1);
  assert.equal(byClass(out, "hbar__axisTitle").length, 0, "no title room unless the title is proven");
  assert.equal(part(out, "plot").a["data-skel-class"], "contain:ltrb");
  assert.equal(out.bounds.plotTop.value, 8);
  assert.equal(out.bounds.plotLeft.value, 22);
  assert.equal(out.bounds.plotRight.value, 670 - 8);
  assert.ok(Math.abs(out.bounds.plotBottom.value - (238 - 21 - (12 + LINE))) < 1e-9);
  assert.ok(boundHolds(out.bounds.legendHeight, 30.857), "one line at least");
});

test("the horizontal legend follows the shared object: a list or a table, hidden entries, off (9.19, B27)", () => {
  const f = { defaults: { hide_from: { legend: true, tooltip: false, viz: false } } };
  const hidden = buildBars(bars({ orientation: "horizontal", legend: legend({ placement: "right" }) }, f), box);
  assert.equal(byClass(hidden, "hbar__legend").length, 0, "every entry hidden: no legend");
  assert.equal(hidden.bounds.plotRight.value, 670 - 8, "and no room beside the plot");
  const table = buildBars(bars({ orientation: "horizontal", legend: legend({ mode: "table", values: ["mean", "max"] }) }), box);
  const wrap = byClass(table, "chartCore__legendTable")[0];
  assert.ok(wrap, "a table follows the mode");
  assert.deepEqual(all(table).filter((n) => n.t === "th" && n.a.scope === "col").map((n) => n.a["aria-sort"]), ["none", "none", "none"]);
  assert.equal(byClass(table, "hbar__legend")[0].a["data-mode"], "table");
  assert.equal(table.bounds.legendHeight.value, 47.214 > 83 ? 83 : Math.min(HEAD_ROW + BODY_ROW, 83));
  const none = buildBars(bars({ orientation: "horizontal", legend: legend({ mode: "table", values: [] }) }), box);
  assert.deepEqual(all(none).filter((n) => n.t === "th" && n.a.scope === "col").map((n) => n.a["aria-sort"]), ["none"], "values [] gives the column Name only (4.26)");
  const off = buildBars(bars({ orientation: "horizontal", legend: legend({ show: false }) }), box);
  assert.equal(byClass(off, "hbar__legend").length, 0);
  assert.equal(off.bounds.plotBottom.value, 238 - 21);
  const right = buildBars(bars({ orientation: "horizontal", legend: legend({ placement: "right" }) }), box);
  assert.equal(byClass(right, "hbar").length, 1);
  assert.equal(byClass(right, "hbar")[0].a["data-legend-placement"], "right");
  assert.equal(right.bounds.plotRight.value, 670 - 40 - 8);
});

test("the horizontal legend: a list at the right keeps at most 30 % of the width, and falls back under the plot where the document proves it (B27)", () => {
  const list = legendOf(bars({ legend: legend({ placement: "right" }) }), rulesOf(bars()));
  const at = (w) => hLegendOf(list, w, 238);
  assert.equal(at(268).w, 40, "one empty entry");
  assert.equal(at(100).fallback, true, "30 + 228 > 100");
  assert.equal(at(267).fallback, true, "40 + 228 = 268");
  assert.equal(at(268).fallback, false);
  assert.equal(at(268).never, false, "a column of 40 px can grow to 30 %");
  assert.equal(at(326).never, true, "W - ceil(0.3 W) = 228");
  assert.equal(at(325).never, false);
  const small = at(260);
  assert.equal(small.h, Math.min(8 + LINE, 83 + 12), "under the plot: padding 4 and 4, and the cap of 35 % with 12 px");
  const table = legendOf(bars({ legend: legend({ placement: "right", mode: "table", values: ["max"] }) }), rulesOf(bars()));
  assert.equal(hLegendOf(table, 1000, 238).never, false, "a table has no cap on its width");
  assert.equal(hLegendOf(table, 100, 238).fallback, true);
  assert.equal(hLegendOf(table, 100, 238).h, Math.min(HEAD_ROW + BODY_ROW, 83));
  const bottom = legendOf(bars(), rulesOf(bars()));
  assert.equal(hLegendOf(bottom, 670, 40).h, 12 + 14, "the cap holds under one line");
  assert.equal(hLegendOf(bottom, 670, 40).hExact, true);
  assert.equal(hLegendOf(bottom, 670, 238).hExact, false);
});

test("a horizontal legend beside the plot, where it can fall back, has no measured part and no right edge", () => {
  const some = buildBars(bars({ orientation: "horizontal", legend: legend({ placement: "right" }) }), { w: 300, h: 238 });
  assert.equal(part(some, "legend"), undefined);
  assert.equal(part(some, "plot").a["data-skel-class"], "contain:ltb");
  const wide = buildBars(bars({ orientation: "horizontal", legend: legend({ placement: "right" }) }), box);
  assert.equal(part(wide, "legend").a["data-skel-class"], "least");
  assert.equal(part(wide, "plot").a["data-skel-class"], "contain:ltrb");
});

test("the horizontal band is axis_width when the document fixes it, the title band is 28 when the title is proven (B24, B25)", () => {
  const h = (fields) => buildBars(bars({ orientation: "horizontal", legend: legend({ show: false }) }, fields), box);
  assert.equal(byClass(h({ defaults: { axis_width: 30 } }), "hbar")[0].v["--band"], "30px");
  assert.equal(byClass(h({ defaults: { axis_width: 0 } }), "hbar")[0].v["--band"], "0px");
  assert.equal(h({ defaults: { axis_width: 30 }, overrides: [{ match: { by: "name", value: "a" }, set: { axis_width: 10 } }] }).bounds.band.kind, "least");
  const titled = h({ defaults: { axis_label: "Weight" } });
  assert.equal(byClass(titled, "hbar__axisTitle").length, 1);
  assert.equal(titled.bounds.plotBottom.value, 238 - 21 - 28);
  const maybe = h({ overrides: [{ match: { by: "name", value: "a" }, set: { axis_label: "W" } }] });
  assert.equal(byClass(maybe, "hbar__axisTitle").length, 0);
  assert.equal(maybe.bounds.title.kind, "least");
});

test("a percent stack: the label 100% is a bar at the right end of the axis, only where the right edge and the band are exact (B28)", () => {
  const h = (options, fields, size = box) => buildBars(bars({ orientation: "horizontal", stacking: "percent", legend: legend({ show: false }), ...options }, fields), size);
  const tick = part(h(), "tick");
  assert.ok(tick, "the tick is claimed");
  assert.equal(tick.a["data-skel-class"], "exact");
  assert.equal(px(tick.v["--w"]), 4 * 7, "the measured width of 100%");
  assert.equal(px(tick.v["--t"]), 9.75, "7 px in the band, a line of 14 px, a bar of 8 px, less a quarter of a pixel");
  assert.equal(px(part(h({}, { defaults: { axis_border: true } }), "tick").v["--t"]), 13.75, "11 px with a border");
  assert.equal(part(h({ stacking: "normal" }), "tick"), undefined, "only a percent stack has this tick");
  assert.equal(part(h({ legend: legend({ placement: "right" }) }), "tick"), undefined, "a legend beside the plot moves the right end");
  assert.equal(part(h({ legend: legend() }), "tick"), undefined, "a list legend at the bottom: the line count is data");
  assert.ok(part(h({ legend: legend() }, {}, { w: 670, h: 40 }), "tick"), "a short panel: the cap holds the legend height");
  assert.equal(part(h({}, { overrides: [{ match: { by: "name", value: "a" }, set: { axis_border: true } }] }), "tick"), undefined, "an override can move the label");
});

test("the plot of the bar chart has decorative bars, a form that claims no count and no value, in neutral tones (R17)", () => {
  const v = buildBars(bars({ legend: legend({ show: false }) }), box);
  const rects = all(v).filter((n) => n.t === "rect");
  assert.ok(rects.length >= 2);
  const heights = new Set(rects.map((n) => n.a.height));
  assert.ok(heights.size > 3, "the heights vary");
  assert.deepEqual([...new Set(rects.map((n) => n.c))], ["skel__bar"], "one tone");
  const period = rects.map((n) => Number(n.a.x));
  assert.equal(period[1] - period[0], 14, "a fixed period");
  const none = buildBars(bars({ legend: legend({ show: false }) }, { defaults: { hide_from: { legend: false, tooltip: false, viz: true } } }), box);
  assert.equal(all(none).filter((n) => n.t === "rect").length, 0, "every series hidden from the plot: no bar");
  const h = buildBars(bars({ orientation: "horizontal", legend: legend({ show: false }) }), box);
  const rows = all(h).filter((n) => n.t === "rect");
  assert.ok(rows.length >= 1 && new Set(rows.map((n) => n.a.width)).size > 1, "the lengths vary");
  for (const n of all(h)) assert.ok(!("text" in n) && !("style" in n.a));
});

test("the bar chart gives the neutral form with no legend member or a warning line", () => {
  const p = bars();
  delete p.options.legend;
  assert.deepEqual(buildBars(p, box).notes, ["no legend member"]);
  assert.deepEqual(buildBars(bars({}, { overrides: [{ match: { by: "regex", value: "/(/" }, set: {} }] }), box).notes, ["warning line"]);
});

test("a horizontal panel so short that the plot has no height claims no plot", () => {
  const out = buildBars(bars({ orientation: "horizontal" }), { w: 670, h: 48 });
  assert.equal(part(out, "plot"), undefined);
  assert.equal(part(out, "legend").a["data-skel-class"], "exact");
  assert.equal(byClass(out, "hbar__grid")[0].k.length, 0, "and no bar");
});
