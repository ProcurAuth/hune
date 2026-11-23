// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the visual parity capture (15.14c): the canvas marks, the tolerances, the pairing of the copies of a repeat, the
// count of the queries of a copy, the rules of the documented differences and the rewrites of the dashboard for Grafana.
import assert from "node:assert/strict";
import test from "node:test";
import { stateQuery } from "../../tools/parity/capture-lib.mjs";
import { fileFacts, grafanaVersion, referencePanel, repeatOf, sectionFacts } from "../../tools/parity/visual.mjs";
import {
  TOLERANCE,
  colourDistance,
  colourToleranceOf,
  compareCopy,
  compareMarks,
  compareNames,
  compareTexts,
  grafanaCopyKey,
  grafanaCopyQueries,
  grafanaPlot,
  grafanaViewValue,
  huneCopyQueries,
  huneCopyTestid,
  linePlotOf,
  marksOf,
  pairBars,
  pairCopies,
  pairMarks,
  parseColour,
  parseGrafanaKey,
  parseHuneTestid,
  splitTexts,
  xScaleOf,
} from "../../tools/parity/visual-lib.mjs";
import { classifyDifference, offsetCuts } from "../../tools/parity/visual-rules.mjs";

const fill = (color, rects, clip = [10, 10, 200, 100]) => ({ k: "fill", color, alpha: 1, rects, clip });
const canvas = (ops) => ({ size: [220, 130], box: [0, 0, 220, 130], ops });

test("the colours: hex, rgb and rgba with the alpha, and a distance in one channel", () => {
  assert.deepEqual(parseColour("#fff5eb"), [255, 245, 235, 1]);
  assert.deepEqual(parseColour("rgba(26, 115, 213, 0.8)"), [26, 115, 213, 0.8]);
  assert.equal(parseColour("gradient"), null);
  assert.equal(colourDistance([255, 245, 235, 1], [253, 245, 235, 1]), 2);
  assert.equal(colourDistance([1, 2, 3, 0], [9, 9, 9, 0]), 0, "a transparent cell has no colour");
  assert.equal(colourDistance(null, [1, 1, 1, 1]), Infinity);
  assert.equal(colourToleranceOf("Viridis"), TOLERANCE.viridis, "8 for viridis");
  assert.equal(colourToleranceOf("Oranges"), TOLERANCE.colour, "2 for an exact scheme");
});

test("the marks of a canvas: only the rectangles that show in the plot, as drawn", () => {
  const c = canvas([
    fill("#ff0000", [[20, 20, 10, 10], [-30, 20, 50, 10], [-60, 20, 20, 10]]),
    { k: "text", text: "a", x: 1, y: 2 },
  ]);
  const m = marksOf(c);
  assert.deepEqual(m.plot, [10, 10, 200, 100]);
  assert.equal(m.marks.length, 2, "a rectangle that is wholly left of the plot is not a mark");
  assert.deepEqual(m.marks[1].box, [-30, 20, 50, 10], "the box is the rectangle as drawn, not the clipped one");
});

test("the pairing of the marks: the nearest centre inside the reach, each mark once", () => {
  const mk = (x, y) => ({ box: [x, y, 10, 10], colour: [0, 0, 0, 1] });
  const r = pairMarks([mk(0, 0), mk(20, 0), mk(40, 0)], [mk(1, 1), mk(21, 0), mk(80, 0)]);
  assert.equal(r.pairs.length, 2);
  assert.equal(r.onlyG.length, 1);
  assert.equal(r.onlyH.length, 1);
});

test("the cells compare with 1.5 px and the colour tolerance; a wider axis moves the plot of Hune into the plot of Grafana", () => {
  const g = canvas([fill("#fff5eb", [[10, 10, 50, 20], [60, 10, 50, 20]], [10, 10, 100, 40])]);
  const near = canvas([fill("#fff5eb", [[10.5, 10.5, 50, 20], [60.5, 10, 50, 20]], [10, 10, 100, 40])]);
  const ok = compareMarks(g, near);
  assert.equal(ok.paired, 2);
  assert.equal(ok.box_ok, true);
  const off = canvas([fill("#fff5eb", [[10, 10, 50, 20], [62, 10, 50, 20]], [10, 10, 100, 40])]);
  assert.equal(compareMarks(g, off).box_ok, false, "2 px is outside 1.5 px");
  // The plot of Hune starts 30 px to the right and is 30 px narrower: the same cells, in the map of the plot.
  const wide = canvas([fill("#fff5eb", [[40, 10, 35, 20], [75, 10, 35, 20]], [40, 10, 70, 40])]);
  const mapped = compareMarks(g, wide);
  assert.equal(mapped.paired, 2);
  assert.equal(mapped.plot_shift, -30);
  assert.equal(mapped.box_ok, true);
  const colour = canvas([fill("#f0f0f0", [[10, 10, 50, 20], [60, 10, 50, 20]], [10, 10, 100, 40])]);
  assert.equal(compareMarks(g, colour).colour_bad_count, 2);
  assert.equal(compareMarks(g, colour, { colourTolerance: 255 }).colour_bad_count, 0);
});

