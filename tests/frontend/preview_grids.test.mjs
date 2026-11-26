// SPDX-License-Identifier: MIT OR Apache-2.0
// The pure rules of the loading previews of the grid family (9.11): web/js/preview/heatmap.js, histogram.js and timeline.js.
// Each test builds a preview from a synthetic panel, with a measurer of fixed widths, and checks one rule of the council files with the JSON
// member that decides it. Where the document does not decide, the test checks the neutral form or the one-sided bound.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { EXACT, LEAST, MOST, boundHolds, setMeasurer } from "../../web/js/preview/common.js";
import { compareLabel } from "../browser/lib/preview_measure.mjs";
import { matchClass, possible, setGutter, warningValues } from "../../web/js/preview/grids.js";
import { axisCase, build as heat, fixedTicks, noPicture } from "../../web/js/preview/heatmap.js";
import { build as hist, legendFacts } from "../../web/js/preview/histogram.js";
import { build as line, gutter, legendCase } from "../../web/js/preview/timeline.js";

// One character is 7 px at 12 px (the width follows the size). The gutter of the scroll bar is 15 px.
setMeasurer((text, font) => text.length * 7 * (Number(/(\d+(?:\.\d+)?)px/.exec(font)[1]) / 12));
setGutter(15);

const parts = (out, part) => out.elements.flatMap(function walk(n) { return [n, ...(n.k || []).flatMap(walk)]; }).filter((n) => n.a && n.a["data-skel-part"] === part);
const num = (n, name) => parseFloat(n.v[name]);
const rectOf = (n) => [num(n, "--x"), num(n, "--y"), num(n, "--w"), num(n, "--h")];

// ---- heatmap ---------------------------------------------------------------------------------------------------------------------
const HEAT = {
  calculate: true, x_buckets: { mode: "count", value: 60 }, y_buckets: { mode: "size", value: 2, scale: "log2" }, y_axis: "left", y_reverse: false, y_min: null, y_max: null,
  color: { mode: "scheme", scheme: "spectral", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null },
  cell_gap: 1, hide_le: 0, hide_ge: null, show_legend: true, tooltip: { mode: "single" },
};
const hp = (over = {}, fields = { defaults: {}, overrides: [] }, data) => ({ id: 1, type: "heatmap", options: { ...HEAT, ...over, color: { ...HEAT.color, ...(over.color || {}) } }, fields, data: data || { query: { source: "s", format: "table", sql: ["SELECT 1"] } }, transforms: [] });
const BOX = { w: 682, h: 238 };

test("heatmap: the plot top, height and right edge follow the members show_legend and y_axis", () => {
  const out = heat(hp(), BOX, null, {});
  const [plot] = parts(out, "plot");
  // 238 less the legend of 46 less the top of 8 and the band of 21: 163. The right edge is 682 less 8.
  assert.deepEqual([rectOf(plot)[1], rectOf(plot)[3], rectOf(plot)[0] + rectOf(plot)[2]], [8, 163, 674]);
  const hidden = rectOf(parts(heat(hp({ y_axis: "hidden", show_legend: false }), BOX, null, {}), "plot")[0]);
  assert.deepEqual(hidden, [8, 0, 666, 217], "a hidden axis: top 0, left 8, no legend");
  const short = rectOf(parts(heat(hp(), { w: 682, h: 48 }, null, {}), "plot")[0]);
  assert.equal(short[3], 40, "the canvas has a floor of 60 px, the plot a floor of 40 px");
});

test("heatmap: the left edge is exact for a hidden axis and a fixed linear axis, else it is bounded at 9", () => {
  assert.equal(axisCase(hp({ y_axis: "hidden" }), null).kind, "H");
  const fixed = hp({ y_buckets: { mode: "size", value: 10, scale: "linear" }, y_min: 0, y_max: 100 }, { defaults: { unit: "ms", decimals: 0 }, overrides: [] });
  assert.deepEqual(axisCase(fixed, null), { kind: "F", unit: "ms", decimals: 0 });
  const out = heat(fixed, BOX, null, {});
  assert.equal(parts(out, "plot")[0].a["data-skel-class"], EXACT);
  assert.ok(boundHolds(out.bounds.plotLeft, 51), "the gutter is exact: 6 ticks, widest label 100 ms");
  const bounded = heat(hp(), BOX, null, {});
  assert.equal(parts(bounded, "plot")[0].a["data-skel-class"], MOST);
  assert.equal(num(parts(bounded, "plot")[0], "--x"), 9, "the least left edge is 9");
  assert.ok(boundHolds(bounded.bounds.plotLeft, 53) && !boundHolds(bounded.bounds.plotLeft, 8), "the loaded left edge is 9 or more");
  assert.ok(boundHolds(bounded.bounds.plotWidth, 621), "the loaded width is at most the preview width");
});

