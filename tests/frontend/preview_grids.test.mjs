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
  // E1: more than 250,000 columns over the range of the request (5 hours = 18,000,000 ms).
  const range = { from: Date.UTC(2026, 10, 13), to: Date.UTC(2026, 10, 13) + 5 * 3600000 };
  assert.equal(noPicture(hp({ x_buckets: { mode: "size", value: 1 } }), null, range), "E1", "a column of 1 ms");
  assert.equal(noPicture(hp({ x_buckets: { mode: "size", value: 73 } }), null, range), "", "246,576 columns of 73 ms: under the limit");
  assert.equal(noPicture(hp({ x_buckets: { mode: "size", value: 72 } }), null, range), "E1", "250,001 columns of 72 ms: one more than the limit");
  assert.equal(noPicture(hp({ x_buckets: { mode: "size", value: 1 } }), null, null), "", "no range: no proof");
  assert.equal(noPicture(hp({ calculate: false, x_buckets: { mode: "size", value: 1 } }), null, range), "", "the shape bucketed has no step");
  assert.equal(noPicture(hp({ x_buckets: { mode: "count", value: 60 } }), null, range), "", "a count of columns");
  assert.equal(heat(hp({ x_buckets: { mode: "size", value: 1 } }), BOX, null, frame()).notes[0], "no-picture:E1");
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

test("histogram: a table with no value column has the head Name only, and the fallback keeps the cap of 35 %", () => {
  assert.deepEqual(legendFacts(hs({ legend: lg({ mode: "table", values: [] }) }), null).heads, []);
  assert.deepEqual(legendFacts(hs({ legend: lg({ mode: "table", values: ["nope", "min"] }) }), null).heads, ["min"]);
  const right = hist(hs({ legend: lg({ placement: "right", mode: "table", values: [] }) }), BOX, null, {});
  assert.equal(rectOf(parts(right, "legend")[0])[2], 38 + 28 + 15, "the column Name and the scroll bar");
  // A short body under the fallback: the table wrap and the list keep floor(0.35 H) at most.
  const narrow = { w: 100, h: 100 };
  const table = hist(hs({ legend: lg({ placement: "right", mode: "table", values: ["min"] }) }), narrow, null, {});
  assert.equal(num(parts(table, "legend")[0], "--h"), 4 + 35, "the cap is 35 px, not 8rem");
  const list = hist(hs({ legend: lg({ placement: "right" }) }), { w: 100, h: 50 }, null, {});
  assert.equal(num(parts(list, "legend")[0], "--h"), 8 + 17, "the list keeps floor(0.35 * 50) = 17 px at most");
});

