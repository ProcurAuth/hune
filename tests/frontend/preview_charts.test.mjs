// SPDX-License-Identifier: MIT OR Apache-2.0
// The loading preview of the charts (9.11): web/js/preview/timeseries.js (the time series, the xy chart and the frame of the engine) and barchart.js.
// Each test follows one rule of the council files. It names the JSON member that decides the rule, and the form that the preview draws when the
// document does not decide. The bounds are checked with `boundHolds` against the values that the loaded panel gives (measured on the owner
// dashboard and on the demo dashboards, see tests/browser/cases/preview_charts.mjs).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { boundHolds, setMeasurer } from "../../web/js/preview/common.js";
import { build as buildBars, orientationOf } from "../../web/js/preview/barchart.js";
import { BODY_ROW, HEAD_ROW, LINE, build, chainOf, gutterOf, legendOf, rulesOf, seriesGeometry, setGutter, setSpacing, tick0OfBars, tick0OfSeries, zeroLabel } from "../../web/js/preview/timeseries.js";

// A text of 7 px for each character: the widths are easy to follow.
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

// ---- The fields that the document decides (R4) ---------------------------------------------------------------------------------------------
test("an override by query with the ref of the only visible query matches every field and joins the defaults", () => {
  const p = ts({}, { defaults: { axis_width: 30 }, overrides: [{ match: { by: "query", value: "A" }, set: { axis_width: 44, unit: "bytes" } }] });
  const r = rulesOf(p);
  assert.equal(r.D.axis_width, 44);
  assert.equal(r.D.unit, "bytes");
  assert.equal(r.touched("axis_width"), false);
});

test("an override by query is undecided with a second visible query, a reuse or a transform", () => {
  const ov = [{ match: { by: "query", value: "A" }, set: { axis_width: 44 } }];
  const two = ts({}, { overrides: ov }, { query: query(), extra_queries: [{ ref: "B", format: "time_series", source: "s", sql: ["x"] }] });
  assert.equal(rulesOf(two).touched("axis_width"), true, "two visible queries");
  const hidden = ts({}, { overrides: ov }, { query: query(), extra_queries: [{ ref: "B", format: "time_series", source: "s", sql: ["x"], hide: true }] });
  assert.equal(rulesOf(hidden).touched("axis_width"), false, "a hidden query does not count");
  assert.equal(rulesOf(ts({}, { overrides: ov }, { reuse: { panel: 2 } })).touched("axis_width"), true, "a reuse");
  assert.equal(rulesOf(ts({}, { overrides: ov }, undefined, { transforms: [{ kind: "organize" }] })).touched("axis_width"), true, "a transform");
  assert.equal(rulesOf(ts({}, { overrides: [{ match: { by: "query", value: "B" }, set: { axis_width: 1 } }] })).touched("axis_width"), true, "another ref");
});

test("an override by name or by names is undecided: it can set any member for some field", () => {
  const r = rulesOf(ts({}, { overrides: [{ match: { by: "name", value: "a" }, set: { unit: "bytes" } }, { match: { by: "names", mode: "include", names: ["b"] }, set: { axis_placement: "right" } }] }));
  assert.equal(r.touched("unit"), true);
  assert.equal(r.touched("axis_placement"), true);
  assert.equal(r.touched("min"), false);
  assert.equal(r.D.unit, undefined, "an undecided override never joins the defaults");
});

test("a pattern that does not compile matches no field and gives the warning line", () => {
  const r = rulesOf(ts({}, { overrides: [{ match: { by: "regex", value: "/(/" }, set: { unit: "bytes" } }] }));
  assert.equal(r.warning, true);
  assert.equal(r.touched("unit"), false, "its set is ignored");
  const out = build(ts({}, { overrides: [{ match: { by: "regex", value: "/(/" }, set: {} }] }), box);
  assert.deepEqual(out.notes, ["warning line"], "the height of the warning line needs the layout of the page: the preview draws nothing");
  assert.deepEqual(out.elements, []);
});