test("heatmap: the fixed axis needs a linear scale, two numbers, y_max above y_min and one unit and decimals", () => {
  const base = { y_buckets: { mode: "size", value: 10, scale: "linear" }, y_min: 0, y_max: 100 };
  assert.equal(axisCase(hp({ ...base, y_buckets: { ...base.y_buckets, scale: "log2" } }), null).kind, "", "a log scale");
  assert.equal(axisCase(hp({ ...base, y_max: null }), null).kind, "", "y_max not set");
  assert.equal(axisCase(hp({ ...base, y_max: 0 }), null).kind, "", "y_max not above y_min");
  assert.equal(axisCase(hp({ ...base, calculate: false }), null).kind, "", "a bucketed frame");
  const byName = { defaults: { unit: "ms" }, overrides: [{ match: { by: "name", value: "x" }, set: { unit: "s" } }] };
  assert.equal(axisCase(hp(base, byName), null).kind, "", "a name override can change the unit: the data decide");
  const byQuery = { defaults: { unit: "ms" }, overrides: [{ match: { by: "query", value: "A" }, set: { unit: "s", decimals: 1 } }] };
  assert.deepEqual(axisCase(hp(base, byQuery), null), { kind: "F", unit: "s", decimals: 1 }, "a query override on the only query matches every field");
  const other = { defaults: { unit: "ms" }, overrides: [{ match: { by: "query", value: "Z" }, set: { unit: "s" } }] };
  assert.deepEqual(axisCase(hp(base, other), null), { kind: "F", unit: "ms", decimals: null }, "a query that is not a ref never matches");
});

test("heatmap: the ticks of the fixed axis use the nice step, the unit and the reversal", () => {
  const o = { y_min: 0, y_max: 100, y_reverse: false };
  const ticks = fixedTicks(o, 163, "none", null);
  assert.deepEqual(ticks.map((t) => t.text), ["0", "20", "40", "60", "80", "100"]);
  assert.equal(ticks[0].y, 163, "the lowest value is at the bottom");
  assert.equal(ticks[5].y, 0);
  const rev = fixedTicks({ ...o, y_reverse: true }, 163, "none", null);
  assert.equal(rev[0].y, 0, "a reversed axis puts the lowest value at the top");
  assert.equal(fixedTicks(o, 163, "percent", 1)[1].text, "20.0%");
});

test("heatmap: a fixed axis draws one label bar and one grid line for each tick, and no other axis does", () => {
  const fixed = hp({ y_buckets: { mode: "size", value: 10, scale: "linear" }, y_min: 0, y_max: 100 });
  const out = heat(fixed, BOX, null, {});
  assert.equal(parts(out, "label").length, 6);
  assert.equal(parts(out, "grid").length, 6);
  const label = parts(out, "label")[0];
  assert.equal(num(label, "--w"), 7, "the width of the text '0' with the measurer");
  assert.equal(num(label, "--h"), 8);
  assert.equal(num(label, "--x") + num(label, "--w"), num(parts(out, "plot")[0], "--x") - 5, "a label ends 5 px before the plot");
  for (const o of [hp(), hp({ y_axis: "hidden" }), hp({ calculate: false })]) {
    const none = heat(o, BOX, null, {});
    assert.equal(parts(none, "label").length + parts(none, "grid").length, 0);
  }
});

test("heatmap: the legend band is 46 px, the bar is 300 px for a body of 350 px or more, else the width less 50", () => {
  const wide = heat(hp(), BOX, null, {});
  assert.deepEqual(rectOf(parts(wide, "scale")[0]), [25, 192 + 10, 300, 9]);
  const legend = rectOf(parts(wide, "legend")[0]);
  assert.deepEqual([legend[1], legend[3]], [192, 46]);
  assert.equal(parts(wide, "legend")[0].a["data-skel-class"], LEAST, "the ends are data: the width is a lower bound");
  const narrow = heat(hp(), { w: 328, h: 238 }, null, {});
  assert.equal(rectOf(parts(narrow, "scale")[0])[2], 278);
  assert.equal(rectOf(parts(heat(hp(), { w: 40, h: 238 }, null, {}), "scale")[0])[2], 10, "the bar has a floor of 10 px");
  assert.equal(parts(heat(hp({ show_legend: false }), BOX, null, {}), "legend").length, 0, "no legend: no band");
});

