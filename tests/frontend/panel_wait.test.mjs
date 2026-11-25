// SPDX-License-Identifier: MIT OR Apache-2.0
// The wait of a panel (9.11): first load and reload are two cases, the preview of each panel type, and the test of the same data.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { SIZES, WAITING, labelWidth, legendSize, legendSpec, plotGutters, previewOf, rangeKey, sameFrames, skeletonKind, smoothPath, waitMode } from "../../web/js/panel_wait.js";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("a first load shows the preview and the dots: nothing is on the screen", () => {
  for (const state of ["idle", "queued", "loading"]) assert.equal(waitMode(state, "nothing"), "first", state);
});

test("a reload keeps the content or the text No data, and shows the dots only", () => {
  for (const state of ["idle", "queued", "loading"]) {
    assert.equal(waitMode(state, "content"), "reload", state);
    assert.equal(waitMode(state, "empty"), "reload", state);
  }
});

test("a panel that does not wait shows no sign, whatever is on the screen", () => {
  for (const state of ["ready", "empty", "error", "timeline-limit", "bucket-limit", "geo-limit", "text-limit"]) {
    for (const shown of ["content", "empty", "nothing"]) assert.equal(waitMode(state, shown), "", `${state} ${shown}`);
  }
  assert.deepEqual([...WAITING].sort(), ["idle", "loading", "queued"]);
});

test("a try that failed leaves nothing on the screen: the next wait is a first load again", () => {
  // The error overlay is not content and not the text No data: the frame reads "nothing" once it hides the overlay for the new try.
  assert.equal(waitMode("loading", "nothing"), "first");
});

test("a request that another one replaced leaves no dots: the state that follows ends the wait", () => {
  // The runtime sets the state of an aborted request itself (runtime.js). Each state that is not a waiting state clears the mode.
  assert.equal(waitMode("loading", "content"), "reload");
  assert.equal(waitMode("ready", "content"), "");
  assert.equal(waitMode("error", "nothing"), "");
});

// ---- The preview ----------------------------------------------------------------------------------------------------------------

const TYPES = ["timeseries", "xychart", "barchart", "stat", "gauge", "bargauge", "table", "piechart", "heatmap", "histogram", "state_timeline", "status_history", "logs", "geomap"];
const panel = (type, options = {}, fields = {}) => ({ id: 1, type, options, fields });

// Every node of a tree, in order.
function walk(n, out = []) {
  out.push(n);
  for (const k of n.k) walk(k, out);
  return out;
}
const classes = (tree) => walk(tree).flatMap((n) => (n.c || "").split(" ").filter(Boolean));
const count = (tree, cls) => classes(tree).filter((c) => c === cls).length;
const has = (tree, cls) => count(tree, cls) > 0;
const find = (tree, cls) => walk(tree).find((n) => (n.c || "").split(" ").includes(cls));

test("each queried panel type has a preview, and a text has none", () => {
  for (const type of TYPES) {
    assert.ok(skeletonKind(type), type);
    const tree = previewOf(panel(type));
    assert.ok(tree && tree.c === "skel", type);
    assert.equal(tree.a["data-kind"], skeletonKind(type), type);
  }
  assert.equal(skeletonKind("text"), "");
  assert.equal(skeletonKind("not-a-type"), "");
  assert.equal(skeletonKind("constructor"), "", "an inherited name is not a type");
  assert.equal(previewOf(panel("text")), null);
  assert.equal(previewOf(null), null);
  // Every kind of preview is used by a type: no dead preview.
  assert.equal(new Set(TYPES.map(skeletonKind)).size, 12);
});