test("a pattern that compiles is undecided", () => {
  const r = rulesOf(ts({}, { overrides: [{ match: { by: "regex", value: "/cpu.*/" }, set: { unit: "bytes" } }] }));
  assert.equal(r.warning, false);
  assert.equal(r.touched("unit"), true);
});

// ---- The legend of the document (E3, E7) ----------------------------------------------------------------------------------------------------
test("the legend states: off, hidden, maybe hidden and shown (options.legend, fields hide_from)", () => {
  const state = (options, fields) => legendOf(ts(options, fields), rulesOf(ts(options, fields))).state;
  assert.equal(state({ legend: legend({ show: false }) }), "off");
  assert.equal(state({}, { defaults: { hide_from: { legend: true, tooltip: false, viz: false } } }), "hidden");
  assert.equal(state({}, { defaults: { hide_from: { legend: true, tooltip: false, viz: false } }, overrides: [{ match: { by: "name", value: "a" }, set: { hide_from: { legend: false, tooltip: false, viz: false } } }] }), "maybe");
  assert.equal(state({}, { overrides: [{ match: { by: "name", value: "a" }, set: { hide_from: { legend: true, tooltip: false, viz: false } } }] }), "maybe");
  assert.equal(state({}, { defaults: { hide_from: { legend: false, tooltip: false, viz: false } } }), "shown");
  assert.equal(state({}), "shown");
});

test("a legend that is off has no placement: the host is at the bottom, so the right gap goes", () => {
  const leg = legendOf(ts({ legend: legend({ show: false, placement: "right" }) }), rulesOf(ts()));
  assert.equal(leg.placement, "bottom");
  const c = chainOf(670, 238, leg);
  assert.equal(c.canvasW, 670);
  assert.equal(c.lh, 0);
});

test("the heads of the table legend: Name, then the calculations in the document order, with no duplicate and no unknown name", () => {
  const leg = (values, extra = {}) => legendOf(ts({ legend: legend({ mode: "table", values, ...extra }) }), rulesOf(ts()));
  assert.deepEqual(leg(["last_not_null", "mean", "max"]).heads, ["Name", "Last *", "Mean", "Max"]);
  assert.deepEqual(leg(["max", "min", "max", "nope"]).heads, ["Name", "Max", "Min"]);
  // Defect D1: no calculation kept gives the five of the engine. The preview copies the loaded panel until the owner decides.
  assert.deepEqual(leg([]).heads, ["Name", "Total", "Min", "Max", "Mean", "Last"]);
});

test("the sorted head has the arrow, and a sort key that is not a column gives none", () => {
  const sorted = (values, sort_by, sort_desc = false) => legendOf(ts({ legend: legend({ mode: "table", values, sort_by, sort_desc }) }), rulesOf(ts()));
  assert.equal(sorted(["mean", "max"], "max").sorted, 2);
  assert.equal(sorted(["mean", "max"], "name").sorted, 0);
  assert.equal(sorted(["mean", "max"], "last", true).sorted, -1, "the otel panels: sort_by last with last_not_null, mean and max");
  assert.equal(sorted(["mean"], "mean", true).desc, true);
  const out = build(ts({ legend: legend({ mode: "table", values: ["mean", "max"], sort_by: "max", sort_desc: true }) }), box);
  const th = all(out).filter((n) => n.t === "th" && n.a.scope === "col");
  assert.deepEqual(th.map((n) => n.a["aria-sort"]), ["none", "none", "descending"]);
});

test("the heads of the table are bars of the measured width of the text at weight 500, and the preview has no text", () => {
  const out = build(ts({ legend: legend({ mode: "table", values: ["mean", "max"] }) }), box);
  const bars_ = byClass(out, "skel__hb");
  assert.deepEqual(bars_.map((n) => px(n.v["--w"])), [4 * 7, 4 * 7, 3 * 7]);
  for (const n of all(out)) assert.ok(!("text" in n) && !("style" in n.a), "no text and no style attribute");
});