test("heatmap: the legend ends are bars only for a number that the document fixes", () => {
  const none = heat(hp(), BOX, null, {});
  assert.equal(parts(none, "legend-label").length, 0, "the ends are data");
  const left = heat(hp({ color: { min: 5 } }), BOX, null, {});
  assert.equal(parts(left, "legend-label").length, 1, "the left end only: the right end is data");
  assert.equal(parts(left, "legend")[0].a["data-skel-class"], LEAST);
  const both = heat(hp({ color: { min: 0, max: 1000 } }), BOX, null, {});
  const [a, b] = parts(both, "legend-label");
  assert.equal(num(a, "--x"), 25, "the left end starts at 25");
  assert.ok(Math.abs(num(a, "--w") - 7 * (11 / 12)) < 0.001, "the text of 0 at 11 px");
  const legend = rectOf(parts(both, "legend")[0]);
  assert.equal(num(b, "--x") + num(b, "--w"), legend[0] + legend[2], "the right end ends at the end of the legend");
  assert.equal(parts(both, "legend")[0].a["data-skel-class"], EXACT);
  assert.equal(parts(heat(hp({ color: { max: 5 } }), BOX, null, {}), "legend-label").length, 0, "a right end alone moves with the left end: no bar");
});

test("heatmap: the cases where no picture can come give an empty preview", () => {
  assert.equal(noPicture(hp({ hide_le: 5, hide_ge: 3 }), null), "E2", "no whole count is above 5 and below 3");
  assert.equal(noPicture(hp({ hide_le: null, hide_ge: 0 }), null), "E2", "a count of 0 is not below 0");
  assert.equal(noPicture(hp({ hide_le: 500000, hide_ge: null }), null), "E2", "no count is above the highest count of 500,000");
  assert.equal(noPicture(hp({ hide_le: 5, hide_ge: 7 }), null), "", "6 passes");
  assert.equal(noPicture(hp({ calculate: false, hide_le: 5, hide_ge: 5 }), null), "E3");
  assert.equal(noPicture(hp({ calculate: false, hide_le: 5, hide_ge: 5.5 }), null), "", "a bucketed value can be 5.2");
  const hiddenViz = { defaults: { hide_from: { legend: false, tooltip: false, viz: true } }, overrides: [] };
  assert.equal(noPicture(hp({}, hiddenViz), null), "E4");
  const partly = { defaults: hiddenViz.defaults, overrides: [{ match: { by: "name", value: "x" }, set: { hide_from: { legend: false, tooltip: false, viz: false } } }] };
  assert.equal(noPicture(hp({}, partly), null), "", "an override can show a field");
  const out = heat(hp({ hide_le: 5, hide_ge: 3 }), BOX, null, {});
  assert.deepEqual([out.elements.length, out.notes], [0, ["no-picture:E2"]]);
});

test("heatmap: a regex override that does not compile gives the neutral form, and axis_width has no effect", () => {
  const bad = { defaults: {}, overrides: [{ match: { by: "regex", value: "/(/" }, set: { unit: "ms" } }] };
  assert.deepEqual(warningValues(hp({}, bad)), ["/(/"]);
  assert.deepEqual(heat(hp({}, bad), BOX, null, {}).notes, ["warning"]);
  const a = heat(hp(), BOX, null, {});
  const b = heat(hp({}, { defaults: { axis_width: 99, axis_placement: "right" }, overrides: [] }), BOX, null, {});
  assert.deepEqual(a.elements, b.elements, "axis_width and axis_placement are not read");
});

