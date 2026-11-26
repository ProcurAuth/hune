// SPDX-License-Identifier: MIT OR Apache-2.0
// The loading preview of the time series and of the xy chart (9.11): web/js/preview/timeseries.js. It also holds the frame of the chart engine that the
// bar chart shares. Each test follows one rule of the council file `timeseries-xychart.md`. It names the JSON member that decides the rule, and the form
// that the preview draws when the document does not decide. The bounds are checked with `boundHolds` against the values that the loaded panel gives
// (measured on the owner dashboard and on the demo dashboards; the browser cases B330 to B334 compare with the loaded panel).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { boundHolds, setMeasurer } from "../../web/js/preview/common.js";
import { build as buildBars } from "../../web/js/preview/barchart.js";
import { chartCore } from "../../web/js/vendor/chdash/app_chart_core.js";
import { boundedYAxis } from "../../web/js/panels/series.js";
import { zoneClock } from "../../web/js/zone.js";
import { BODY_ROW, HEAD_ROW, LINE, barMarks, build, chainOf, fixedAxis, fixedLeft, gutterOf, legendOf, linePath, markStyle, marksOf, rulesOf, seriesGeometry, setGutter, setSpacing, setTextHeight, tick0OfBars, tick0OfSeries, xAxisOf, zeroLabel } from "../../web/js/preview/timeseries.js";

// A text of 7 px for each character: the widths are easy to follow.
setMeasurer((text) => text.length * 7);
setSpacing(0);
setGutter(0);
setTextHeight(14);


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
  // 4.26: `values` = [] gives the column "Name" only (the loaded panel hides the five columns of the engine).
  assert.deepEqual(leg([]).heads, ["Name"]);
  assert.deepEqual(leg(["nope"]).columns, []);
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
  assert.equal(chainOf(260, 40, list).lh, Math.round(8 + 14), "under the plot the list keeps the cap of 35 % (9.19)");
  assert.equal(narrow.canvasW, 260);
  const table = legendOf(ts({ legend: legend({ placement: "right", mode: "table", values: ["mean"] }) }), rulesOf(ts()));
  const t = chainOf(300, 238, table);
  assert.equal(t.fallback, true);
  assert.equal(t.lh, 51, "4 + the head and one row");
  assert.equal(chainOf(300, 100, table).lh, 4 + 35, "the table wrap keeps the cap of 35 % too");
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