// ---- The vertical chain (E5, E8, E10, E13): the integers of the engine --------------------------------------------------------------------
test("a list legend at the bottom is one line with no name: 31 px, and the cap is floor(0.35 H) in px", () => {
  const leg = legendOf(ts(), rulesOf(ts()));
  const c = chainOf(670, 238, leg);
  assert.equal(c.cap, 83);
  assert.equal(c.lh, 31);
  assert.equal(c.lhExact, false, "the line count is data");
  assert.equal(Math.round(12 + LINE), 31);
});

test("a list legend whose cap is under one line is exact: the cap holds for any count (E5 case 1)", () => {
  const leg = legendOf(ts(), rulesOf(ts()));
  const c = chainOf(670, 50, leg);
  assert.equal(c.cap, 17);
  assert.equal(c.lh, 29, "12 + the cap");
  assert.equal(c.lhExact, true);
});

test("a table legend at the bottom is the head and one row, min(47, cap): exact when the cap is under 47 (E8 case 1)", () => {
  const leg = legendOf(ts({ legend: legend({ mode: "table" }) }), rulesOf(ts()));
  assert.ok(Math.abs(HEAD_ROW + BODY_ROW - 47.214) < 0.01);
  const tall = chainOf(670, 238, leg);
  assert.equal(tall.lh, 47);
  assert.equal(tall.lhExact, false);
  const short = chainOf(670, 100, leg);
  assert.equal(short.lh, 35);
  assert.equal(short.lhExact, true);
});

test("the canvas and the plot heights follow the chain: C = max(60, floor(H - Lh)), P = max(40, C - top - band)", () => {
  const out = build(ts(), box);
  const plot = part(out, "plot");
  assert.equal(px(plot.v["--h"]), 238 - 31 - 8 - 21);
  assert.equal(px(plot.v["--y"]), 8);
  // A small panel keeps the least sizes.
  const small = build(ts(), { w: 300, h: 70 });
  assert.equal(px(part(small, "plot").v["--h"]), 40, "the plot is at least 40 px high");
  assert.equal(px(byClass(small, "chartCore__plot")[0].v["--c"]), 60, "the canvas is at least 60 px high");
});

test("a legend at the right beside the plot takes no height; a list never falls back at 346 px and more", () => {
  const leg = legendOf(ts({ legend: legend({ placement: "right" }) }), rulesOf(ts()));
  const wide = chainOf(346, 238, leg);
  assert.equal(wide.lh, 0);
  assert.equal(wide.claimRight, true);
  assert.equal(wide.lhExact, true);
  assert.equal(wide.fallback, false);
  const between = chainOf(300, 238, leg);
  assert.equal(between.fallback, false, "the fallback is not certain between the two cases");
  assert.equal(between.claimRight, false, "the right edge is not claimed: the legend can fall back when the data come");
  assert.equal(between.lhExact, false);
});

test("the fallback of a right legend is certain under Lmin + 228 px, and the legend is then under the plot (E10)", () => {
  const list = legendOf(ts({ legend: legend({ placement: "right" }) }), rulesOf(ts()));
  const narrow = chainOf(260, 238, list);
  assert.equal(narrow.fallback, true);
  assert.equal(narrow.lh, 27, "4 + 18.857 + 4");
  assert.equal(narrow.canvasW, 260);
  const table = legendOf(ts({ legend: legend({ placement: "right", mode: "table", values: ["mean"] }) }), rulesOf(ts()));
  const t = chainOf(300, 238, table);
  assert.equal(t.fallback, true);
  assert.equal(t.lh, 51, "4 + the head and one row");
  const out = build(ts({ legend: legend({ placement: "right", mode: "table", values: ["mean"] }) }), { w: 300, h: 238 });
  assert.equal(byClass(out, "skel__host")[0].a["data-legend-fallback"], "");
  assert.equal(byClass(out, "skel__host")[0].a["data-legend-placement"], "right");
});