test("a tree holds only nodes with a tag, custom properties, attributes and children, and no colour or style attribute", () => {
  const allowed = new Set(["div", "svg", "path"]);
  for (const type of TYPES) {
    for (const options of [{}, { legend: { show: true, placement: "right", mode: "table", values: ["mean", "max"] } }, { orientation: "horizontal", graph_mode: "area", display_mode: "lcd", reduce: { values: true } }]) {
      for (const n of walk(previewOf(panel(type, options)))) {
        assert.ok(allowed.has(n.t), `${type} tag ${n.t}`);
        for (const [name, value] of Object.entries(n.v)) {
          assert.match(name, /^--[a-z-]+$/, `${type} property ${name}`);
          assert.match(String(value), /^-?[0-9.]+(px|%)?$/, `${type} ${name}=${value}`);
        }
        assert.ok(!("style" in n.a) && !("fill" in n.a) && !("stroke" in n.a), `${type} attributes`);
        assert.ok(Array.isArray(n.k));
      }
    }
  }
});

test("the legend of the options: shown at the bottom as a list when the options say nothing, hidden when show is false", () => {
  assert.deepEqual(legendSpec(panel("timeseries")), { place: "bottom", mode: "list", items: 4, columns: 0 });
  assert.equal(legendSpec(panel("timeseries", { legend: { show: false } })), null);
  assert.deepEqual(legendSpec(panel("timeseries", { legend: { show: true, placement: "right", mode: "table", values: ["mean", "max", "last"] } })), { place: "right", mode: "table", items: 3, columns: 3 });
});

test("the room of the legend is the room that the real legend takes (measured), and nothing when it is off", () => {
  const bottomList = legendSpec(panel("timeseries"));
  assert.equal(legendSize(bottomList), SIZES.LEGEND_BOTTOM);
  assert.equal(legendSize(bottomList, true), SIZES.PIE_BOTTOM, "the pie has a line of its own and a gap of 4 px");
  assert.equal(legendSize(null), 0);
  assert.equal(legendSize({ place: "bottom", mode: "table", items: 3, columns: 2 }), SIZES.LEGEND_TABLE_HEAD + SIZES.LEGEND_TABLE_ROWS * SIZES.LEGEND_TABLE_ROW);
  assert.equal(legendSize({ place: "right", mode: "list", items: 4, columns: 0 }), SIZES.LEGEND_RIGHT_LIST);
  const table = (columns) => legendSize({ place: "right", mode: "table", items: 3, columns });
  assert.equal(table(2), SIZES.LEGEND_RIGHT_BASE + 2 * SIZES.LEGEND_RIGHT_COLUMN);
  assert.equal(table(0), table(1), "a table has one value column at least");
  assert.ok(legendSize({ place: "right", mode: "table", items: 3, columns: 2 }, true) < table(2), "the legend of a pie is narrower");
});

test("the width of a y label follows the unit and the decimals of the panel", () => {
  const w = (unit, decimals) => labelWidth(panel("timeseries", {}, { defaults: { unit, decimals } }));
  assert.equal(w("none", 0), Math.ceil(3 * SIZES.CHAR));
  assert.equal(w("ms", 0), Math.ceil(6 * SIZES.CHAR), "500 ms");
  assert.ok(w("ms", 0) > w("none", 0));
  assert.equal(w("percent", 0), Math.ceil(4 * SIZES.CHAR), "500%");
  assert.ok(w("none", 2) > w("none", 0), "decimals widen the label");
  assert.equal(labelWidth(panel("timeseries")), Math.ceil(3 * SIZES.CHAR), "no unit and no decimals");
});

test("the gutters of a plot: the labels and 9 px at the left, 8 px at the right, 21 px under, 8 px above", () => {
  const g = plotGutters(panel("timeseries", {}, { defaults: { unit: "ms", decimals: 0 } }));
  assert.deepEqual(g, { left: Math.ceil(6 * SIZES.CHAR) + 9, right: 8, bottom: 21, top: 8 });
  const hidden = plotGutters(panel("timeseries", {}, { defaults: { axis_placement: "hidden" } }));
  assert.deepEqual([hidden.left, hidden.top], [8, 0], "no y axis: the plot starts at the top edge and at 8 px from the left");
  assert.equal(plotGutters(panel("timeseries", {}, { defaults: { axis_width: 80 } })).left, 80);
  const right = plotGutters({ ...panel("timeseries", {}, { defaults: { unit: "ms" } }), fields: { defaults: { unit: "ms" }, overrides: [{ match: { by: "name", value: "x" }, set: { axis_placement: "right" } }] } });
  assert.ok(right.right > 8, "an override that puts a field at the right gives the right gutter room for labels");
  assert.equal(plotGutters(panel("heatmap"), { yAxis: false }).left, 8);
});