// ---- the shared rules of the fields ----------------------------------------------------------------------------------------------
test("the possible values of a key follow the overrides in order, with the classes never, every and maybe", () => {
  const p = (fields, data) => ({ ...hp({}, fields, data), fields });
  const data = { query: { source: "s", format: "table", sql: ["SELECT 1"], ref: "A" } };
  const two = { query: data.query, extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 2"] }] };
  const fields = { defaults: { unit: "ms" }, overrides: [{ match: { by: "name", value: "x" }, set: { unit: "s" } }] };
  assert.deepEqual(possible(p(fields, data), null, "unit"), ["ms", "s"], "a name matcher may match");
  const q = { defaults: { unit: "ms" }, overrides: [{ match: { by: "query", value: "A" }, set: { unit: "s" } }] };
  assert.deepEqual(possible(p(q, data), null, "unit"), ["s"], "a query on the only visible query matches every field");
  assert.deepEqual(possible(p(q, two), null, "unit"), ["ms", "s"], "with two queries it may match");
  const wrong = { defaults: { unit: "ms" }, overrides: [{ match: { by: "query", value: "Z" }, set: { unit: "s" } }] };
  assert.deepEqual(possible(p(wrong, data), null, "unit"), ["ms"], "a ref that no query has never matches");
  const bad = { defaults: { unit: "ms" }, overrides: [{ match: { by: "regex", value: "/(/" }, set: { unit: "s" } }] };
  assert.deepEqual(possible(p(bad, data), null, "unit"), ["ms"], "a pattern that does not compile never matches");
  const transformed = { ...p(q, data), transforms: [{ id: "x" }] };
  assert.equal(matchClass({ by: "query", value: "A" }, transformed, null), "maybe", "a transform can make fields with no ref");
  const reuse = { id: 2, data: { reuse: { panel: 1, with_transforms: false } }, transforms: [], fields: q, options: {} };
  assert.equal(matchClass({ by: "query", value: "A" }, reuse, { sections: [{ panels: [p(q, data)] }] }), "every", "the source panel owns the queries");
  assert.equal(matchClass({ by: "query", value: "A" }, reuse, null), "maybe", "an unknown source");
});

// ---- histogram -------------------------------------------------------------------------------------------------------------------
const HIST = { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } };
const lg = (over) => ({ ...HIST.legend, ...over });
const hs = (over = {}, fields = { defaults: {}, overrides: [] }) => ({ id: 1, type: "histogram", options: { ...HIST, ...over }, fields, data: { query: { source: "s", format: "table", sql: ["SELECT 1"] } }, transforms: [] });

test("histogram: the top is 8, the right edge is the box less 8, the left edge is the gutter of the label 0 and the bottom follows the legend", () => {
  const out = hist(hs({ legend: lg({ show: false }) }), BOX, null, {});
  const [plot] = parts(out, "plot");
  // 238 less the band of 21 and the top of 8: 209. The label 0 is 7 px wide: the gutter is 16.
  assert.deepEqual(rectOf(plot), [16, 8, 658, 209]);
  assert.ok(boundHolds(out.bounds.plotTop, 8) && boundHolds(out.bounds.plotRight, 674));
  assert.ok(boundHolds(out.bounds.plotLeft, 30) && !boundHolds(out.bounds.plotLeft, 10), "the left edge only moves right");
  assert.equal(plot.a["data-skel-class"], MOST, "the gutter is bounded");
  assert.deepEqual(out.notes, ["legend:off"]);
});

test("histogram: the label 0 ends 5 px before the plot and the line of the tick 0 is at the base", () => {
  const out = hist(hs({ legend: lg({ show: false }) }), BOX, null, {});
  const [label] = parts(out, "label");
  assert.equal(num(label, "--x") + num(label, "--w"), 16 - 5);
  assert.equal(num(label, "--y") + 4, 8 + 209, "the label is centred on the base line");
  const [grid] = parts(out, "grid");
  assert.deepEqual([num(grid, "--x"), num(grid, "--y"), num(grid, "--h")], [16, 217, 1]);
  assert.equal(parts(out, "label").length + parts(out, "grid").length, 2, "no other label and no other line: the counts are data");
  assert.equal(label.a["data-skel-class"], LEAST);
});

test("histogram: the least legend at the bottom is one list item of 31 px, or the head and one row of a table", () => {
  const list = hist(hs(), BOX, null, {});
  assert.equal(rectOf(parts(list, "plot")[0])[3], 238 - 31 - 29, "the plot height follows the legend");
  const [legend] = parts(list, "legend");
  assert.equal(num(legend, "--y"), 238 - 31);
  assert.ok(Math.abs(num(legend, "--h") - 30.857) < 0.01);
  assert.equal(parts(list, "mark").length, 1);
  assert.equal(num(parts(list, "mark")[0], "--x"), 4);
  assert.ok(Math.abs(num(parts(list, "mark")[0], "--y") - 207 - (8 + (18.8568 - 4) / 2)) < 0.01, "the pill is centred in the first line");
  assert.equal(parts(list, "legend-label").length, 0, "the name is data");
  const table = hist(hs({ legend: lg({ mode: "table", values: ["min"] }) }), BOX, null, {});
  assert.equal(rectOf(parts(table, "plot")[0])[3], 238 - 47 - 29);
  assert.equal(parts(table, "legend-head").length, 1);
  assert.equal(num(parts(table, "legend-head")[0], "--x"), 30, "the head Name starts after the gutter of the sort arrow");
  // The cap of 35 %: a short body keeps its legend inside 0.35 of the height.
  const short = hist(hs(), { w: 682, h: 48 }, null, {});
  assert.equal(num(parts(short, "legend")[0], "--h"), 12 + 16);
});