test("with the gutter of the platform, the least legend beside the plot is wider and the fallback comes sooner", () => {
  const list = legendOf(ts({ legend: legend({ placement: "right" }) }), rulesOf(ts()));
  setGutter(15);
  try {
    assert.equal(chainOf(282, 238, list).fallback, true, "40 + 15 + 228 = 283");
    assert.equal(chainOf(283, 238, list).fallback, false);
  } finally {
    setGutter(0);
  }
  assert.equal(chainOf(267, 238, list).fallback, true, "40 + 228 = 268");
  assert.equal(chainOf(267, 238, list).lh, 27);
  assert.equal(chainOf(268, 238, list).fallback, false);
});

test("every entry hidden: no legend at the bottom, and at the right the grid keeps its 8 px gap (defect D3)", () => {
  const f = { defaults: { hide_from: { legend: true, tooltip: false, viz: false } } };
  const bottom = chainOf(670, 238, legendOf(ts({}, f), rulesOf(ts({}, f))));
  assert.equal(bottom.lh, 0);
  assert.equal(bottom.canvasW, 670);
  assert.equal(bottom.draw, false);
  const right = chainOf(670, 238, legendOf(ts({ legend: legend({ placement: "right" }) }, f), rulesOf(ts({}, f))));
  assert.equal(right.canvasW, 662);
  assert.equal(right.claimRight, true);
  const out = build(ts({ legend: legend({ placement: "right" }) }, f), box);
  assert.equal(byClass(out, "chartCore__legend").length, 0, "a hidden legend is not drawn");
});

test("an override that can hide some entries makes the legend uncertain: the preview draws none and takes the whole height", () => {
  const f = { overrides: [{ match: { by: "name", value: "a" }, set: { hide_from: { legend: true, tooltip: false, viz: false } } }] };
  const out = build(ts({}, f), box);
  assert.equal(byClass(out, "chartCore__legend").length, 0);
  assert.equal(px(part(out, "plot").v["--h"]), 238 - 8 - 21);
  assert.equal(out.bounds.legendHeight.kind, "least");
  assert.equal(out.bounds.legendHeight.value, 0);
});

// ---- The margins and the gutters (E11, E12, E14, E15) -----------------------------------------------------------------------------------
test("the top margin is 8, and 0 when every axis is hidden; an undecided override of the placement makes it data", () => {
  const top = (fields) => {
    const p = ts({}, fields);
    return seriesGeometry(p, rulesOf(p));
  };
  assert.deepEqual([top({}).top, top({}).topExact], [8, true]);
  assert.deepEqual([top({ defaults: { axis_placement: "right" } }).top, top({ defaults: { axis_placement: "right" } }).topExact], [8, true]);
  assert.deepEqual([top({ defaults: { axis_placement: "hidden" } }).top, top({ defaults: { axis_placement: "hidden" } }).topExact], [0, true]);
  const loose = top({ defaults: { axis_placement: "hidden" }, overrides: [{ match: { by: "name", value: "a" }, set: { axis_placement: "left" } }] });
  assert.deepEqual([loose.top, loose.topExact], [0, false]);
});

test("the band under the plot is 21 px for a time series and 41 px for an xy chart, whose x axis has a title (E12)", () => {
  const tsOut = build(ts(), box);
  const xyOut = build(panel("xychart", { x_field: null, y_fields: [], draw: "points" }), box);
  assert.equal(tsOut.bounds.band.value, 21);
  assert.equal(px(part(tsOut, "plot").v["--h"]), 238 - 31 - 8 - 21);
  assert.equal(px(part(xyOut, "plot").v["--h"]), 238 - 31 - 8 - 41);
  assert.equal(xyOut.bounds.plotTop.value, 8);
});