test("histogram: the legend is empty or unknown when a member can hide every entry", () => {
  const hide = { defaults: { hide_from: { legend: true, tooltip: false, viz: false } }, overrides: [] };
  const empty = hist(hs({ legend: lg({ placement: "right" }) }, hide), BOX, null, {});
  assert.deepEqual(empty.notes, ["legend:empty"]);
  assert.equal(rectOf(parts(empty, "plot")[0])[0] + rectOf(parts(empty, "plot")[0])[2], 682 - 8, "a hidden legend takes no room: the placement is bottom");
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
    // The range of the request is allowed (9.11): the preview reads it with `rangeOf` of grids.js and never makes a new "now".
    assert.ok(!/Date\.now|new Date\(\)/.test(js.replace(/\/\/.*$/gm, "")), `${name}: no new now`);
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

// ---- the time axis (the range of the request) --------------------------------------------------------------------------------------
import { rangeOf, textBoxHeight, timeAxis } from "../../web/js/preview/grids.js";

const T0 = Date.UTC(2026, 10, 13);
const HOUR = 3600000;
// The context of the frame: the runtime gives the range of the key, the page gives the zone and the range of the dashboard.
const frame = (from = T0, to = T0 + 6 * HOUR, extra = {}) => ({ rt: { rangeOf: () => ({ from_ms: from, to_ms: to, error: null }) }, page: { zone: "utc", linkContext: () => ({ from: 0, to: 1 }) }, key: 1, ...extra });

test("the range of the request is the range of the runtime for the key, else the range of the page, else none", () => {
  assert.deepEqual(rangeOf(frame()), { from: T0, to: T0 + 6 * HOUR });
  assert.deepEqual(rangeOf({ page: { linkContext: () => ({ from: 5, to: 9 }) } }), { from: 5, to: 9 }, "the editor draft has no rangeOf");
  assert.equal(rangeOf(frame(T0, T0)), null, "a range that is not finite and ascending");
  assert.deepEqual(rangeOf({ rt: { rangeOf: () => ({ error: "bad" }) }, key: 1, page: { linkContext: () => ({ from: 5, to: 9 }) } }), { from: 5, to: 9 }, "a range with an error falls back to the page");
  assert.equal(rangeOf(null), null);
  assert.equal(rangeOf({}), null);
});

test("timeAxis: the labels and the lines are the ticks of the engine over the range, in the zone", () => {
  const axis = timeAxis({ from: T0, to: T0 + 6 * HOUR, left: 50, plotW: 600, width: 682, zone: "utc" });
  // 600 px for 6 hours: the engine picks the step of 30 minutes (a label is 35 px wide with the measurer of the test).
  assert.deepEqual(axis.labels.map((l) => l.label), ["00:00", "00:30", "01:00", "01:30", "02:00", "02:30", "03:00", "03:30", "04:00", "04:30", "05:00", "05:30", "06:00"]);
  for (const l of axis.labels) assert.ok(l.x - l.w / 2 >= 1 && l.x + l.w / 2 <= 681, "a label stays inside the canvas");
  assert.equal(axis.lines.length >= axis.labels.length, true);
  assert.ok(Math.abs(axis.lines[0].x - 50) < 1e-9, "the tick at the start of the range is at the left edge");
  assert.equal(textBoxHeight(12), 14, "no page: the fixed height of the test");
});

test("heatmap: a hidden axis and a fixed axis draw the x labels and the vertical lines when the range is known, else nothing", () => {
  const hidden = hp({ y_axis: "hidden", show_legend: false });
  const none = heat(hidden, BOX, null, {});
  assert.equal(parts(none, "label-x").length + parts(none, "grid-x").length, 0, "no range: the band stays empty");
  const out = heat(hidden, BOX, null, frame());
  const labels = parts(out, "label-x");
  const lines = parts(out, "grid-x");
  assert.ok(labels.length >= 6 && lines.length >= labels.length, `${labels.length} labels, ${lines.length} lines`);
  const [plot] = parts(out, "plot");
  const [px0, py0, pw, ph] = rectOf(plot);
  for (const l of lines) assert.deepEqual([num(l, "--y"), num(l, "--h"), num(l, "--w")], [py0, ph, 1], "a line spans the plot height, one device pixel wide");
  // The label bars: 8 px high, centred on the text box that starts 9 px under the plot (a box of 14 px in the test).
  for (const l of labels) assert.deepEqual([num(l, "--h"), num(l, "--y") + 4], [8, py0 + ph + 9 + 7]);
  assert.equal(labels[0].a["data-skel-class"], EXACT);
  assert.ok(Math.abs(num(lines[0], "--x") - px0) <= 0.5, "the first tick is at the left edge of the plot");
  assert.ok(boundHolds(out.bounds.xLabels, labels.length));
  // A fixed axis too: the left edge is exact. A bounded left edge draws nothing: the tick step depends on the plot width.
  const fixed = hp({ y_buckets: { mode: "size", value: 10, scale: "linear" }, y_min: 0, y_max: 100 }, { defaults: { unit: "ms", decimals: 0 }, overrides: [] });
  assert.ok(parts(heat(fixed, BOX, null, frame()), "label-x").length >= 6);
  assert.equal(parts(heat(hp(), BOX, null, frame()), "label-x").length, 0, "bounded left edge: no x label");
  // A longer range changes the ticks (the day ticks write the date).
  const days = heat(hidden, BOX, null, frame(T0, T0 + 3 * 24 * HOUR));
  assert.ok(parts(days, "label-x").length >= 3);
  assert.ok(parts(days, "label-x").length !== labels.length || num(parts(days, "label-x")[0], "--w") !== num(labels[0], "--w"), "the range decides the ticks");
});

test("timeline: the time axis of a state timeline is drawn with an exact plot box, and never for a status history or an undecided box", () => {
  const off = { show_labels: false, legend: { show: false } };
  const out = line(ts("state_timeline", off), BOX, null, frame());
  const labels = parts(out, "label-x");
  const lines = parts(out, "grid-x");
  assert.ok(labels.length >= 6 && lines.length >= labels.length);
  // The bars sit on the bottom edge of the canvas box: 12 px less half of the text box (7 px) less half the bar (4 px) from the bottom.
  for (const l of labels) assert.equal(num(l, "--b"), 12 - 7 - 4);
  assert.equal(num(lines[0], "--x"), 8, "show_labels false: the gutter is 8, and the first tick is on it");
  assert.equal(parts(line(ts("status_history", off), BOX, null, frame()), "label-x").length, 0, "the axis of a status history is data");
  assert.equal(parts(line(ts("state_timeline", off), BOX, null, {}), "label-x").length, 0, "no range: no label");
  assert.equal(parts(line(ts("state_timeline", { show_labels: true, legend: { show: false } }), BOX, null, frame()), "label-x").length, 0, "the names are data: the gutter is bounded");
  const named = { defaults: { display_name: "Status" }, overrides: [] };
  assert.ok(parts(line(ts("state_timeline", { legend: { show: false } }, named), BOX, null, frame()), "label-x").length >= 6, "one fixed name: the gutter is exact");
  // The legend of the case D is data: its height and its width decide the canvas.
  const palette = { defaults: { color: { mode: "palette", fixed: null }, display_name: "Status" }, overrides: [] };
  assert.equal(parts(line(ts("state_timeline", {}, palette), BOX, null, frame()), "label-x").length, 0, "the legend of the states is data");
  // A legend of the case C at the right changes the canvas width: no label. At the bottom the width is the box width.
  const thresholds = { defaults: { display_name: "Status", color: { mode: "thresholds", fixed: null }, thresholds: { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] } }, overrides: [] };
  assert.ok(parts(line(ts("state_timeline", {}, thresholds), BOX, null, frame()), "label-x").length >= 6);
  assert.equal(parts(line(ts("state_timeline", { legend: { placement: "right" } }, thresholds), BOX, null, frame()), "label-x").length, 0);
});