test("a smooth path has a segment for each interval, and closes under the line when it is an area", () => {
  const ys = [10, 40, 20, 60];
  const d = smoothPath(ys);
  assert.match(d, /^M0 10C/);
  assert.equal((d.match(/C/g) || []).length, ys.length - 1);
  assert.ok(d.endsWith("100 60"));
  assert.ok(smoothPath(ys, { close: true }).endsWith("L100 100L0 100Z"));
});

test("a time series shows lines with an area, the axes and the legend, and no dots", () => {
  const tree = previewOf(panel("timeseries", { legend: { show: true, placement: "bottom", mode: "list", values: [] } }, { defaults: { unit: "ms", decimals: 0 } }));
  assert.equal(count(tree, "skel__line"), 3, "three lines");
  assert.equal(count(tree, "skel__fill"), 1, "an area under the first one");
  assert.ok(walk(tree).filter((n) => n.t === "path").every((n) => /^M/.test(n.a.d)));
  assert.ok(!classes(tree).some((c) => /dot/.test(c)), "no dot");
  assert.equal(count(tree, "skel__grid"), 5);
  assert.equal(count(tree, "skel__ylab"), 5, "five y labels");
  assert.equal(count(tree, "skel__xlab"), 6, "six x labels");
  assert.equal(tree.a["data-legend"], "bottom");
  assert.equal(tree.v["--legend"], "30.8px", "the height that the real legend takes");
  const area = find(tree, "skel__area");
  assert.deepEqual([area.v["--left"], area.v["--right"], area.v["--top"], area.v["--bottom"]], ["55px", "8px", "8px", "21px"]);
  const legend = find(tree, "skel__legend");
  assert.equal(count(legend, "skel__item"), 4);
  assert.equal(count(legend, "skel__sw--pill"), 4, "the swatch of a series is a pill");
});

test("a time series without a legend reserves nothing for it, and one with the legend at the right reserves its width", () => {
  const off = previewOf(panel("timeseries", { legend: { show: false } }));
  assert.equal(off.a["data-legend"], "none");
  assert.equal(off.v["--legend"], "0px");
  assert.ok(!has(off, "skel__legend"));
  const right = previewOf(panel("timeseries", { legend: { show: true, placement: "right", mode: "list", values: [] } }));
  assert.equal(right.a["data-legend"], "right");
  assert.equal(right.v["--legend"], `${SIZES.LEGEND_RIGHT_LIST}px`);
  const table = previewOf(panel("timeseries", { legend: { show: true, placement: "bottom", mode: "table", values: ["mean", "max"] } }));
  assert.equal(table.a["data-mode"], "table");
  const rows = find(table, "skel__legend").k;
  assert.equal(rows.length, 3, "a head and two rows");
  assert.equal(rows[0].c.includes("skel__lhead"), true);
  assert.equal(count(find(table, "skel__legend"), "skel__lval"), 3 * 2, "a cell for each value column in each row");
});

test("a hidden y axis gives no label column and a plot that starts at the top edge", () => {
  const tree = previewOf(panel("timeseries", {}, { defaults: { axis_placement: "hidden" } }));
  assert.ok(!has(tree, "skel__ycol"));
  const area = find(tree, "skel__area");
  assert.deepEqual([area.v["--left"], area.v["--top"]], ["8px", "0px"]);
});

test("a time series drawn with bars shows bars and no lines, stacked when the options stack them", () => {
  const bars = previewOf(panel("timeseries", { draw: "bars", stacking: "normal", legend: { show: false } }));
  assert.equal(count(bars, "skel__bar"), 12);
  assert.equal(count(bars, "skel__bar--stack"), 12);
  assert.ok(!has(bars, "skel__line") && !has(bars, "skel__svg"));
  assert.equal(count(previewOf(panel("timeseries", { draw: "bars" })), "skel__bar--stack"), 0);
});