test("all axes hidden: left and right are 8 px and the plot is exact (E14 a, E15)", () => {
  const out = build(ts({ legend: legend({ show: false }) }, { defaults: { axis_placement: "hidden" } }), box);
  const plot = part(out, "plot");
  assert.equal(plot.a["data-skel-class"], "exact");
  assert.deepEqual([px(plot.v["--x"]), px(plot.v["--y"]), px(plot.v["--r"]), px(plot.v["--h"])], [8, 0, 8, 238 - 21]);
  // The loaded plot of the same panel on the demo dashboards: [8, 0, 666, 217] in a box of 682 x 238.
  const wide = build(ts({ legend: legend({ show: false }) }, { defaults: { axis_placement: "hidden" } }), { w: 682, h: 238 });
  assert.ok(boundHolds(wide.bounds.plotLeft, 8) && boundHolds(wide.bounds.plotTop, 0) && boundHolds(wide.bounds.plotRight, 682 - 8) && boundHolds(wide.bounds.plotBottom, 217));
});

test("axis_width of the defaults is the gutter: exact, plus 20 px with a title, and never exact when an override can set it (E14 b)", () => {
  const g = (fields) => {
    const p = ts({}, fields);
    return gutterOf(rulesOf(p), { sure: true, tick0: null });
  };
  assert.deepEqual(g({ defaults: { axis_width: 50 } }), { value: 50, exact: true });
  assert.deepEqual(g({ defaults: { axis_width: 50, axis_label: "Weight" } }), { value: 70, exact: true });
  assert.deepEqual(g({ defaults: { axis_width: 0 } }), { value: 0, exact: true }, "the width can be 0");
  const over = g({ defaults: { axis_width: 50 }, overrides: [{ match: { by: "name", value: "a" }, set: { axis_width: 30 } }] });
  assert.equal(over.exact, false);
  assert.equal(over.value, 30, "the least value that can apply");
  const label = g({ defaults: { axis_width: 50, axis_label: "W" }, overrides: [{ match: { by: "name", value: "a" }, set: { axis_label: "" } }] });
  assert.equal(label.exact, false, "an override can remove the title");
  assert.equal(label.value, 50);
});

test("an override by query sets the same plot as the same members in the defaults (R4 b)", () => {
  const moved = build(ts({ legend: legend({ show: false }) }, { defaults: { axis_width: 44 } }), box);
  const over = build(ts({ legend: legend({ show: false }) }, { overrides: [{ match: { by: "query", value: "A" }, set: { axis_width: 44 } }] }), box);
  assert.deepEqual(part(over, "plot").v, part(moved, "plot").v);
  assert.equal(part(over, "plot").a["data-skel-class"], "exact", "the width of the defaults is exact, and so is the plot");
});

test("the least left gutter is 9 px, or 9 plus the width of the label 0 when the tick 0 is certain (E14)", () => {
  const least = (fields, options) => {
    const p = ts(options, fields);
    const r = rulesOf(p);
    return gutterOf(r, { sure: true, tick0: tick0OfSeries(p, r) ? zeroLabel(r.D) : null });
  };
  assert.equal(least({}).value, 9, "no range: the tick 0 is not certain");
  assert.equal(least({ defaults: { min: 0 } }).value, Math.ceil(9 + 7), "min = 0: the label 0 has one character");
  assert.equal(least({ defaults: { min: 0, unit: "percent" } }).value, Math.ceil(9 + 2 * 7), "the label 0 is 0%");
  assert.equal(least({ defaults: { min: 0, unit: "short", decimals: 0 } }).value, 16);
  assert.equal(least({ defaults: { min: -5, max: 5 } }).value, 16, "a range across 0");
  assert.equal(least({ defaults: { soft_min: 0, soft_max: 500 } }).value, 16, "soft ends 0 and 500");
  assert.equal(least({ defaults: { axis_centered_zero: true } }).value, 16);
  assert.equal(least({ defaults: { min: 5 } }).value, 9, "a range above 0");
  assert.equal(least({ defaults: { min: 0, soft_min: -1 } }).value, 9, "a soft end moves the edge");
  assert.equal(least({ defaults: { min: 0 }, overrides: [{ match: { by: "name", value: "a" }, set: { min: 10 } }] }).value, 9, "an override can move the range");
  assert.equal(least({ defaults: { min: 0 } }, { y_scale: "log" }).value, 9, "a log scale");
  assert.equal(least({ defaults: { min: 0, transform: "negative_y" } }).value, 9);
  assert.equal(least({ defaults: { min: 0, axis_label: "Weight" } }).value, 16 + 28, "the title room: 28 px");
});