test("histogram: the legend is empty or unknown when a member can hide every entry", () => {
  const hide = { defaults: { hide_from: { legend: true, tooltip: false, viz: false } }, overrides: [] };
  const empty = hist(hs({ legend: lg({ placement: "right" }) }, hide), BOX, null, {});
  assert.deepEqual(empty.notes, ["legend:empty-right"]);
  assert.equal(rectOf(parts(empty, "plot")[0])[0] + rectOf(parts(empty, "plot")[0])[2], 682 - 16, "the empty column keeps the gap of 8 px");
  assert.equal(parts(empty, "legend").length, 0);
  const bottom = hist(hs({}, hide), BOX, null, {});
  assert.equal(rectOf(parts(bottom, "plot")[0])[3], 209, "no legend room at the bottom");
  const maybe = { defaults: {}, overrides: [{ match: { by: "name", value: "x" }, set: { hide_from: { legend: true, tooltip: false, viz: false } } }] };
  const unknown = hist(hs({}, maybe), BOX, null, {});
  assert.deepEqual(unknown.notes, ["legend:unknown"]);
  assert.equal(parts(unknown, "legend").length, 0, "undecided: the least legend is none");
  const facts = legendFacts(hs({}, hide), null);
  assert.deepEqual([facts.EMPTY, facts.ON], [true, false]);
});

test("histogram: a display name that the document writes in full gives the name of the legend and the shape", () => {
  const named = { defaults: { display_name: "n" }, overrides: [] };
  assert.deepEqual(legendFacts(hs({ combine: true }, named), null).name, "Count", "combine with names that are known: one series Count");
  assert.equal(legendFacts(hs({ combine: false }, named), null).name, "n", "one name for every series");
  assert.equal(legendFacts(hs({ combine: true }, { defaults: {}, overrides: [] }), null).name, null, "the names are data");
  assert.equal(legendFacts(hs({ combine: true }, { defaults: { display_name: "xMin" }, overrides: [] }), null).COUNT, false, "xMin gives the shape bucketed");
  assert.equal(legendFacts(hs({ combine: true }, { defaults: { display_name: "${__field.name}" }, overrides: [] }), null).COUNT, false, "a template is data");
  const out = hist(hs({ combine: true }, named), BOX, null, {});
  const [name] = parts(out, "legend-label");
  assert.equal(num(name, "--w"), 35, "Count is 5 characters");
  assert.equal(num(name, "--x"), 26);
  assert.equal(parts(out, "legend")[0].a["data-skel-class"], EXACT, "one entry of a known name: the legend is exact");
});

test("histogram: a legend at the right takes its least width, and moves under the plot when the body is too narrow", () => {
  const right = hist(hs({ legend: lg({ placement: "right" }) }), BOX, null, {});
  const legend = rectOf(parts(right, "legend")[0]);
  assert.equal(legend[2], 40 + 15, "40 px of the item and 15 px of the scroll bar");
  assert.equal(rectOf(parts(right, "plot")[0])[0] + rectOf(parts(right, "plot")[0])[2], 682 - 8 - 55 - 8, "the plot ends before the gap and the legend");
  assert.deepEqual(right.notes, ["legend:right"]);
  const narrow = hist(hs({ legend: lg({ placement: "right" }) }), { w: 270, h: 238 }, null, {});
  assert.deepEqual(narrow.notes, ["legend:fallback"], "270 is less than 55 + 228");
  assert.equal(rectOf(parts(narrow, "plot")[0])[0] + rectOf(parts(narrow, "plot")[0])[2], 270 - 8, "the fallback keeps the plot to the full width");
  assert.ok(Math.abs(num(parts(narrow, "legend")[0], "--h") - 26.857) < 0.01, "padding 4, one line, padding 4");
  const table = hist(hs({ legend: lg({ placement: "right", mode: "table", values: ["min", "max"] }) }), BOX, null, {});
  // Name, Min and Max: 38 px of padding and the text for each head (7 px for each character), and the scroll bar of 15 px.
  assert.equal(rectOf(parts(table, "legend")[0])[2], 38 + 28 + 38 + 21 + 38 + 21 + 15);
  assert.equal(parts(table, "legend")[0].a["data-skel-class"], LEAST);
});