test("a bar chart follows its orientation, its stacking and its values", () => {
  const horizontal = previewOf(panel("barchart", { orientation: "horizontal", show_values: "always", legend: { show: false } }));
  assert.equal(count(horizontal, "skel__hrow"), 5);
  assert.equal(count(horizontal, "skel__hval"), 5, "a value at the end of each bar");
  assert.equal(find(horizontal, "skel__vgrids").v["--label"], `${SIZES.HBAR_LABELS}px`, "the label column of the real chart");
  assert.equal(horizontal.v["--legend"], "0px");
  assert.equal(count(previewOf(panel("barchart", { orientation: "horizontal", show_values: "never" })), "skel__hval"), 0);
  assert.equal(count(previewOf(panel("barchart", { orientation: "horizontal", stacking: "normal" })), "skel__hbar--stack"), 5);
  const vertical = previewOf(panel("barchart", { orientation: "vertical" }));
  assert.equal(count(vertical, "skel__bar"), 7);
  assert.ok(has(vertical, "skel__ycol") && has(vertical, "skel__legend"));
});

test("a pie chart is a ring or a disc, with a legend of its own under or beside it", () => {
  const donut = previewOf(panel("piechart", { pie_type: "donut", legend: { show: true, placement: "bottom", mode: "list" } }));
  assert.ok(has(donut, "skel__pie") && !has(donut, "skel__pie--disc"));
  assert.ok(has(previewOf(panel("piechart", { pie_type: "pie" })), "skel__pie--disc"));
  assert.equal(donut.v["--legend"], `${SIZES.PIE_BOTTOM}px`);
  assert.equal(count(find(donut, "skel__legend"), "skel__sw--dot"), 4, "the swatch of a part is a round one");
  const right = previewOf(panel("piechart", { legend: { show: true, placement: "right", mode: "table", values: ["value", "percent"] } }));
  assert.equal(right.v["--legend"], `${SIZES.PIE_RIGHT_BASE + 2 * SIZES.PIE_RIGHT_COLUMN}px`);
  assert.equal(previewOf(panel("piechart", { legend: { show: false } })).v["--legend"], "0px");
});

test("a stat shows a line under its value only when the graph mode is on", () => {
  assert.ok(has(previewOf(panel("stat", { graph_mode: "area" })), "skel__spark"));
  assert.equal(count(previewOf(panel("stat", { graph_mode: "area" })), "skel__fill"), 1);
  assert.equal(count(previewOf(panel("stat", { graph_mode: "line" })), "skel__fill"), 0);
  assert.ok(!has(previewOf(panel("stat", { graph_mode: "none" })), "skel__spark"));
  assert.ok(!has(previewOf(panel("stat")), "skel__spark"));
});

test("gauges and bar gauges: one for each value when the option asks for one value of each series", () => {
  assert.equal(count(previewOf(panel("gauge", { reduce: { values: true } })), "skel__gauge"), 3);
  assert.equal(count(previewOf(panel("gauge", { reduce: { values: false } })), "skel__gauge"), 1);
  assert.equal(count(previewOf(panel("bargauge", { reduce: { values: true }, orientation: "vertical", display_mode: "lcd" })), "skel__bg"), 4);
  const one = previewOf(panel("bargauge", { orientation: "horizontal" }));
  assert.equal(count(one, "skel__bg"), 1);
  assert.equal(one.a["data-orient"], "horizontal");
  assert.equal(previewOf(panel("bargauge", { orientation: "vertical" })).a["data-orient"], "vertical");
  assert.equal(count(previewOf(panel("bargauge", { display_mode: "lcd" })), "skel__bg--lcd"), 1);
});