test("the bars pair by x, with no regard to the height; the bars of a small height that only Hune draws are empty buckets", () => {
  const bar = (x, h) => ({ box: [x, 100 - h, 20, h], colour: [1, 2, 3, 0.8], raw: "c" });
  const gp = { plot: [0, 0, 300, 100], marks: [bar(0, 80), bar(20, 40), bar(40, 20)] };
  const hp = { plot: [0, 0, 300, 100], marks: [bar(0, 60), bar(20, 30), bar(40, 15), bar(60, 1)] };
  const r = pairBars(gp, hp);
  assert.equal(r.pairs.length, 3);
  assert.equal(r.onlyH.length, 1);
  const g = canvas([{ k: "fill", color: "#111", alpha: 1, rects: [[0, 20, 20, 80], [20, 60, 20, 40], [40, 80, 20, 20]], clip: [0, 0, 300, 100] }]);
  const h = canvas([{ k: "fill", color: "#111", alpha: 1, rects: [[0, 40, 20, 60], [20, 70, 20, 30], [40, 85, 20, 15], [60, 96, 20, 3]], clip: [0, 0, 300, 100] }]);
  const m = compareMarks(g, h, { mode: "bars" });
  assert.equal(m.slivers, 1);
  assert.equal(m.only_hune_count, 0);
  assert.equal(m.box_ok, true, "the heights compare as a share of the tallest bar");
});

test("the texts of the axes split below and above the plot, and compare as bags", () => {
  const c = canvas([
    fill("#000", [[10, 10, 5, 5]]),
    { k: "text", text: "5", x: 1, y: 30 },
    { k: "text", text: "00:00", x: 10, y: 118 },
    { k: "text", text: "", x: 10, y: 118 },
  ]);
  const t = splitTexts(c);
  assert.deepEqual(t.x.map((x) => x.text), ["00:00"]);
  assert.deepEqual(t.y.map((x) => x.text), ["5"]);
  assert.equal(compareTexts([{ text: "a" }, { text: "b" }], [{ text: "b" }, { text: "a" }]).equal, true);
  assert.deepEqual(compareTexts([{ text: "a" }], [{ text: "b" }]), { equal: false, only_grafana: ["a"], only_hune: ["b"] });
});

test("the keys of a copy: Grafana panel-<id>-clone-<n - 1> is the copy n, and Hune panel-<id>-r<n> is the copy n", () => {
  assert.deepEqual(parseGrafanaKey("panel-7"), { id: 7, n: 1 });
  assert.deepEqual(parseGrafanaKey("panel-7-clone-1"), { id: 7, n: 2 });
  assert.deepEqual(parseGrafanaKey("panel-7-clone-4"), { id: 7, n: 5 });
  assert.equal(parseGrafanaKey("panel-x"), null);
  assert.deepEqual(parseHuneTestid("panel-7"), { id: 7, n: 1 });
  assert.deepEqual(parseHuneTestid("panel-7-r2"), { id: 7, n: 2 });
  assert.equal(parseHuneTestid("panel-7-r1"), null, "the first copy is the number");
  assert.equal(parseHuneTestid("panel-7-menu"), null);
  for (const n of [1, 2, 3, 5]) {
    assert.deepEqual(parseGrafanaKey(grafanaCopyKey(7, n)), parseHuneTestid(huneCopyTestid(7, n)), `copy ${n}`);
  }
});