test("the right axis: the placement of the defaults gives it, with the left gutter at 8 px (E15)", () => {
  const g = (fields) => {
    const p = ts({}, fields);
    return seriesGeometry(p, rulesOf(p));
  };
  const right = g({ defaults: { axis_placement: "right", axis_label: "W", axis_width: 40 } });
  assert.deepEqual(right.left, { value: 8, exact: true });
  assert.deepEqual(right.right, { value: 60, exact: true });
  assert.deepEqual(g({}).right, { value: 8, exact: true }, "one unit gives one scale");
  assert.deepEqual(g({ defaults: { axis_placement: "left" } }).right, { value: 8, exact: true });
  const unit = g({ overrides: [{ match: { by: "name", value: "a" }, set: { unit: "bytes" } }] });
  assert.deepEqual(unit.right, { value: 8, exact: false }, "a second unit can put a series on the right scale");
  const place = g({ overrides: [{ match: { by: "name", value: "a" }, set: { axis_placement: "right" } }] });
  assert.equal(place.right.exact, false);
  assert.equal(place.left.exact, false);
  assert.equal(place.left.value, 8, "the left axis may not show: the least gutter is 8");
});

test("the pill of the first entry shows only when it is surely on the left scale", () => {
  const pill = (fields) => {
    const p = ts({}, fields);
    return seriesGeometry(p, rulesOf(p)).pill;
  };
  assert.equal(pill({}), true);
  assert.equal(pill({ defaults: { axis_placement: "left" } }), true);
  assert.equal(pill({ defaults: { axis_placement: "right" } }), false, "every series is on the right: the first item moves to the right end");
  assert.equal(pill({ defaults: { axis_placement: "hidden" } }), false);
  assert.equal(pill({ overrides: [{ match: { by: "name", value: "a" }, set: { axis_placement: "right" } }] }), false);
  assert.equal(pill({ overrides: [{ match: { by: "name", value: "a" }, set: { hide_from: { legend: true, tooltip: false, viz: false } } }] }), false);
  const out = build(ts(), box);
  const marks = all(out).filter((n) => n.t === "i");
  assert.deepEqual(marks.map((n) => n.c), [""], "the mark of the entry shows");
  const hidden = build(ts({}, { defaults: { axis_placement: "right" } }), box);
  assert.deepEqual(all(hidden).filter((n) => n.t === "i").map((n) => n.c), ["skel__nopill"], "the entry keeps its line and shows no mark");
});

// ---- The bounds against the loaded panels ---------------------------------------------------------------------------------------------------
test("the otel time series (table legend of three values, no range): the bounds hold for the loaded plot of the 12 panels", () => {
  const p = ts({ legend: legend({ mode: "table", values: ["last_not_null", "mean", "max"], sort_by: "last", sort_desc: true }) }, { defaults: { unit: "percent" } });
  const out = build(p, box);
  const b = out.bounds;
  // Measured on the loaded panels 2 to 21: the left gutter is 25 to 94 px, the plot ends at 662, the top is 8, and the plot is 99 to 126 px high.
  for (const left of [25, 47, 94]) assert.ok(boundHolds(b.plotLeft, left), `left ${left}`);
  assert.ok(boundHolds(b.plotTop, 8));
  assert.ok(boundHolds(b.plotRight, 662));
  for (const bottom of [8 + 99, 8 + 126]) assert.ok(boundHolds(b.plotBottom, bottom), `bottom ${bottom}`);
  assert.ok(boundHolds(b.legendHeight, 83));
  assert.equal(b.plotLeft.kind, "least");
  assert.equal(b.legendHeight.kind, "least");
  assert.equal(part(out, "plot").a["data-skel-class"], "contain:ltrb");
});