test("histogram: with one series named Count and the percent stacking every label follows from the height", () => {
  const named = { defaults: { display_name: "n" }, overrides: [] };
  const out = hist(hs({ combine: true, stacking: "percent", legend: lg({ show: false }) }, named), BOX, null, {});
  assert.ok(parts(out, "label").length >= 3, "the ticks of the axis from 0 to 2");
  assert.equal(parts(out, "plot")[0].a["data-skel-class"], EXACT, "labels, gutter and plot are exact");
  assert.ok(boundHolds(out.bounds.plotLeft, rectOf(parts(out, "plot")[0])[0]));
  const plain = hist(hs({ stacking: "percent", legend: lg({ show: false }) }), BOX, null, {});
  assert.equal(parts(plain, "label").length, 1, "a percent chart with unknown names keeps the label 0 only");
});

test("histogram: a panel that can never draw a chart has an empty preview", () => {
  const none = { defaults: { hide_from: { legend: false, tooltip: false, viz: true } }, overrides: [] };
  const out = hist(hs({}, none), BOX, null, {});
  assert.deepEqual([out.elements.length, out.notes], [0, ["no-chart"]]);
  const some = { defaults: none.defaults, overrides: [{ match: { by: "query", value: "A" }, set: { hide_from: { legend: false, tooltip: false, viz: false } } }] };
  assert.ok(hist(hs({}, some), BOX, null, {}).elements.length > 0, "a query override on the only query shows the series");
});

test("histogram: bucket and legend members that the rules ignore change nothing", () => {
  const a = hist(hs(), BOX, null, {});
  const b = hist(hs({ bucket_count: 5, bucket_size: 2, bucket_offset: 3, fill_opacity: 0, line_width: 0, tooltip: { mode: "hidden" } }), BOX, null, {});
  assert.deepEqual(a.elements, b.elements);
  const c = hist(hs({}, { defaults: { unit: "ms", decimals: 3, min: 0, max: 9, axis_width: 99 }, overrides: [] }), BOX, null, {});
  assert.deepEqual(a.elements, c.elements);
});

// ---- state timeline and status history -------------------------------------------------------------------------------------------
const STATE = { merge_values: true, show_values: "never", align_values: "left", row_height: 80, line_width: 0, fill_opacity: 75, span_nulls: false, show_labels: true, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } };
const ts = (type = "state_timeline", over = {}, fields = { defaults: {}, overrides: [] }) => ({ id: 1, type, options: { ...STATE, ...over, legend: { ...STATE.legend, ...(over.legend || {}) } }, fields, data: { query: { source: "s", format: "table", sql: ["SELECT 1"] } }, transforms: [] });
const steps = (...v) => ({ mode: "absolute", steps: v.map((value, k) => ({ value, color: ["green", "orange", "red"][k] })) });
const thresholds = (t, extra = {}) => ({ defaults: { color: { mode: "thresholds", fixed: null }, thresholds: t, ...extra }, overrides: [] });

test("timeline: the legend case follows the colour mode, the thresholds and the hide_from member", () => {
  assert.equal(legendCase(ts("state_timeline", { legend: { show: false } }), null).kind, "A", "legend off");
  assert.equal(legendCase(ts("state_timeline"), null).kind, "B", "a state timeline with no colour is a gradient: no entry");
  assert.deepEqual(legendCase(ts("status_history"), null), { kind: "C", items: ["< 80", "80+"] }, "a status history has the thresholds 80 by default");
  assert.equal(legendCase(ts("state_timeline", {}, { defaults: { color: { mode: "palette", fixed: null } }, overrides: [] }), null).kind, "D", "the states give the entries");
  const hidden = { defaults: { color: { mode: "palette", fixed: null }, hide_from: { legend: true, tooltip: false, viz: false } }, overrides: [] };
  assert.equal(legendCase(ts("state_timeline", {}, hidden), null).kind, "B", "every entry hidden");
  const stepsHidden = thresholds(steps(null, 50), { hide_from: { legend: true, tooltip: false, viz: false } });
  assert.equal(legendCase(ts("state_timeline", {}, stepsHidden), null).kind, "C", "the thresholds legend ignores hide_from.legend");
  const one = thresholds(steps(null));
  assert.equal(legendCase(ts("state_timeline", {}, one), null).kind, "D", "one step gives the states");
  const maybe = { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [{ match: { by: "name", value: "x" }, set: { color: { mode: "thresholds", fixed: null }, thresholds: steps(null, 5) } }] };
  assert.equal(legendCase(ts("state_timeline", {}, maybe), null).kind, "D", "an override may give a thresholds legend");
});