test("the pairing of the copies of a repeat by (id, copy), also for the panels of a repeated section that share a key in Grafana", () => {
  const g = [
    { key: "panel-1", box: [0, 0, 10, 10], title: "H a" },
    { key: "panel-1-clone-1", box: [10, 0, 10, 10], title: "H b" },
    { key: "panel-1-clone-2", box: [20, 0, 10, 10], title: "H c" },
    { key: "panel-9", box: [0, 20, 30, 10], title: "After" },
  ];
  const h = [
    { key: "panel-1", box: [0, 0, 10, 10], title: "H a" },
    { key: "panel-1-r2", box: [10, 0, 10, 10], title: "H b" },
    { key: "panel-1-r3", box: [20, 0, 10, 10], title: "H c" },
    { key: "panel-9", box: [0, 20, 30, 10], title: "After" },
  ];
  const r = pairCopies(g, h);
  assert.deepEqual(r.pairs.map((p) => `${p.id}:${p.n}`), ["1:1", "1:2", "1:3", "9:1"]);
  assert.equal(r.onlyG.length + r.onlyH.length, 0);
  // A repeated section: the panels of each copy of the section keep the key `panel-2` in Grafana. The n-th one is the copy n.
  const gs = [{ key: "panel-2", box: [0, 0, 1, 1] }, { key: "panel-3", box: [0, 0, 1, 1] }, { key: "panel-2", box: [0, 9, 1, 1] }, { key: "panel-3", box: [0, 9, 1, 1] }];
  const hs = [{ key: "panel-2", box: [0, 0, 1, 1] }, { key: "panel-3", box: [0, 0, 1, 1] }, { key: "panel-2-r2", box: [0, 9, 1, 1] }, { key: "panel-3-r2", box: [0, 9, 1, 1] }];
  assert.deepEqual(pairCopies(gs, hs).pairs.map((p) => `${p.id}:${p.n}`), ["2:1", "2:2", "3:1", "3:2"]);
  // A copy that one side lacks is listed.
  const lost = pairCopies(g, h.slice(0, 2));
  assert.equal(lost.onlyG.length, 2);
});

test("the comparison of a copy: the box with the origin of the first panel, the title and the number of queries", () => {
  const pair = { id: 1, n: 2, grafana: { key: "panel-1-clone-1", box: [610, 113, 602, 220], title: "Host b", queries: 1 }, hune: { key: "panel-1-r2", box: [614.5, 100, 602, 220], title: "Host b", queries: 1 } };
  const ok = compareCopy(pair, { origin: { grafana: [10, 113], hune: [14.5, 100] } });
  assert.equal(ok.box_ok, true);
  assert.equal(ok.title.equal, true);
  assert.equal(ok.queries.equal, true);
  const bad = compareCopy({ ...pair, hune: { ...pair.hune, title: "Host c", queries: 2 } }, { origin: { grafana: [10, 113], hune: [14.5, 100] } });
  assert.equal(bad.title.equal, false);
  assert.equal(bad.queries.equal, false);
  assert.equal(compareCopy(pair, { origin: { grafana: [10, 113], hune: [10, 100] } }).box_ok, false, "4.5 px apart");
});

test("the queries of a copy: Grafana groups the requests by panel id and Hune by the value of the variable", () => {
  const req = (id, title, n) => ({ headers: { "x-panel-id": id, "x-panel-title": title }, body: JSON.stringify({ queries: Array.from({ length: n }, (_, i) => ({ refId: "ABC"[i] })) }) });
  const requests = [req("1", "Host ${host}", 2), req("901", "Host ${host}", 2), req("902", "Host ${host}", 2), req("2", "After", 1)];
  assert.deepEqual(grafanaCopyQueries(requests, { title: "Host ${host}" }), [2, 2, 2]);
  assert.deepEqual(grafanaCopyQueries(requests, { id: 2, plain: true }), [1]);
  const hr = (id, values, all = false) => ({ url: `http://h/api/dashboards/d/panels/${id}/query`, body: JSON.stringify({ variables: { host: { all, values } } }) });
  const hune = [hr(1, ["a"]), hr(1, ["a"]), hr(1, ["b"]), hr(1, ["c"]), hr(2, [], true)];
  assert.equal(huneCopyQueries(hune, { id: 1, variable: "host", value: "a" }), 2);
  assert.equal(huneCopyQueries(hune, { id: 1, variable: "host", value: "c" }), 1);
  assert.equal(huneCopyQueries(hune, { id: 2 }), 1);
});

test("the view URL of Grafana for a copy carries the VALUE of the variable, not an index (measured on Grafana 12.4.2)", () => {
  assert.equal(grafanaViewValue("b", 2), "b$panel-2");
  assert.equal(grafanaViewValue("20", 1), "20$panel-1");
});