test("the bounds are one-sided: a loaded panel that breaks them is caught", () => {
  const out = build(ts(), box);
  assert.equal(boundHolds(out.bounds.plotLeft, 8), false, "the loaded plot starts left of the least gutter");
  assert.equal(boundHolds(out.bounds.plotBottom, 220), false, "the loaded plot is lower than the least legend allows");
  assert.equal(boundHolds(out.bounds.plotTop, 0), false);
});

// ---- The neutral form ---------------------------------------------------------------------------------------------------------------------------
test("a panel with no legend member has the neutral form", () => {
  const p = ts();
  delete p.options.legend;
  assert.deepEqual(build(p, box), { elements: [], bounds: {}, notes: ["no legend member"] });
});

test("the class of a measured part is exact only when every edge is exact", () => {
  const cls = (options, fields) => part(build(ts(options, fields), box), "plot").a["data-skel-class"];
  assert.equal(cls({ legend: legend({ show: false }) }, { defaults: { axis_placement: "hidden" } }), "exact");
  assert.equal(cls({ legend: legend({ show: false }) }, { defaults: { axis_width: 50 } }), "exact");
  assert.equal(cls({ legend: legend({ show: false }) }), "contain:ltrb");
  assert.equal(cls({ legend: legend({ placement: "right" }) }), "contain:ltrb", "a list at the right in 670 px never falls back");
  assert.equal(cls({ legend: legend({ placement: "right", mode: "table", values: ["mean"] }) }), "contain:ltb", "a table beside the plot can fall back");
  assert.equal(cls({}, { defaults: { axis_placement: "hidden" }, overrides: [{ match: { by: "name", value: "a" }, set: { axis_placement: "left" } }] }), "contain:lrb", "no claim on the top margin");
});

test("a legend beside the plot where a fallback can come has no measured part; under the plot it has one", () => {
  const lgOf = (options, size) => part(build(ts(options), size), "legend");
  assert.equal(lgOf({ legend: legend({ placement: "right", mode: "table", values: ["mean"] }) }, box), undefined);
  assert.equal(lgOf({ legend: legend({ placement: "right" }) }, { w: 300, h: 238 }), undefined);
  assert.equal(lgOf({ legend: legend({ placement: "right" }) }, box).a["data-skel-class"], "least");
  assert.equal(lgOf({ legend: legend({ placement: "right" }) }, { w: 260, h: 238 }).a["data-skel-class"], "least", "the fallback is certain");
  assert.equal(lgOf({}, { w: 670, h: 50 }).a["data-skel-class"], "exact", "a short panel: the cap decides the height");
  assert.equal(lgOf({}, box).a["data-skel-class"], "least");
});

test("only the first and the last head text are measured parts: their places are fixed by the document", () => {
  const out = build(ts({ legend: legend({ mode: "table", values: ["mean", "max"] }) }), box);
  const labels = parts(out).filter((n) => n.a["data-skel-part"] === "head-label");
  assert.equal(labels.length, 2);
  assert.ok(part(out, "head"));
});

// ---- The custom properties and the markup --------------------------------------------------------------------------------------------------------
test("the custom properties are px values and the markup has no style attribute", () => {
  for (const p of [ts(), ts({ legend: legend({ mode: "table", values: ["max"], placement: "right" }) }), panel("xychart", { legend: legend() }), bars(), bars({ orientation: "horizontal" })]) {
    const out = p.type === "barchart" ? buildBars(p, box) : build(p, box);
    for (const n of all(out)) {
      assert.ok(!("style" in n.a));
      for (const [name, value] of Object.entries(n.v)) {
        assert.match(name, /^--[a-z-]+$/);
        assert.match(String(value), /^-?[0-9.]+(px|%)?$/);
      }
    }
  }
});