test("a table has the columns that the document names (3 to 8), a head, and a foot when a field has one", () => {
  const named = (names, footer = false) => ({ defaults: {}, overrides: names.map((value, i) => ({ match: { by: "name", value }, set: footer && i === 0 ? { footer: { reducers: ["count"] } } : {} })) });
  const t = (names, footer) => previewOf({ id: 1, type: "table", options: {}, fields: named(names, footer) });
  assert.equal(t([]).v["--cols"], "3", "three at least");
  assert.equal(t(["a", "b", "c", "d", "e"]).v["--cols"], "5");
  assert.equal(t(["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]).v["--cols"], "8", "eight at most");
  assert.equal(count(t(["a", "b", "c", "d"]), "skel__th"), 4);
  assert.equal(count(t(["a", "b", "c", "d"]), "skel__row"), 10);
  assert.ok(!has(t(["a", "b", "c"]), "skel__tfoot"));
  assert.ok(has(t(["a", "b", "c"], true), "skel__tfoot"));
});

test("a heatmap shows its color scale only when the option asks for it; a timeline reserves no legend; logs follow show_time", () => {
  const withScale = previewOf(panel("heatmap", { show_legend: true }));
  assert.ok(has(withScale, "skel__scale"));
  assert.equal(withScale.v["--legend"], `${SIZES.SCALE}px`);
  assert.ok(!has(previewOf(panel("heatmap", { show_legend: false })), "skel__scale"));
  assert.ok(!has(previewOf(panel("heatmap")), "skel__scale"));
  assert.equal(previewOf(panel("heatmap", { y_axis: "hidden" })).k[0].k.some((n) => n.c === "skel__ycol"), false, "no y axis, no y labels");
  const timeline = previewOf(panel("state_timeline", { legend: { show: true, placement: "bottom", mode: "list" } }));
  assert.ok(!has(timeline, "skel__legend"), "the legend comes from the states of the data");
  assert.equal(timeline.v["--legend"], "0px");
  assert.equal(count(previewOf(panel("logs", { show_time: true })), "skel__ltime"), 10);
  assert.equal(count(previewOf(panel("logs", { show_time: false })), "skel__ltime"), 0);
});

test("the histogram counts have four digits and the heatmap and the timeline keep their axes", () => {
  const h = previewOf(panel("histogram", {}, { defaults: { unit: "ms", decimals: 0 } }));
  assert.equal(find(h, "skel__area").v["--left"], `${Math.ceil(4 * SIZES.CHAR) + 9}px`, "a count has no unit");
  assert.equal(count(h, "skel__bar--flush"), 10);
  assert.equal(count(previewOf(panel("state_timeline")), "skel__lane"), 4);
  assert.equal(count(previewOf(panel("geomap")), "skel__pin"), 5);
});

// ---- The same data -------------------------------------------------------------------------------------------------------------

const frame = (values, extra = {}) => ({ interval_ms: 1000, rows: values[0].length, columns: [{ name: "t", type: "DateTime", kind: "time" }, { name: "v", type: "UInt64", kind: "number" }], values, ...extra });

test("an answer with the same data is the same, whatever its request members say", () => {
  const a = [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]], { query_id: "q1", stats: { elapsed_ms: 10 }, sql: "SELECT 1 -- 1", etag: "x" }) }];
  const b = [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]], { query_id: "q2", stats: { elapsed_ms: 99 }, sql: "SELECT 1 -- 2", etag: "y" }) }];
  assert.equal(sameFrames(a, b), true);
  assert.equal(sameFrames(a, a), true);
});

test("a changed value, column, interval, ref or count is a different answer", () => {
  const base = () => [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]]) }];
  assert.equal(sameFrames(base(), [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 7]]) }]), false, "a value");
  assert.equal(sameFrames(base(), [{ ref: "A", frame: frame([[1, 2], [4, 5]]) }]), false, "a row count");
  assert.equal(sameFrames(base(), [{ ref: "B", frame: frame([[1, 2, 3], [4, 5, 6]]) }]), false, "a ref");
  assert.equal(sameFrames(base(), [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]], { interval_ms: 2000 }) }]), false, "an interval");
  const other = frame([[1, 2, 3], [4, 5, 6]]);
  other.columns[1].name = "w";
  assert.equal(sameFrames(base(), [{ ref: "A", frame: other }]), false, "a column");
  assert.equal(sameFrames(base(), base().concat(base())), false, "a count of answers");
  assert.equal(sameFrames(null, base()), false);
  assert.equal(sameFrames(base(), null), false);
  assert.equal(sameFrames([{ ref: "A", frame: null }], base()), false);
});