test("the names of the series compare in order and as a set", () => {
  assert.equal(compareNames(["A a", "B b"], ["A a", "B b"]).equal, true);
  const r = compareNames(["A a", "B b"], ["B b", "A a"]);
  assert.equal(r.equal, false);
  assert.equal(r.same_set, true);
});

test("the rules: a documented difference gets its section, any other difference is a fault", () => {
  const base = { check: "texts:y-axis", type: "heatmap", detail: { only_grafana: ["1.0 ms", "2.0 ms"], only_hune: ["1", "2"] }, siblings: [] };
  assert.deepEqual(classifyDifference(base), { class: "documented", rule: "19.20g:unit-of-names" });
  assert.equal(classifyDifference({ ...base, detail: { only_grafana: ["1.0 ms"], only_hune: ["3"] } }).class, "fault");
  const log = { check: "texts:y-axis", type: "heatmap", detail: { only_grafana: ["0.00781", "0.100"], only_hune: ["0.0078125", "0.1"] }, siblings: [] };
  assert.equal(classifyDifference(log).rule, "19.20g:log-label-form");
  const note = { check: "plot", type: "heatmap", detail: {}, lines: { hune: ["1 value of 0 or less is not on the log scale."] }, siblings: [] };
  assert.equal(classifyDifference(note).rule, "19.20g:log-note");
  assert.equal(classifyDifference({ ...note, lines: { hune: [] } }).class, "fault");
  const names = { check: "names:legend", type: "timeseries", detail: { grafana: ["a"], hune: ["A a"] } };
  assert.equal(classifyDifference(names).rule, "19.20f:name-prefix-on-failure");
  assert.equal(classifyDifference({ ...names, detail: { grafana: ["a"], hune: ["B c"] } }).class, "fault");
  const axis = { check: "texts:y-axis", type: "histogram", detail: { only_grafana: ["2500"], only_hune: [] }, custom: {}, siblings: [] };
  assert.equal(classifyDifference(axis).class, "fault", "the ticks of the y axis of a histogram are not in 19.20g");
  const colours = { check: "colours", type: "histogram", detail: { examples: [{ grafana: "rgba(86, 166, 75, 0.8)", hune: "rgba(26, 115, 213, 0.8)" }] } };
  assert.equal(classifyDifference(colours).rule, "19.20g:palette");
  assert.equal(classifyDifference({ ...colours, detail: { examples: [{ grafana: "rgba(86, 166, 75, 0.8)", hune: "rgba(26, 115, 213, 0.3)" }] } }).class, "fault", "a different alpha is not the palette");
});

test("the dashboard that Grafana gets: the uid, the data source map and the quoted placeholders; the file stays as it is", () => {
  const dashboard = {
    uid: "x_y",
    panels: [{ id: 1, type: "stat", datasource: { type: "grafana-clickhouse-datasource", uid: "conf-ds" }, targets: [{ refId: "A", rawSql: "SELECT 1 WHERE 'a' IN (${host}) AND b = ${host:csv}" }] }],
  };
  const o = { dashboard, dsMap: ["conf-ds", "clickhouse"], quotePlaceholders: true };
  const g = grafanaVersion(o);
  assert.equal(g.uid, "hpar-x_y");
  assert.equal(g.panels[0].datasource.uid, "clickhouse");
  assert.equal(g.panels[0].targets[0].rawSql, "SELECT 1 WHERE 'a' IN (${host:singlequote}) AND b = ${host:csv}");
  assert.equal(dashboard.uid, "x_y");
  assert.equal(dashboard.panels[0].datasource.uid, "conf-ds");
  const plain = grafanaVersion({ dashboard, dsMap: undefined, quotePlaceholders: false });
  assert.equal(plain.panels[0].targets[0].rawSql, dashboard.panels[0].targets[0].rawSql);
});

test("the reference panel for the width of the window and the repeat of a dashboard", () => {
  const dashboard = {
    panels: [
      { id: 1, gridPos: { w: 24 }, repeat: "host" },
      { id: 2, gridPos: { w: 24 } },
      { id: 3, gridPos: { w: 12 } },
    ],
    templating: { list: [{ name: "host", options: [{ value: "$__all" }, { value: "a" }, { value: "b" }] }] },
  };
  assert.deepEqual(referencePanel(dashboard), { id: 2, w: 24 }, "a copy has the width of the repeat, so a repeated panel is not the reference");
  assert.deepEqual(repeatOf(dashboard), { name: "host", values: ["a", "b"] });
});