test("the family files read no query, and the preview of the charts draws no colour of the document", () => {
  for (const name of ["timeseries", "barchart"]) {
    const js = readFileSync(new URL(`../../web/js/preview/${name}.js`, import.meta.url), "utf8");
    assert.ok(!/\.queries\b|\.sql\b|\.expr\b|rawSql|query_text|rangeOf/.test(js), `${name} reads a query`);
    assert.ok(!/\bcolor\b\s*:|\.color\b|palette/.test(js.replace(/\/\/.*$/gm, "")), `${name} reads a colour`);
  }
});

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
  const root = out.elements[0];
  assert.equal(root.c, "hbar");
  assert.equal(root.a["data-legend-placement"], "bottom");
  assert.equal(root.v["--hbar-label"], "22px");
  assert.equal(root.v["--band"], "21px");
  assert.ok(byClass(out, "hbar__grid").length === 1 && byClass(out, "hbar__axis").length === 1 && byClass(out, "hbar__legend").length === 1);
  assert.equal(byClass(out, "hbar__axisTitle").length, 0, "no title room unless the title is proven");
  assert.equal(part(out, "plot").a["data-skel-class"], "contain:ltrb");
  assert.equal(out.bounds.plotTop.value, 8);
  assert.equal(out.bounds.plotLeft.value, 22);
  assert.equal(out.bounds.plotRight.value, 670 - 8);
  assert.ok(Math.abs(out.bounds.plotBottom.value - (238 - 21 - (8 + LINE + 4))) < 1e-9);
  assert.ok(boundHolds(out.bounds.legendHeight, 30.857), "one line at least");
});

test("the horizontal legend follows show and placement only: no mode, no hidden entry, no fallback (B27, defect D4)", () => {
  const f = { defaults: { hide_from: { legend: true, tooltip: false, viz: false } } };
  const out = buildBars(bars({ orientation: "horizontal", legend: legend({ mode: "table", placement: "right" }) }, f), box);
  assert.equal(out.elements[0].a["data-legend-placement"], "right");
  const legendEl = byClass(out, "hbar__legend")[0];
  assert.equal(legendEl.k[0].c, "hbar__legendItem", "always a list");
  assert.equal(out.bounds.plotRight.value, 670 - Math.min(40, 0.3 * 670) - 8);
  const off = buildBars(bars({ orientation: "horizontal", legend: legend({ show: false }) }), box);
  assert.equal(byClass(off, "hbar__legend").length, 0);
  assert.equal(off.bounds.plotBottom.value, 238 - 21);
  const narrow = buildBars(bars({ orientation: "horizontal", legend: legend({ placement: "right" }) }), { w: 100, h: 238 });
  assert.equal(narrow.bounds.legendWidth.value, 30, "the legend keeps at most 30 % of the width");
});

test("the horizontal band is axis_width when the document fixes it, the title band is 28 when the title is proven (B24, B25)", () => {
  const h = (fields) => buildBars(bars({ orientation: "horizontal", legend: legend({ show: false }) }, fields), box);
  assert.equal(h({ defaults: { axis_width: 30 } }).elements[0].v["--band"], "30px");
  assert.equal(h({ defaults: { axis_width: 0 } }).elements[0].v["--band"], "0px");
  assert.equal(h({ defaults: { axis_width: 30 }, overrides: [{ match: { by: "name", value: "a" }, set: { axis_width: 10 } }] }).bounds.band.kind, "least");
  const titled = h({ defaults: { axis_label: "Weight" } });
  assert.equal(byClass(titled, "hbar__axisTitle").length, 1);
  assert.equal(titled.bounds.plotBottom.value, 238 - 21 - 28);
  const maybe = h({ overrides: [{ match: { by: "name", value: "a" }, set: { axis_label: "W" } }] });
  assert.equal(byClass(maybe, "hbar__axisTitle").length, 0);
  assert.equal(maybe.bounds.title.kind, "least");
});

test("the bar chart gives the neutral form with no legend member or a warning line", () => {
  const p = bars();
  delete p.options.legend;
  assert.deepEqual(buildBars(p, box).notes, ["no legend member"]);
  assert.deepEqual(buildBars(bars({}, { overrides: [{ match: { by: "regex", value: "/(/" }, set: {} }] }), box).notes, ["warning line"]);
});