test("every entry hidden: no legend, and no gap at the right either: the host is at the bottom (9.19)", () => {
  const f = { defaults: { hide_from: { legend: true, tooltip: false, viz: false } } };
  const bottom = chainOf(670, 238, legendOf(ts({}, f), rulesOf(ts({}, f))));
  assert.equal(bottom.lh, 0);
  assert.equal(bottom.canvasW, 670);
  assert.equal(bottom.draw, false);
  const right = chainOf(670, 238, legendOf(ts({ legend: legend({ placement: "right" }) }, f), rulesOf(ts({}, f))));
  assert.equal(right.canvasW, 670, "the grid keeps no gap: the placement of the host is bottom");
  assert.equal(right.placement, "bottom");
  assert.equal(right.claimRight, true);
  const out = build(ts({ legend: legend({ placement: "right" }) }, f), box);
  assert.equal(byClass(out, "chartCore__legend").length, 0, "a hidden legend is not drawn");
  assert.equal(byClass(out, "skel__host")[0].a["data-legend-placement"], "bottom");
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
  assert.equal(pill({ defaults: { axis_placement: "hidden" } }), true, "every axis hidden: every series goes on the left scale (9.19)");
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
    assert.ok(!/\.queries\b|\.sql\b|\.expr\b|rawSql|query_text/.test(js), `${name} reads a query`);
    assert.ok(!/\bcolor\b\s*:|\.color\b|palette/.test(js.replace(/\/\/.*$/gm, "")), `${name} reads a colour`);
  }
  const css = readFileSync(new URL("../../web/css/panel-loading.css", import.meta.url), "utf8");
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/.test(css.replace(/\/\*[\s\S]*?\*\//g, "")), "a colour literal in the CSS of the previews");
});

// ---- The decorative form of a series (section 3 of the council file) ---------------------------------------------------------------------------------
const marks = (type, options, fields, size = box) => {
  const p = type === "xychart" ? panel("xychart", { legend: legend({ show: false }), draw: "points", point_size: 5, line_width: 1, ...options }, fields) : ts({ legend: legend({ show: false }), draw: "line", interpolation: "linear", line_width: 1, fill_opacity: 0, gradient_mode: "none", show_points: "auto", point_size: 5, ...options }, fields);
  return all(build(p, size));
};
const series = (nodes) => nodes.filter((n) => (n.c || "").includes("skel__series"));
const dots = (nodes) => nodes.filter((n) => n.c === "skel__dot");

test("the line of a time series follows the interpolation of the document: a polyline, a curve, or steps", () => {
  const d = (interpolation) => series(marks("timeseries", { interpolation }))[0].a.d;
  assert.ok(/^M0 64( L[\d. ]+)+$/.test(d("linear")), "only line segments");
  assert.ok(/ C/.test(d("smooth")) && !/ L/.test(d("smooth")), "curve segments");
  assert.ok(/^M0 64 L0 50 L9 50/.test(d("step_before")), "step before: the step is at the left end of the segment");
  assert.ok(/^M0 64 L9 64 L9 50/.test(d("step_after")), "step after: the step is at the right end");
  assert.equal(linePath("linear").split(" L").length - 1, 10, "the same vertices for every form");
  assert.ok(/^M0 64 C/.test(linePath("smooth")) && linePath("smooth").endsWith("100 42"), "the curve passes through the first and the last vertex");
});

test("the line width, the dash and the fill come from the document; a width of 0 draws no line", () => {
  const first = (options, fields) => series(marks("timeseries", options, fields))[0];
  assert.equal(first({ line_width: 3 }).v["--lw"], "3px");
  assert.equal(series(marks("timeseries", { line_width: 0 })).length, 0, "no stroke at width 0");
  assert.equal(first({}).a["stroke-dasharray"], undefined, "solid");
  assert.equal(first({}, { defaults: { line_dash: "dash" } }).a["stroke-dasharray"], "10 10");
  assert.equal(first({}, { defaults: { line_dash: "dot" } }).a["stroke-dasharray"], "0 10");
  assert.equal(first({}, { defaults: { line_dash: "dash", line_dash_array: [4, 2] } }).a["stroke-dasharray"], "4 2");
  const area = (options) => all(build(ts({ legend: legend({ show: false }), draw: "line", interpolation: "linear", line_width: 1, fill_opacity: 0, gradient_mode: "none", ...options }), box)).filter((n) => n.c === "skel__area");
  assert.equal(area({}).length, 0, "no fill with fill_opacity 0");
  assert.equal(area({ fill_opacity: 30 })[0].a["fill-opacity"], "0.3");
  assert.ok(area({ fill_opacity: 30 })[0].a.d.endsWith("L100 100 L0 100 Z"), "the fill closes under the line");
  const fade = all(build(ts({ legend: legend({ show: false }), draw: "line", interpolation: "linear", line_width: 1, fill_opacity: 40, gradient_mode: "opacity" }), box));
  assert.ok(fade.some((n) => n.t === "linearGradient") && fade.some((n) => n.t === "stop" && n.a["stop-opacity"] === "0.4"), "the fill fades to 0");
  assert.match(fade.find((n) => n.c === "skel__fade").a.fill, /^url\(#skel-fade-1\)$/);
});

test("dots: draw points gives dots and no line; show_points always gives both; the size is point_size", () => {
  const p = marks("timeseries", { draw: "points", point_size: 9 });
  assert.equal(series(p).length, 0);
  assert.equal(dots(p).length, 11);
  assert.deepEqual([...new Set(dots(p).map((n) => n.v["--d"]))], ["9px"]);
  const both = marks("timeseries", { show_points: "always" });
  assert.equal(series(both).length, 1);
  assert.equal(dots(both).length, 11);
  assert.equal(dots(marks("timeseries", { show_points: "never" })).length, 0);
  assert.equal(dots(marks("timeseries", { show_points: "auto" })).length, 0, "auto depends on the spacing of the points: data");
  for (const n of dots(p)) assert.match(n.v["--x"], /^[0-9.]+%$/);
});

test("the xy chart: points (the default) give dots, lines give a line, points_and_lines give both; no fill", () => {
  const p = marks("xychart", { draw: "points" });
  assert.deepEqual([series(p).length, dots(p).length], [0, 11]);
  const l = marks("xychart", { draw: "lines" });
  assert.deepEqual([series(l).length, dots(l).length], [1, 0]);
  const b = marks("xychart", { draw: "points_and_lines", point_size: 7 });
  assert.deepEqual([series(b).length, dots(b).length], [1, 11]);
  assert.equal(dots(b)[0].v["--d"], "7px");
  assert.equal(all(build(panel("xychart", { legend: legend({ show: false }), draw: "lines", line_width: 2, fill_opacity: 50 }), box)).filter((n) => n.c === "skel__area").length, 0);
});

test("nothing draws when every series is hidden from the plot, and when an override can change the form (draw)", () => {
  const hidden = { defaults: { hide_from: { legend: false, tooltip: false, viz: true } } };
  assert.equal(marks("timeseries", {}, hidden).filter((n) => n.t === "path" || n.c === "skel__dot" || n.t === "rect").length, 0);
  assert.equal(marks("xychart", {}, hidden).filter((n) => n.c === "skel__dot").length, 0);
  const some = { defaults: hidden.defaults, overrides: [{ match: { by: "name", value: "a" }, set: { hide_from: { legend: false, tooltip: false, viz: false } } }] };
  assert.equal(series(marks("timeseries", {}, some)).length, 1, "an override can show a series");
  assert.equal(markStyle(ts({ draw: "line" }, { overrides: [{ match: { by: "name", value: "a" }, set: { draw: "bars" } }] }), rulesOf(ts({}, { overrides: [{ match: { by: "name", value: "a" }, set: { draw: "bars" } }] }))), null);
});

test("draw bars: bars with a fixed period and heights that vary, in one tone, also for a stack (O3)", () => {
  const b = marks("timeseries", { draw: "bars", stacking: "normal", fill_opacity: 80 });
  const rects = b.filter((n) => n.t === "rect");
  assert.ok(rects.length >= 40);
  assert.equal(new Set(rects.map((n) => n.a.height)).size > 3, true);
  assert.deepEqual([...new Set(rects.map((n) => n.c))], ["skel__bar"]);
  assert.equal(Number(rects[1].a.x) - Number(rects[0].a.x), 14);
  assert.equal(barMarks(100).k.length, 7, "the count follows the width: one bar for each 14 px");
  assert.equal(barMarks(10).k.length, 2, "at least two bars");
  assert.equal(series(b).length, 0, "no line");
  assert.equal(marks("timeseries", { draw: "line" }).filter((n) => n.t === "rect").length, 0);
});

test("the marks use neutral classes only: no colour, no style attribute, no text", () => {
  for (const options of [{}, { draw: "bars" }, { draw: "points" }, { fill_opacity: 50, gradient_mode: "opacity" }]) {
    for (const n of marks("timeseries", options)) {
      assert.ok(!("style" in n.a) && !("text" in n) && !("stroke" in n.a) && !("fill" in n.a && !/^url/.test(n.a.fill)));
      for (const value of Object.values(n.v)) assert.match(String(value), /^-?[0-9.]+(px|%)?$/);
    }
  }
  assert.deepEqual(marksOf(null, 600, 1), []);
});

// ---- The y axis that the document fixes (E17 case i, R9, R10) --------------------------------------------------------------------------------------
const fixed = (fields, options, P = 209) => {
  const p = ts(options, fields);
  return fixedAxis(p, rulesOf(p), P);
};
const range = { defaults: { min: 0, max: 100, unit: "percent" } };

test("a hard min and a hard max give the axis of the loaded panel: the same code, the same labels (E17 i)", () => {
  const f = fixed(range);
  const direct = boundedYAxis(0, 100, 0, 100, 209, false);
  assert.deepEqual(f.axis, direct);
  assert.deepEqual(f.labels, ["0%", "20%", "40%", "60%", "80%", "100%"]);
  assert.equal(fixed({ defaults: { min: 0, max: 1024, unit: "bytes" } }).axis.step % 2, 0, "the unit bytes gives binary steps");
  assert.deepEqual(fixed({ defaults: { min: 0, max: 4, decimals: 1 } }, {}, 160).labels.slice(0, 2), ["0.0", "1.0"], "decimals write each tick with them");
});

test("the fixed axis needs both hard ends, a linear scale and no override that can change it", () => {
  assert.equal(fixed({ defaults: { min: 0 } }), null, "one end is data");
  assert.equal(fixed({ defaults: { min: 0, max: 100, soft_max: 120 } }), null, "a soft end moves the range");
  assert.equal(fixed({ defaults: { min: 5, max: 5 } }), null, "no range");
  assert.equal(fixed(range, { y_scale: "log" }), null);
  assert.equal(fixed(range, { stacking: "percent" }), null, "a percent stack has its own ends");
  assert.ok(fixed(range, { stacking: "normal" }), "a normal stack keeps the hard ends");
  assert.equal(fixed({ defaults: { min: 0, max: 100, transform: "negative_y" } }), null);
  assert.equal(fixed({ defaults: { min: 0, max: 100, axis_placement: "right" } }), null, "the axis is at the right");
  for (const key of ["min", "max", "soft_min", "unit", "decimals", "axis_placement", "axis_border"]) {
    assert.equal(fixed({ ...range, overrides: [{ match: { by: "name", value: "a" }, set: { [key]: 1 } }] }), null, `an override of ${key}`);
  }
  assert.ok(fixed({ ...range, overrides: [{ match: { by: "query", value: "A" }, set: { axis_label: "W" } }] }), "an override by query joins the defaults");
});

test("the gutter of the fixed axis is the widest label plus 9 px, exact when P is exact or when it is the same for every P (R9)", () => {
  const left = (fields, P, pExact, rangeP) => {
    const p = ts({}, fields);
    return fixedLeft(p, rulesOf(p), P, pExact, rangeP);
  };
  // The measure is 7 px for each character: "100%" is 28 px, and ceil(9 + 28) = 37.
  assert.deepEqual(left(range, 209, true, null), { value: 37, exact: true, fixed: fixed(range, {}, 209) });
  assert.equal(left(range, 178, false, null), null, "P is not exact and no range is known");
  const loose = left(range, 178, false, [114, 178]);
  assert.deepEqual([loose.value, loose.exact, loose.fixed], [37, true, null], "the labels have the same widest text for every P: the gutter is exact");
  // A range where the widest label changes with P (the tick step switches at 150 px): the gutter is not used.
  let moving = null;
  for (let max = 2; max < 2000 && !moving; max++) {
    const w = (P) => Math.max(...fixed({ defaults: { min: 0, max } }, {}, P).labels.map((t) => t.length));
    if (w(150) !== w(151)) moving = max;
  }
  assert.ok(moving, "the test finds a range where the widest label changes at P = 150");
  assert.equal(left({ defaults: { min: 0, max: moving } }, 151, false, [140, 160]), null, "the gutter changes inside the range of P");
  assert.ok(left({ defaults: { min: 0, max: moving } }, 151, false, [151, 160]).exact, "and it holds on one side of the switch");
  // The title room and the width of the document.
  assert.equal(left({ defaults: { ...range.defaults, axis_label: "W" } }, 209, true, null).value, 37 + 28, "the title gives 28 px more");
  assert.equal(left({ defaults: { ...range.defaults, axis_label: "W" } }, 209, true, null).fixed, null, "and the title is a text on the canvas: the labels are not drawn");
  assert.deepEqual(left({ defaults: { ...range.defaults, axis_width: 50 } }, 209, true, null).value, 50, "axis_width wins");
  assert.equal(left({ ...range, overrides: [{ match: { by: "name", value: "a" }, set: { axis_label: "W" } }] }, 209, true, null), null, "an override can set a title");
});

test("with the fixed axis and an exact plot height, the y labels are bars at the places of the engine", () => {
  const p = ts({ legend: legend({ show: false }) }, range);
  const out = build(p, box);
  const plot = part(out, "plot");
  const left = px(plot.v["--x"]);
  assert.equal(left, 37);
  assert.equal(plot.a["data-skel-class"], "exact", "the legend is off, so the plot is exact");
  const labels = parts(out).filter((n) => n.a["data-skel-part"] === "label-y");
  assert.equal(labels.length, 6);
  const P = 238 - 8 - 21;
  labels.forEach((n, i) => {
    const w = [2, 3, 3, 3, 3, 4][i] * 7 + 7 * 0;
    void w;
    assert.equal(px(n.v["--h"]), 8);
    const text = ["0%", "20%", "40%", "60%", "80%", "100%"][i];
    assert.equal(px(n.v["--w"]), text.length * 7);
    assert.equal(px(n.v["--x"]) + px(n.v["--w"]), left - 5, "the label ends 5 px before the plot");
    assert.ok(Math.abs(px(n.v["--y"]) + 4 - (8 + P - (i * 20 * P) / 100)) < 0.01, "the centre is the place of the tick");
  });
  assert.equal(parts(out).filter((n) => n.a["data-skel-part"] === "label-x").length, 0, "no range: no x label");
  // A legend whose height is data: the height of the plot is not exact, so there is no label.
  const bottom = build(ts({}, range), box);
  assert.equal(parts(bottom).filter((n) => n.a["data-skel-part"] === "label-y").length, 0);
  assert.equal(part(bottom, "plot").a["data-skel-class"], "contain:ltrb");
  assert.equal(px(part(bottom, "plot").v["--x"]), 37, "but the left edge is exact in value");
  assert.equal(bottom.bounds.plotLeft.kind, "exact");
  // The border moves the labels 4 px out.
  const border = build(ts({ legend: legend({ show: false }) }, { defaults: { ...range.defaults, axis_border: true } }), box);
  const l0 = parts(border).find((n) => n.a["data-skel-part"] === "label-y");
  assert.equal(px(l0.v["--x"]) + px(l0.v["--w"]), 37 - 9);
});

// ---- The x labels from the range of the request (E19, R12) ----------------------------------------------------------------------------------------
const HOUR = 3600000;
const T0 = Date.UTC(2026, 10, 26, 6, 0, 0);
const rt = (r) => ({ rangeOf: (key) => (key === 7 ? r : null) });
const ctxOf = (r, zone = "utc") => ({ rt: rt(r), page: { zone }, key: 7 });
const live = { from_ms: T0, to_ms: T0 + 6 * HOUR };

test("the x labels use the ticks and the layout of the engine over the range of the request", () => {
  const g = { left: 8, top: 0, plotW: 666, plotH: 217, canvasW: 682 };
  const x = xAxisOf(live, "utc", g);
  assert.ok(x.labels.length >= 3);
  assert.ok(x.labels.every((l) => /^\d\d:\d\d$/.test(l.text)), "the clock text: a range of 24 hours or less has no date");
  const ticks = chartCore.timeTicks(live.from_ms, live.to_ms, 666, (t) => t.length * 7, zoneClock("utc"));
  assert.ok(ticks.length === x.ticks.length && x.ticks.length >= x.labels.length, "a vertical line at each tick in the plot, also at a tick with no label");
  for (const l of x.labels) assert.ok(l.x - l.w / 2 >= 0 && l.x + l.w / 2 <= 682, "the label stays inside the canvas");
  for (let i = 1; i < x.labels.length; i++) assert.ok(x.labels[i].x > x.labels[i - 1].x);
  assert.equal(xAxisOf(null, "utc", g), null);
  assert.equal(xAxisOf({ error: "x", from_ms: 1, to_ms: 2 }, "utc", g), null);
  assert.equal(xAxisOf({ from_ms: 5, to_ms: 5 }, "utc", g), null);
  // The zone moves the ticks: a zone of +5:30 puts them at 30 minutes.
  const india = xAxisOf(live, "Asia/Kolkata", g);
  assert.notDeepEqual(india.labels.map((l) => l.text), x.labels.map((l) => l.text));
});

test("the x labels are bars of the measured width centred under the plot, only where the whole plot is exact", () => {
  const options = { legend: legend({ show: false }) };
  const hidden = { defaults: { axis_placement: "hidden" } };
  const out = build(ts(options, hidden), { w: 682, h: 238 }, null, ctxOf(live));
  const labels = parts(out).filter((n) => n.a["data-skel-part"] === "label-x");
  assert.ok(labels.length >= 3);
  for (const n of labels) {
    assert.equal(px(n.v["--h"]), 8);
    assert.equal(px(n.v["--y"]), 217 + 9 + 7 - 4, "the top of the line is 9 px under the plot, the bar is at the middle of a line of 14 px");
    assert.equal(n.a["data-skel-class"], "exact");
  }
  assert.equal(px(labels[0].v["--w"]), 5 * 7);
  assert.equal(all(out).filter((n) => n.t === "line").length >= labels.length, true, "a vertical line at each tick");
  // Not exact: a list legend (its height is data), or a gutter that the data decide.
  assert.equal(parts(build(ts({}, hidden), { w: 682, h: 238 }, null, ctxOf(live))).filter((n) => n.a["data-skel-part"] === "label-x").length, 0);
  assert.equal(parts(build(ts(options), { w: 682, h: 238 }, null, ctxOf(live))).filter((n) => n.a["data-skel-part"] === "label-x").length, 0);
  // No range, no runtime, a range with an error, a panel that draws bars: no label.
  for (const [ctx, fields, o] of [[null, hidden, options], [{ rt: {}, page: {}, key: 7 }, hidden, options], [ctxOf({ error: "x" }), hidden, options], [ctxOf(live), hidden, { ...options, draw: "bars" }], [ctxOf(live), { defaults: { axis_placement: "hidden", draw: "bars" } }, options], [{ ...ctxOf(live), key: 8 }, hidden, options]]) {
    assert.equal(parts(build(ts(o, fields), { w: 682, h: 238 }, null, ctx)).filter((n) => n.a["data-skel-part"] === "label-x").length, 0);
  }
  assert.equal(parts(build(ts(options, { defaults: { axis_placement: "hidden" }, overrides: [{ match: { by: "name", value: "a" }, set: { draw: "bars" } }] }), { w: 682, h: 238 }, null, ctxOf(live))).filter((n) => n.a["data-skel-part"] === "label-x").length, 0, "an override can set bars");
  // An xy chart has no time axis.
  assert.equal(parts(build(panel("xychart", { legend: legend({ show: false }), draw: "points" }), { w: 682, h: 238 }, null, ctxOf(live))).filter((n) => n.a["data-skel-part"] === "label-x").length, 0);
});

test("the labels follow the box: a new width gives new places, and a short range of seconds gives its own ticks", () => {
  const hidden = { defaults: { axis_placement: "hidden" } };
  const wide = parts(build(ts({ legend: legend({ show: false }) }, hidden), { w: 1000, h: 238 }, null, ctxOf(live))).filter((n) => n.a["data-skel-part"] === "label-x");
  const narrow = parts(build(ts({ legend: legend({ show: false }) }, hidden), { w: 300, h: 238 }, null, ctxOf(live))).filter((n) => n.a["data-skel-part"] === "label-x");
  assert.ok(wide.length > narrow.length);
  const secs = parts(build(ts({ legend: legend({ show: false }) }, hidden), { w: 682, h: 238 }, null, ctxOf({ from_ms: T0, to_ms: T0 + 60000 }))).filter((n) => n.a["data-skel-part"] === "label-x");
  assert.ok(secs.length >= 3);
});