test("the time of the Grafana URL is in ISO form (Grafana 12.4.2 ignores milliseconds on a dashboard URL)", () => {
  assert.equal(stateQuery(1794528000000, 1794546000000, [], true), "from=2026-11-13T00:00:00.000Z&to=2026-11-13T05:00:00.000Z");
  assert.equal(stateQuery(1, 2, []), "from=1&to=2");
});

test("the plot of Grafana is its clip less half the widest line in it: a line of 3 px makes the clip 1.5 px larger on each side (histogram_f panel 7)", () => {
  // The capture of 2026-11-15: Grafana clips the bars to [38.5, 6.5, 1759, 180] with a line of 3 px; Hune has the clip [40, 7, 1756, 180] (1 px above and below).
  const bars = [[40, 100, 50, 84], [91, 60, 50, 124]];
  const g = canvas([fill("rgba(1, 2, 3, 0.3)", bars, [38.5, 6.5, 1759, 180]), { k: "stroke", color: "rgba(1, 2, 3, 1)", width: 3, rects: bars, clip: [38.5, 6.5, 1759, 180] }]);
  assert.deepEqual(grafanaPlot(g, [38.5, 6.5, 1759, 180]), [40, 8, 1756, 177]);
  assert.deepEqual(marksOf(g, { grafana: true }).plot, [40, 8, 1756, 177]);
  assert.deepEqual(marksOf(g).plot, [38.5, 6.5, 1759, 180], "without the flag the plot is the clip (Hune)");
  const h = canvas([fill("rgba(1, 2, 3, 0.3)", bars, [40, 7, 1756, 180])]);
  const m = compareMarks(g, h, { mode: "bars" });
  assert.equal(m.plot_shift, 0);
  assert.equal(m.plot_width_delta, 0);
  assert.equal(m.plot_top_delta, 1);
  // A clip with no line is the plot.
  assert.deepEqual(grafanaPlot(canvas([fill("#000", bars, [10, 10, 100, 100])]), [10, 10, 100, 100]), [10, 10, 100, 100]);
});

test("the plot of a time series comes from the clip of its lines; a time series whose y axes are all hidden is known from the file", () => {
  const line = (clip, width = 1) => ({ k: "stroke", color: "#f00", width, rects: [], clip });
  // Grafana 12.4.2, all y axes hidden: the clip [-0.5, -0.5, 1805, 207] with a line of 1 px is the plot [0, 0, 1804, 206].
  assert.deepEqual(linePlotOf(canvas([line([-0.5, -0.5, 1805, 207]), line(null)]), { grafana: true }), [0, 0, 1804, 206]);
  assert.deepEqual(linePlotOf(canvas([line([0, -1, 1804, 187])])), [0, -1, 1804, 187]);
  assert.equal(linePlotOf(canvas([line(null)])), null);
  const ts = (defaults, overrides = []) => fileFacts({ panels: [{ id: 1, type: "timeseries", fieldConfig: { defaults: { custom: defaults }, overrides } }] }).panels.get(1).hiddenAxes;
  assert.equal(ts({ axisPlacement: "hidden" }), true);
  assert.equal(ts({}), false);
  assert.equal(ts({ axisPlacement: "hidden" }, [{ matcher: { id: "byName", options: "b" }, properties: [{ id: "custom.axisPlacement", value: "right" }] }]), false, "an override shows an axis");
});

test("the scale of a number axis from its labels: the medians of next labels, so a moved last label does not count", () => {
  const labels = [-15, 10, 35, 60, 85].map((v) => ({ text: String(v), x: 100 + 4 * v }));
  labels.push({ text: "110", x: 520 }); // the last label moved 20 px to the left (it stays in the canvas)
  assert.deepEqual(xScaleOf(labels), { a: 100, k: 4 });
  assert.equal(xScaleOf([{ text: "a", x: 1 }, { text: "5", x: 2 }]), null);
  assert.equal(offsetCuts({ bucketSize: 25, bucketOffset: 10 }), true);
  assert.equal(offsetCuts({ bucketSize: 25, bucketOffset: 50 }), false);
  assert.equal(offsetCuts({ bucketOffset: 10 }), false, "no size in the file: no rule");
});