test("timeline: the texts of a thresholds legend are the texts of the steps, with the unit and the decimals", () => {
  const pct = thresholds(steps(null, 50, 90), { unit: "percent", decimals: 1 });
  assert.deepEqual(legendCase(ts("state_timeline", {}, pct), null).items, ["< 50.0%", "50.0%+", "90.0%+"]);
  const percentage = thresholds({ mode: "percentage", steps: steps(null, 30).steps });
  assert.deepEqual(legendCase(ts("state_timeline", {}, percentage), null).items, ["< 30%", "30%+"]);
  const unitMaybe = { defaults: { color: { mode: "thresholds", fixed: null }, thresholds: steps(null, 80), unit: "ms" }, overrides: [{ match: { by: "name", value: "x" }, set: { unit: "s" } }] };
  assert.equal(legendCase(ts("status_history", {}, unitMaybe), null).kind, "D", "the unit is not fixed: the texts are data");
});

test("timeline: the left gutter is 8 without names, the gutter of a fixed name, else the least 21", () => {
  assert.deepEqual(gutter(ts("state_timeline", { show_labels: false }), null), { left: 8, exact: true, why: "no names" });
  const named = { defaults: { display_name: "Status" }, overrides: [] };
  const g = gutter(ts("state_timeline", {}, named), null);
  assert.deepEqual([g.left, g.exact], [Math.round(6 * 7 + 20.5), true], "round(width + 20.5)");
  assert.deepEqual(gutter(ts(), null), { left: 21, exact: false, why: "names are data" });
  const template = { defaults: { display_name: "${__field.name}" }, overrides: [] };
  assert.equal(gutter(ts("state_timeline", {}, template), null).exact, false, "a template is data");
  const empty = { defaults: { display_name: "${__series.name}" }, overrides: [] };
  assert.equal(gutter(ts("state_timeline", {}, empty), null).exact, false, "the series name is empty: the base name shows");
  const byName = { defaults: { display_name: "Status" }, overrides: [{ match: { by: "name", value: "x" }, set: { display_name: "Other" } }] };
  assert.equal(gutter(ts("state_timeline", {}, byName), null).exact, false, "an override may rename a row");
});

test("timeline: the rows envelope is one tint on the clip box, and the canvas is exact or bounded by the legend case", () => {
  const b = build(ts(), BOX);
  const main = b.elements[0].k[0];
  assert.equal(main.a["data-skel-part"], "plot");
  assert.equal(main.a["data-skel-class"], EXACT);
  const rows = main.k[0];
  assert.equal(rows.c, "skel__rows");
  assert.equal(rows.a["data-skel-class"], MOST, "the gutter is bounded");
  assert.equal(num(rows, "--x"), 21);
  assert.ok(boundHolds(b.bounds.gutter, 49) && !boundHolds(b.bounds.gutter, 8));
  assert.ok(boundHolds(b.bounds.canvasWidth, 682) && boundHolds(b.bounds.canvasHeight, 238), "no legend room: the canvas is the host");
  const d = build(ts("state_timeline", {}, { defaults: { color: { mode: "palette", fixed: null } }, overrides: [] }), BOX);
  assert.equal(d.elements[0].k[0].a["data-skel-class"], MOST, "case D: the canvas can only get smaller");
  assert.ok(boundHolds(d.bounds.canvasHeight, 207.2) && !boundHolds(d.bounds.canvasHeight, 250));
  const off = build(ts("state_timeline", { show_labels: false, legend: { show: false } }), BOX);
  assert.equal(off.elements[0].k[0].k[0].a["data-skel-class"], EXACT);
  assert.equal(num(off.elements[0].k[0].k[0], "--x"), 8);
});
function build(panel, box) {
  return line(panel, box, null, {});
}