test("a value of another type or a null is a different value", () => {
  const a = [{ ref: "A", frame: frame([[1, null], ["x", "y"]]) }];
  assert.equal(sameFrames(a, [{ ref: "A", frame: frame([[1, null], ["x", "y"]]) }]), true);
  assert.equal(sameFrames(a, [{ ref: "A", frame: frame([[1, 0], ["x", "y"]]) }]), false);
  assert.equal(sameFrames(a, [{ ref: "A", frame: frame([["1", null], ["x", "y"]]) }]), false);
});

test("a very large answer counts as different, so the test stays cheap", () => {
  const big = () => [{ ref: "A", frame: frame([new Array(300000).fill(1), new Array(300000).fill(2)]) }];
  assert.equal(sameFrames(big(), big()), false);
});

test("a chart draws again when its range changes, and not when the range is the same", () => {
  assert.equal(rangeKey({ from_ms: 1, to_ms: 2 }), "1:2");
  assert.equal(rangeKey({ from_ms: 1, to_ms: 2 }), rangeKey({ from_ms: 1, to_ms: 2 }));
  assert.notEqual(rangeKey({ from_ms: 1, to_ms: 2 }), rangeKey({ from_ms: 1, to_ms: 3 }));
  assert.equal(rangeKey(null), "");
  assert.equal(rangeKey({ error: "x", from_ms: 1, to_ms: 2 }), "");
  assert.equal(rangeKey({ from_ms: NaN, to_ms: 2 }), "");
});

test("the frame uses the model: the mode is set with the state, the same data does not draw, the module loads before the old drawing goes", () => {
  const js = read("web/js/panel_frame.js");
  assert.match(js, /import \{ buildPreview \} from "\.\/panel_preview\.js";/);
  assert.match(js, /import \{ rangeKey, sameFrames, WAITING, waitMode \} from "\.\/panel_wait\.js";/);
  assert.match(js, /const skeleton = buildPreview\(panel\);/, "the preview follows the whole panel, not its type only");
  assert.match(js, /root\.dataset\.wait = mode;/);
  assert.match(js, /!sameFrames\(rendered, info\.items\)/);
  const draw = js.slice(js.indexOf("async function draw(info)"), js.indexOf("function framesSelect"));
  assert.ok(draw.indexOf("mod = await LOADERS[panel.type]()") < draw.indexOf("destroyInstance();\n    clear(content);"), "no await between the clear and the render");
  const after = draw.slice(draw.indexOf("destroyInstance();\n    clear(content);"));
  assert.equal((after.match(/\bawait\b/g) || []).length, 0, "no await after the old drawing goes");
});

test("the preview is drawn with the CSSOM and the SVG namespace, never with a style attribute", () => {
  const js = read("web/js/panel_preview.js");
  assert.match(js, /document\.createElementNS\(SVG_NS, n\.t\)/);
  assert.match(js, /e\.style\.setProperty\(name, value\)/);
  assert.ok(!/setAttribute\("style"|innerHTML|insertAdjacentHTML/.test(js));
  assert.match(js, /"aria-hidden": "true"/);
});

test("the preview and the dots take no colour literal, and the light that sweeps moves with a transform only", () => {
  const css = read("web/css/panel-loading.css");
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/.test(css));
  const keyframes = [...css.matchAll(/@keyframes ([\w-]+)/g)].map((m) => m[1]);
  assert.deepEqual(keyframes, ["panel-pulse", "panel-skel"]);
  // The light is one band: its keyframes move it with translateX and change nothing else (no size, no position, no repaint of the shapes).
  const start = css.indexOf("@keyframes panel-skel");
  const skel = css.slice(start, css.indexOf("\n", start));
  assert.match(skel, /translateX\(-100%\)/);
  assert.ok(!/width|height|left|top|opacity/.test(skel), skel);
  // Reduced motion stops the light and keeps the drawing still.
  assert.match(css, /@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\.panel__skeleton::after \{ animation: none; display: none; \}/);
});