test("the bars of a histogram with a bucket offset pair by their x value; Grafana and Hune start the x axis at other places", () => {
  // Buckets of 25 from -15. Grafana: the axis starts at 0 with 4 px a unit; Hune: the axis starts at -15 with 3.5 px a unit, and a bar is its slot less 1 px.
  const g = canvas([fill("#111", [[-60, 50, 100, 50], [40, 20, 100, 80], [140, 60, 100, 40]], [0, 0, 300, 100])]);
  const h = canvas([fill("#111", [[0.5, 50, 86.5, 50], [88, 20, 86.5, 80], [175.5, 60, 86.5, 40]], [0, 0, 300, 100])]);
  const byValue = { g: { a: 0, k: 4 }, h: { a: 52.5, k: 3.5 } };
  const m = compareMarks(g, h, { mode: "bars", byValue });
  assert.equal(m.paired, 3);
  assert.equal(m.box_ok, true);
  assert.equal(compareMarks(g, h, { mode: "bars" }).paired < 3, true, "without the scales the bars do not pair");
});

test("the rule of the header of a copy of a repeated section: 2.75 px for each copy 2 to n above the box, within 0.5 px (19.20b)", () => {
  const dashboard = {
    panels: [
      { id: 1, type: "stat" },
      { id: 5, type: "row", repeat: "host" },
      { id: 2, type: "stat" },
      { id: 6, type: "row" },
      { id: 4, type: "stat" },
    ],
    templating: { list: [{ name: "host", options: [{ value: "$__all" }, ...["a", "b", "c", "d", "e"].map((value) => ({ value }))] }] },
  };
  const sec = sectionFacts(dashboard);
  assert.deepEqual([[...sec.inSection], [...sec.below], sec.copies], [[2], [4], 5]);
  const box = (n, dy, extra = {}) => ({ check: "box", type: "stat", n, detail: { delta: [0, Math.abs(dy), 0, 0], shift: [0, dy] }, section: { inSection: true, below: false, copies: 5 }, ...extra });
  assert.equal(classifyDifference(box(2, 2.75)).rule, "19.20b:copy-header");
  assert.equal(classifyDifference(box(5, 11.2)).rule, "19.20b:copy-header");
  assert.equal(classifyDifference(box(3, 2.75)).class, "fault", "the copy 3 has two short headers above it");
  assert.equal(classifyDifference(box(2, -2.75)).class, "fault", "a box of Hune that is higher is a fault");
  assert.equal(classifyDifference(box(2, 3.5)).class, "fault");
  assert.equal(classifyDifference({ ...box(2, 2.75), detail: { delta: [2, 2.75, 0, 0], shift: [2, 2.75] } }).class, "fault", "a move in x is a fault");
  assert.equal(classifyDifference(box(1, 11, { section: { inSection: false, below: true, copies: 5 } })).rule, "19.20b:copy-header", "a panel below the section");
  assert.equal(classifyDifference(box(1, 11, { section: { inSection: false, below: false, copies: 5 } })).class, "fault", "a panel above the section");
  assert.equal(classifyDifference({ ...box(2, 2.75), section: undefined }).class, "fault", "no repeated section");
});

test("the rules of the histogram: one bucket keys on the rectangle of Grafana with no x, and the offset on the options (19.20g)", () => {
  const marks = { check: "marks", type: "histogram", detail: { only_grafana: [{ box: [null, 96.5, 1756, 87.5] }], only_hune: [{ box: [40.5, 97.5, 1755, 88] }] }, bars: { hune: 1, grafana_no_x: 1 } };
  assert.equal(classifyDifference(marks).rule, "19.20g:one-bucket");
  assert.equal(classifyDifference({ ...marks, bars: { hune: 1, grafana_no_x: 0 } }).class, "fault");
  const x = { check: "texts:x-axis", type: "histogram", detail: { only_grafana: [], only_hune: ["0", "400"] }, bars: { hune: 1, grafana_no_x: 1 } };
  assert.equal(classifyDifference(x).rule, "19.20g:one-bucket");
  assert.equal(classifyDifference({ ...x, detail: { only_grafana: ["5"], only_hune: ["0", "400"] } }).class, "fault", "a label that only Grafana draws");
  assert.equal(classifyDifference({ ...x, check: "texts:y-axis" }).class, "fault", "the y axis follows the rule of 9.19f");
  const off = { check: "texts:x-axis", type: "histogram", options: { bucketSize: 25, bucketOffset: 10 }, detail: { only_grafana: ["0"], only_hune: ["-15"] } };
  assert.equal(classifyDifference(off).rule, "19.20g:bucket-offset");
  assert.equal(classifyDifference({ ...off, options: { bucketSize: 25, bucketOffset: 0 } }).class, "fault");
  assert.equal(classifyDifference({ ...off, check: "texts:y-axis" }).class, "fault");
});