test("timeline: a thresholds legend uses the classes of the loaded legend, with text bars and the fallback decision", () => {
  const panel = ts("status_history", { legend: { placement: "right" } });
  const out = build(panel, BOX);
  const root = out.elements[0];
  assert.equal(root.c, "timeline timeline--right");
  assert.deepEqual(root.a, {}, "wide enough: beside the plot");
  const ul = root.k[1];
  assert.equal(ul.t, "ul");
  assert.equal(ul.c, "timeline__legend");
  assert.equal(ul.k.length, 2);
  assert.equal(ul.v["--legend-max"], "83px", "35 % of 238");
  const bars = ul.k.map((li) => li.k[1]);
  assert.deepEqual(bars.map((b) => num(b, "--w")), [4 * 7, 3 * 7], "'< 80' and '80+'");
  // 8 + 32 + 28 = 68 px beside the plot, and 228 px for the plot: under 296 px the legend moves under the plot.
  assert.deepEqual(build(panel, { w: 295, h: 238 }).elements[0].a, { "data-fallback": "" });
  assert.deepEqual(build(panel, { w: 296, h: 238 }).elements[0].a, {});
  assert.equal(build(ts("status_history"), BOX).elements[0].c, "timeline timeline--bottom");
  assert.ok(out.notes.includes("legend:right") && build(panel, { w: 200, h: 238 }).notes.includes("legend:fallback"));
});

test("timeline: a panel that can never draw has an empty preview, and the draw members have no effect", () => {
  const none = { defaults: { hide_from: { legend: false, tooltip: false, viz: true } }, overrides: [] };
  const out = build(ts("state_timeline", {}, none), BOX);
  assert.deepEqual([out.elements.length, out.notes], [0, ["never-drawn"]]);
  const a = build(ts(), BOX);
  const b = build(ts("state_timeline", { merge_values: false, span_nulls: true, row_height: 10, line_width: 9, fill_opacity: 0, show_values: "always", align_values: "right", tooltip: { mode: "hidden" } }), BOX);
  assert.deepEqual(a.elements, b.elements);
  const bad = { defaults: {}, overrides: [{ match: { by: "regex", value: "/(/" }, set: { unit: "ms" } }] };
  assert.deepEqual(build(ts("state_timeline", {}, bad), BOX).notes, ["warning"]);
});

test("the harness compares a bar with a text by width and vertical centre, and one-sided in x for a bound", () => {
  const text = [20, 10, 30, 15]; // a text box of 15 px
  assert.equal(compareLabel("exact", [20, 13.5, 30, 8], text).verdict, "ok", "the same centre, the same x");
  assert.equal(compareLabel("exact", [20, 15, 30, 8], text).verdict, "fail", "1 px lower");
  assert.equal(compareLabel("exact", [20, 13.5, 31, 8], text).verdict, "fail", "wider");
  assert.equal(compareLabel("least", [7, 13.5, 30, 8], text).verdict, "ok", "at the left of the loaded text: the gutter can only grow");
  assert.equal(compareLabel("least", [21, 13.5, 30, 8], text).verdict, "fail", "at the right of the loaded text");
  assert.equal(compareLabel("data", [0, 0, 1, 1], text).verdict, "data");
});

// ---- the rules of the three files ---------------------------------------------------------------------------------------------------
test("the grid previews use no colour of the document, no style attribute and no query text", () => {
  for (const name of ["heatmap", "histogram", "timeline", "grids"]) {
    const js = readFileSync(new URL(`../../web/js/preview/${name}.js`, import.meta.url), "utf8");
    assert.ok(!/\.sql\b|\.queries\b|\.expr\b/.test(js.replace(/\/\/.*$/gm, "")), `${name}: no query text`);
    assert.ok(!/setAttribute\("style"|innerHTML|resolveFill|fieldColorValue|\.color\.fixed/.test(js), `${name}: no colour and no style attribute`);
    assert.ok(!/rangeOf/.test(js.replace(/\/\/.*$/gm, "")), `${name}: no resolved range`);
  }
  const css = readFileSync(new URL("../../web/css/panel-loading.css", import.meta.url), "utf8");
  const section = css.slice(css.indexOf("family: heatmap"), css.indexOf("family: logs"));
  assert.ok(!/#[0-9a-f]{3,8}\b|rgba?\(|hsl/i.test(section), "the sections of the grid family have no colour literal");
  // A custom property has a number with px or %.
  for (const out of [heat(hp({ color: { min: 0, max: 9 } }), BOX, null, {}), hist(hs(), BOX, null, {}), build(ts("status_history"), BOX)]) {
    const walk = (n) => [n, ...(n.k || []).flatMap(walk)];
    for (const n of out.elements.flatMap(walk)) for (const v of Object.values(n.v || {})) assert.match(v, /^-?[0-9.]+(px|%)?$/);
  }
});
