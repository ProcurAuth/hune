// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B330 to B339 (9.11): the loading preview of the charts (the time series, the xy chart and the bar chart).
// Each dashboard has option variants (lib/preview_charts_fixtures.mjs). The harness (lib/preview_measure.mjs) holds the queries so that each panel keeps
// its preview, reads the preview, releases the queries, and reads the loaded panel. The tolerance classes decide: an exact part is the same within
// 0.5 px, a `contain` part holds the loaded part on the edges that the rule claims, a least part is not larger than the loaded part.
// A dashboard is measured once for all the cases that read it. HUNE_PREVIEW_DUMP=<dir> writes the table of each dashboard to <dir>/<uid>.txt and the
// captures of the loading state (the light and the dark theme) to <dir>.
import { mkdirSync, writeFileSync } from "node:fs";
import { formatTable, measureDashboard } from "../lib/preview_measure.mjs";
import { barVariantsDoc, timeSeriesVariantsDoc } from "../lib/preview_charts_fixtures.mjs";
import { sleep } from "../lib/h.mjs";

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const DUMP = process.env.HUNE_PREVIEW_DUMP || "";
const HOLD = (req) => /\/panels\/[^/]+\/query/.test(req.url);
const HEIGHT = 6000;

// The panel ids of the fixtures: the order of TS_VARIANTS, BAR_VARIANTS (the list of the xy charts follows).
const TS = { list: 1, table: 2, tableEmpty: 3, listRight: 4, tableRight: 5, hiddenBottom: 6, hiddenRight: 7, off: 8, axesHidden: 9, axisRight: 10, axisWidth: 11, fixed: 12, soft: 13, undecided: 14, undecidedHide: 15, queryOverride: 16, shortList: 17, shortTable: 18, narrowTable: 19, narrowList: 20, bars: 21, fixedOff: 22, fixedTitle: 23, fixedBytes: 24, styles: 25, points: 26, barsOff: 27, hiddenViz: 28 };
const BAR = { list: 1, table: 2, tableEmpty: 3, listRight: 4, tableRight: 5, off: 6, hidden: 7, axis: 8, title: 9, turned: 10, hBottom: 11, hRight: 12, hOff: 13, hBand: 14, hPercent: 15, autoTall: 16, autoWide: 17, short: 18, hTable: 19, hHidden: 20, hFallbackList: 21, hFallbackTable: 22, hBetween: 23, hShort: 24, hTableEmpty: 25, xyList: 26, xyTable: 27, xyRight: 28, xyOff: 29 };

// The host of the preview and the host of the loaded panel of each panel. After the load, the preview is hidden and its markup stays. A horizontal
// bar chart has the attributes on its element `.hbar`. `marks` counts the decorative marks of the preview.
const hostInfo = (page, ids) => page.eval(`(() => {
  const out = {};
  for (const id of ${JSON.stringify(ids)}) {
    const p = document.querySelector('[data-testid="panel-' + id + '"]');
    const sk = p.querySelector(".panel__skeleton .hbar") || p.querySelector(".panel__skeleton .skel__host");
    const lo = p.querySelector(".panel__plot .hbar") || p.querySelector(".panel__plot");
    const at = (e) => (e ? { placement: e.getAttribute("data-legend-placement"), fallback: e.hasAttribute("data-legend-fallback") } : null);
    const n = (sel) => p.querySelectorAll(".panel__skeleton " + sel).length;
    const first = (sel) => { const e = p.querySelector(".panel__skeleton " + sel); return e ? getComputedStyle(e) : null; };
    const series = first(".skel__series");
    const dot = first(".skel__dot");
    out[id] = {
      skeleton: at(sk), loaded: at(lo), hbar: !!p.querySelector(".panel__skeleton .hbar"), loadedHbar: !!p.querySelector(".panel__plot .hbar"), legends: p.querySelectorAll(".panel__skeleton .chartCore__legend, .panel__skeleton .hbar__legend").length,
      marks: { bars: n("rect.skel__bar"), series: n(".skel__series"), area: n(".skel__area, .skel__fade"), dots: n(".skel__dot"), stops: n("stop") },
      width: series ? series.strokeWidth : null, dash: series ? series.strokeDasharray : null, dot: dot ? dot.width : null,
    };
  }
  return out;
})()`);

const memo = new Map();
// Measures a dashboard once for all the cases that read it. The result is plain data, so it outlives the page of the case.
async function measured(t, doc) {
  if (memo.has(doc.uid)) return memo.get(doc.uid);
  const r = await t.api.create("transit", doc);
  t.assert(r.status === 201, `create ${doc.uid}: ${r.status} ${r.text.slice(0, 400)}`);
  const page = await t.page({ width: 1440, height: HEIGHT });
  const result = await measureDashboard(page, t.dash(doc.uid), { width: 1440, height: HEIGHT });
  const hosts = await hostInfo(page, doc.sections.flatMap((s) => s.panels.map((p) => p.id)));
  if (DUMP) {
    mkdirSync(DUMP, { recursive: true });
    writeFileSync(`${DUMP}/${doc.uid}.txt`, `${formatTable(result.rows, { all: true })}\n`);
    writeFileSync(`${DUMP}/${doc.uid}.json`, `${JSON.stringify(result, null, 2)}\n`);
  }
  const out = { result, hosts };
  memo.set(doc.uid, out);
  return out;
}

const tsDoc = () => timeSeriesVariantsDoc("pv330");
const barDoc = () => barVariantsDoc("pv335");

const rowsOf = (result, id) => result.rows.filter((r) => String(r.id) === String(id));
const rowOf = (result, id, part, index = 0) => rowsOf(result, id).find((r) => r.part === part && r.index === index && r.verdict !== "unclaimed");
const claimed = (result, id) => rowsOf(result, id).filter((r) => r.verdict !== "unclaimed");

// Every claimed part of the panels is ok, and each panel loaded with data.
function allOk(t, result, ids, what) {
  for (const id of ids) {
    const loaded = result.loaded[id];
    t.assert(loaded && loaded.state === "ready", `${what} ${id} loaded (${loaded && loaded.state})`);
    t.assert(result.previews[id] && result.previews[id].shown, `${what} ${id}: the preview shows`);
    const rows = claimed(result, id);
    t.assert(rows.length > 0, `${what} ${id}: the preview claims a part`);
    t.eq(rows.filter((r) => r.verdict !== "ok").map((r) => `${r.part}#${r.index} ${r.verdict} ${JSON.stringify(r.preview)} ${JSON.stringify(r.loaded)}`), [], `${what} ${id}: no claimed part differs`);
  }
}

export const cases = [
  {
    id: "B330",
    skip: SKIP,
    timeout: 400000,
    name: "time series: the legend as a list or a table, at the bottom or at the right, hidden or off. The preview takes the least legend and the plot holds the loaded plot (9.11)",
    async run(t) {
      const { result } = await measured(t, tsDoc());
      allOk(t, result, [TS.list, TS.table, TS.tableEmpty, TS.listRight, TS.hiddenBottom, TS.hiddenRight, TS.off], "panel");
      // A list at the bottom: the loaded legend is one line here, as the least legend (31 px: 30.8). The class is least.
      const list = rowOf(result, TS.list, "legend");
      t.eq(list.cls, "least", "a list at the bottom: the line count is data");
      t.assert(Math.abs(list.preview[3] - 30.857) < 0.1 && list.loaded[3] >= list.preview[3] - 0.5, `the least list legend is one line: ${list.preview[3]} and ${list.loaded[3]}`);
      // A table at the bottom (values last_not_null and max, sorted by max): the head row has the height and the first and the last text of the loaded one.
      const head = rowOf(result, TS.table, "head");
      t.assert(head && head.verdict === "ok" && Math.abs(head.preview[3] - head.loaded[3]) <= 0.5 && Math.abs(head.preview[2] - head.loaded[2]) <= 0.5, `the head row: ${JSON.stringify(head)}`);
      const labels = claimed(result, TS.table).filter((r) => r.part === "head-label");
      t.eq(labels.map((r) => r.text), ["Name", "Max"], "the first and the last head text");
      for (const l of labels) t.assert(Math.abs(l.preview[2] - l.loaded[2]) <= 0.5 && Math.abs(l.preview[0] - l.loaded[0]) <= 0.5, `the head text ${l.text} has the width and the place of the loaded text: ${JSON.stringify(l)}`);
      // No value in the document: the table has the column "Name" only (4.26), so the first and the last head text are the same text.
      const empty = claimed(result, TS.tableEmpty).find((r) => r.part === "head");
      t.assert(empty && empty.verdict === "ok", "the head row of a table with no value");
      t.eq(rowsOf(result, TS.tableEmpty).filter((r) => r.part === "head-label").map((r) => r.text), ["Name"], "one head text");
      // Every entry hidden: no legend, and no gap at the right either: the plot has the whole width (9.19).
      for (const id of [TS.hiddenBottom, TS.hiddenRight, TS.off]) t.eq(claimed(result, id).filter((r) => r.part === "legend").length, 0, `panel ${id}: no legend part`);
      const hr = rowOf(result, TS.hiddenRight, "plot");
      t.assert(Math.abs(hr.preview[0] + hr.preview[2] - (hr.loaded[0] + hr.loaded[2])) <= 0.5, `the right edge with every entry hidden at the right: ${JSON.stringify(hr)}`);
      t.eq(hr.loaded[0] + hr.loaded[2], 682 - 8, "the plot ends 8 px before the right edge: the gap is gone");
      // A list at the right in 682 px never falls back: its legend is bounded, its plot claims all four edges.
      t.eq(rowOf(result, TS.listRight, "plot").cls, "contain:ltrb", "a list at the right");
    },
  },
  {
    id: "B331",
    skip: SKIP,
    timeout: 400000,
    name: "time series: the margins and the gutters. The document fixes them in some cases (exact, within 0.5 px) and bounds them in the others (the plot holds the loaded plot) (9.11)",
    async run(t) {
      const { result } = await measured(t, tsDoc());
      allOk(t, result, [TS.axesHidden, TS.axisRight, TS.axisWidth, TS.fixed, TS.soft, TS.queryOverride, TS.bars, TS.fixedOff, TS.fixedTitle, TS.fixedBytes, TS.barsOff], "panel");
      // The document fixes the plot: all axes hidden (top 0, gutters 8), axis_width 50 with a title (70), an override by query of axis_width 44.
      for (const id of [TS.axesHidden, TS.axisWidth, TS.queryOverride]) {
        const r = rowOf(result, id, "plot");
        t.eq(r.cls, "exact", `panel ${id}: the plot is exact`);
        t.assert(r.delta.every((d) => Math.abs(d) <= 0.5), `panel ${id}: the plot is the same within 0.5 px ${JSON.stringify(r.delta)}`);
      }
      const hidden = rowOf(result, TS.axesHidden, "plot");
      t.eq([hidden.loaded[0], hidden.loaded[1]], [8, 0], "all axes hidden: the plot starts at 8 px and at the top edge");
      // A fixed range and soft ends make the tick 0 certain: the least gutter holds the label 0, and the loaded gutter is not less.
      for (const id of [TS.fixed, TS.soft]) {
        const r = rowOf(result, id, "plot");
        t.assert(r.preview[0] > 9 && r.preview[0] <= r.loaded[0] + 0.5, `panel ${id}: the least gutter ${r.preview[0]} holds the label 0 and is at most ${r.loaded[0]}`);
      }
      // An axis at the right with a title: the left gutter is 8 and the right gutter has the least value.
      const right = rowOf(result, TS.axisRight, "plot");
      t.eq(right.loaded[0], 8, "an axis at the right: the left gutter is 8");
      t.assert(right.preview[0] + right.preview[2] >= right.loaded[0] + right.loaded[2] - 0.5, "the least right gutter");
      // The title room of the plot with an exact width: the same plot as the defaults.
      t.eq(rowOf(result, TS.bars, "plot").cls, "contain:ltrb", "draw bars: the same frame");
      // A fixed range and a legend of a list: the plot height is data, but the gutter is the same for every height of the range (R9), so the left edge is exact.
      const fixed = rowOf(result, TS.fixed, "plot");
      t.assert(Math.abs(fixed.preview[0] - fixed.loaded[0]) <= 0.5, `the left edge of a fixed range: ${fixed.preview[0]} and ${fixed.loaded[0]}`);
      t.eq(claimed(result, TS.fixed).filter((r) => r.part === "label-y").length, 0, "no y label: the height of the plot is data");
      // The range of the request gives the x labels (the harness holds the request: `rt.rangeOf`). All axes hidden, legend off: every x label is exact.
      const hiddenXs = claimed(result, TS.axesHidden).filter((r) => r.part === "label-x");
      t.assert(hiddenXs.length >= 3 && hiddenXs.every((r) => r.cls === "exact" && r.verdict === "ok" && /^\d\d:\d\d$/.test(r.text)), `the x labels of the range: ${JSON.stringify(hiddenXs.map((r) => [r.text, r.delta]))}`);
      t.eq(claimed(result, TS.axisWidth).filter((r) => r.part === "label-x").length >= 3, true, "axis_width: the gutter is exact, so the x labels are too");
      // A fixed range and a legend that is off: the height of the plot is exact, so every y label is exact and so is the plot.
      const fixedOff = rowOf(result, TS.fixedOff, "plot");
      t.eq(fixedOff.cls, "exact", "a fixed range, legend off: the plot is exact");
      const ys = claimed(result, TS.fixedOff).filter((r) => r.part === "label-y");
      t.eq(ys.map((r) => r.text), ["0%", "20%", "40%", "60%", "80%", "100%"], "the labels of the loaded axis, in the order of the preview");
      t.assert(ys.every((r) => r.cls === "exact" && r.verdict === "ok"), "each y label is the same within 0.5 px");
      t.assert(claimed(result, TS.fixedOff).filter((r) => r.part === "label-x").length >= 3, "and the x labels");
      // With a title the gutter is exact and the labels are not drawn (the title is a text of the canvas too).
      const titled = rowOf(result, TS.fixedTitle, "plot");
      t.assert(titled.cls === "exact" && titled.delta.every((d) => Math.abs(d) <= 0.5), `a fixed range with a title: ${JSON.stringify(titled)}`);
      t.eq(claimed(result, TS.fixedTitle).filter((r) => r.part === "label-y").length, 0, "no y label with a title");
      // The unit bytes gives binary steps, and a border moves the labels 4 px out.
      const bytes = claimed(result, TS.fixedBytes).filter((r) => r.part === "label-y");
      t.assert(bytes.length >= 3 && bytes.every((r) => r.verdict === "ok"), `the labels of bytes with a border: ${JSON.stringify(bytes.map((r) => [r.text, r.delta]))}`);
      // Bars keep the span of the data (9.19): the x axis has no label from the range.
      t.eq(rowOf(result, TS.barsOff, "plot").cls, "exact", "bars, all axes hidden: the plot is exact");
      t.eq(claimed(result, TS.barsOff).filter((r) => r.part === "label-x").length, 0, "no x label for bars");
    },
  },
  {
    id: "B332",
    skip: SKIP,
    timeout: 400000,
    name: "time series: an override that the document does not decide leaves the least form. The legend may be hidden by an override, so the preview draws none (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, tsDoc());
      allOk(t, result, [TS.undecided, TS.undecidedHide, TS.hiddenViz], "panel");
      // Overrides by name set the placement, the unit and the width: no equality, the plot holds the loaded plot, and the top is not claimed.
      const u = rowOf(result, TS.undecided, "plot");
      t.eq(u.cls, "contain:lrb", "overrides of the placement: the top margin is not claimed");
      t.assert(u.preview[0] <= u.loaded[0] + 0.5, `the least gutter ${u.preview[0]} is at most ${u.loaded[0]}`);
      // An override can hide some entries: the preview has no legend and takes the whole height.
      t.eq(claimed(result, TS.undecidedHide).filter((r) => r.part === "legend").length, 0, "no legend part");
      t.eq(hosts[TS.undecidedHide].legends, 0, "no legend element in the preview");
      // Every series hidden from the plot: no mark in the preview.
      const hv = hosts[TS.hiddenViz].marks;
      t.eq([hv.bars, hv.series, hv.area, hv.dots], [0, 0, 0, 0], "no mark with hide_from.viz");
      t.eq(hosts[TS.list].marks.series, 1, "and one line for a panel that shows its series");
      const p = rowOf(result, TS.undecidedHide, "plot");
      t.assert(p.preview[3] >= p.loaded[3] - 0.5 && p.preview[3] > p.loaded[3], `the plot of the preview is the larger one: ${p.preview[3]} and ${p.loaded[3]}`);
    },
  },
  {
    id: "B333",
    skip: SKIP,
    timeout: 400000,
    name: "time series: the cap of the legend (35 % of the height) fixes its height in a short panel, and a right legend falls back under the plot where the document proves it (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, tsDoc());
      allOk(t, result, [TS.shortList, TS.shortTable, TS.narrowTable, TS.narrowList], "panel");
      // A short panel: the cap is under one line (list) or under the head and one row (table), so the legend height and place are exact.
      for (const id of [TS.shortList, TS.shortTable]) {
        const r = rowOf(result, id, "legend");
        t.eq(r.cls, "exact", `panel ${id}: the legend is exact`);
        t.assert(r.delta.every((d) => Math.abs(d) <= 0.5), `panel ${id}: the legend is the same within 0.5 px ${JSON.stringify(r.delta)}`);
        t.eq(rowOf(result, id, "plot").loaded[3], 40, `panel ${id}: the plot has its least height of 40 px`);
      }
      // A narrow panel with a table at the right: the fallback is certain, and the loaded legend is under the plot too.
      const narrow = hosts[TS.narrowTable];
      t.eq(narrow.skeleton, { placement: "right", fallback: true }, "the preview puts the legend under the plot");
      t.eq(narrow.loaded, { placement: "right", fallback: true }, "the loaded panel does the same");
      // A list in 328 px: between the two certain cases. The preview stays beside, claims no right edge and no legend part.
      const between = hosts[TS.narrowList];
      t.eq(between.skeleton, { placement: "right", fallback: false }, "the preview keeps the legend beside the plot");
      t.eq(rowOf(result, TS.narrowList, "plot").cls, "contain:ltb", "no right edge");
      t.eq(claimed(result, TS.narrowList).filter((r) => r.part === "legend").length, 0, "no legend part where the fallback can come");
      // A list in 682 px never falls back, and neither does the preview.
      const wide = hosts[TS.listRight];
      t.eq([wide.skeleton.fallback, wide.loaded.fallback], [false, false], "a list in 682 px");
    },
  },
  {
    id: "B334",
    skip: SKIP,
    timeout: 400000,
    name: "xy chart: the band under the plot is 41 px (the x axis has a title), the gutter is bounded, and the legend variants hold (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, barDoc());
      allOk(t, result, [BAR.xyList, BAR.xyTable, BAR.xyRight, BAR.xyOff], "xy chart");
      // draw = points (the default): dots of point_size and no line, no fill.
      for (const id of [BAR.xyList, BAR.xyOff]) t.eq([hosts[id].marks.dots, hosts[id].marks.series, hosts[id].marks.area, hosts[id].dot], [11, 0, 0, "5px"], `xy chart ${id}: dots of 5 px, no line and no fill`);
      for (const id of [BAR.xyList, BAR.xyOff]) {
        const r = rowOf(result, id, "plot");
        t.eq(r.cls, "contain:ltrb", `xy chart ${id}: the plot is bounded`);
        t.assert(Math.abs(r.preview[1] + r.preview[3] - (r.loaded[1] + r.loaded[3])) <= 0.5, `xy chart ${id}: the bottom edge is the same (41 px band): ${JSON.stringify(r)}`);
      }
      t.eq(rowOf(result, BAR.xyList, "plot").loaded[1], 8, "the top is 8 px");
      t.assert(rowOf(result, BAR.xyTable, "head").verdict === "ok", "the head row of the table legend");
    },
  },
  {
    id: "B335",
    skip: SKIP,
    timeout: 400000,
    name: "bar chart, vertical: the frame of the engine. The legend variants, the gutter and the title room hold; the document fixes the plot when it fixes the axis width (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, barDoc());
      allOk(t, result, [BAR.list, BAR.table, BAR.tableEmpty, BAR.listRight, BAR.tableRight, BAR.off, BAR.hidden, BAR.axis, BAR.title], "bar chart");
      // The plot has decorative bars with a fixed period of 14 px: no count of the data. One tone, and no text.
      t.assert(hosts[BAR.list].marks.bars >= 40 && hosts[BAR.list].marks.series === 0, `decorative bars: ${hosts[BAR.list].marks.bars}`);
      // The common case: top 8, right W - 8, band 21; the gutter is the least one (the tick 0 is certain: the bars start at 0).
      const list = rowOf(result, BAR.list, "plot");
      t.eq(list.cls, "contain:ltrb", "the plot is bounded");
      t.eq([list.preview[1], list.preview[0] + list.preview[2]], [list.loaded[1], list.loaded[0] + list.loaded[2]], "the top and the right edge are the loaded ones");
      t.assert(list.preview[0] >= 9 && list.preview[0] <= list.loaded[0] + 0.5, `the least gutter ${list.preview[0]}`);
      // axis_width 40 and a title: the plot is exact.
      const axis = rowOf(result, BAR.axis, "plot");
      t.eq(axis.cls, "exact", "the plot with axis_width and a title");
      t.assert(axis.delta.every((d) => Math.abs(d) <= 0.5), `the plot is the same within 0.5 px ${JSON.stringify(axis.delta)}`);
      // A title with soft ends: the room of the title (28 px) is in the least gutter.
      const title = rowOf(result, BAR.title, "plot");
      t.assert(title.preview[0] >= 9 + 28 && title.preview[0] <= title.loaded[0] + 0.5, `the title room: ${title.preview[0]}, loaded ${title.loaded[0]}`);
      // The table legend with no value: the column "Name" only (4.26).
      t.assert(rowOf(result, BAR.tableEmpty, "head").verdict === "ok", "the head row of a table with no value");
      t.eq(rowsOf(result, BAR.tableEmpty).filter((r) => r.part === "head-label").length, 1, "the column Name only");
      t.eq(claimed(result, BAR.hidden).filter((r) => r.part === "legend").length, 0, "every entry hidden: no legend");
    },
  },
  {
    id: "B336",
    skip: SKIP,
    timeout: 400000,
    name: "bar chart, vertical: a turned x label makes the band data, so the preview takes the least band (21 px) and the loaded plot is lower (9.11)",
    async run(t) {
      const { result } = await measured(t, barDoc());
      allOk(t, result, [BAR.turned, BAR.short], "bar chart");
      const turned = rowOf(result, BAR.turned, "plot");
      t.assert(turned.loaded[3] < turned.preview[3] - 1, `the turned labels shorten the loaded plot: ${turned.preview[3]} and ${turned.loaded[3]}`);
      t.eq(turned.preview[1], 8, "the top is not moved");
      t.eq(rowOf(result, BAR.short, "legend").cls, "exact", "a short panel: the cap of the legend");
    },
  },
  {
    id: "B337",
    skip: SKIP,
    timeout: 400000,
    name: "bar chart, horizontal: the preview uses the markup of the loaded chart and follows its legend (list or table, hidden, cap, fallback under the plot). The plot holds the loaded plot (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, barDoc());
      allOk(t, result, [BAR.hBottom, BAR.hRight, BAR.hOff, BAR.hBand, BAR.hPercent, BAR.hTable, BAR.hHidden, BAR.hFallbackList, BAR.hFallbackTable, BAR.hBetween, BAR.hShort, BAR.hTableEmpty], "horizontal bar chart");
      for (const id of [BAR.hBottom, BAR.hRight, BAR.hOff, BAR.hBand]) {
        const r = rowOf(result, id, "plot");
        t.eq(r.cls, "contain:ltrb", `panel ${id}: the plot is bounded`);
        t.eq(r.preview[1], 8, `panel ${id}: the top is 8 px`);
        t.assert(r.preview[0] === 22 && r.loaded[0] >= 22, `panel ${id}: the label column is 22 px at least, loaded ${r.loaded[0]}`);
        t.eq(hosts[id].hbar, true, `panel ${id}: the preview has the markup of the horizontal chart`);
      }
      // The title and the axis width move the bottom edge. The document fixes them, so the bottom edge is the loaded one.
      for (const id of [BAR.hOff, BAR.hBand]) {
        const r = rowOf(result, id, "plot");
        t.assert(Math.abs(r.preview[1] + r.preview[3] - (r.loaded[1] + r.loaded[3])) <= 0.5, `panel ${id}: the bottom edge is the same: ${JSON.stringify(r)}`);
      }
      // A list at the right: a column that keeps at most 30 % of the width.
      const right = rowOf(result, BAR.hRight, "legend");
      t.assert(right.preview[2] <= right.loaded[2] + 0.5 && right.preview[3] <= right.loaded[3] + 0.5, `the right legend: ${JSON.stringify(right)}`);
      // A table follows the mode: the head row, and the first and the last head text, as the vertical form.
      t.eq(rowOf(result, BAR.hTable, "head").verdict, "ok", "the head row of a table legend");
      t.eq(claimed(result, BAR.hTable).filter((r) => r.part === "head-label").map((r) => r.text), ["Name", "Last"], "the first and the last head text");
      t.eq(claimed(result, BAR.hTableEmpty).filter((r) => r.part === "head-label").map((r) => r.text), ["Name"], "values [] gives the column Name only (4.26)");
      // Every entry hidden at the right: no legend and no room beside the plot.
      t.eq(hosts[BAR.hHidden].legends, 0, "every entry hidden: no legend in the preview");
      const hidden = rowOf(result, BAR.hHidden, "plot");
      t.eq(hidden.loaded[0] + hidden.loaded[2], 682 - 8, "the plot ends 8 px before the right edge");
      t.assert(Math.abs(hidden.preview[0] + hidden.preview[2] - (hidden.loaded[0] + hidden.loaded[2])) <= 0.5, `the right edge of the plot: ${JSON.stringify(hidden)}`);
      // The legend goes under the plot when the plot would have less than 220 px: the preview does the same where it is certain.
      t.eq([hosts[BAR.hFallbackList].skeleton.fallback, hosts[BAR.hFallbackList].loaded.fallback], [true, true], "a list in a narrow panel");
      t.eq([hosts[BAR.hFallbackTable].skeleton.fallback, hosts[BAR.hFallbackTable].loaded.fallback], [true, true], "a table in a narrow panel");
      // Between the two certain cases the preview keeps the legend beside the plot, claims no right edge and no legend part.
      t.eq(hosts[BAR.hBetween].skeleton.fallback, false, "a table where the fallback can come: the preview stays beside the plot");
      t.eq(rowOf(result, BAR.hBetween, "plot").cls, "contain:ltb", "no right edge");
      t.eq(claimed(result, BAR.hBetween).filter((r) => r.part === "legend").length, 0, "no legend part");
      // A short panel: the cap of 35 % is under one line, so the legend is exact, and the plot has no height: no plot part.
      t.eq(rowOf(result, BAR.hShort, "legend").cls, "exact", "a short panel: the legend height is the cap");
      t.eq(claimed(result, BAR.hShort).filter((r) => r.part === "plot").length, 0, "a plot with no height is not measured");
      // A percent stack: the label 100% is a bar at the right end of the axis.
      const tick = rowOf(result, BAR.hPercent, "tick");
      t.assert(tick && tick.cls === "exact" && tick.verdict === "ok" && tick.text === "100%", `the label 100%: ${JSON.stringify(tick)}`);
      // The decorative bars: rows with bars of different lengths.
      t.assert(hosts[BAR.hBottom].marks.bars >= 3, `bars in the plot: ${hosts[BAR.hBottom].marks.bars}`);
    },
  },
  {
    id: "B338",
    skip: SKIP,
    timeout: 400000,
    name: "bar chart: orientation auto draws the horizontal form when the box is higher than wide, and the preview follows the size of the panel during its first load (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, barDoc());
      allOk(t, result, [BAR.autoTall, BAR.autoWide], "bar chart");
      const tall = hosts[BAR.autoTall];
      const wide = hosts[BAR.autoWide];
      t.eq([tall.hbar, tall.loadedHbar], [true, true], "auto in a box that is higher than wide: horizontal, preview and loaded panel");
      t.eq([wide.hbar, wide.loadedHbar], [false, false], "auto in a box that is wider than high: vertical, preview and loaded panel");
      // The preview follows its box: a new width during the first load gives a new plot, and the loaded plot then agrees.
      const doc = timeSeriesVariantsDoc("pv338");
      const r = await t.api.create("transit", doc);
      t.assert(r.status === 201, `create pv338: ${r.status}`);
      const p = await t.page({ width: 1440, height: HEIGHT });
      const hold = p.holdWhen(HOLD);
      await p.goto(t.dash("pv338"));
      const plot = `(() => { const e = document.querySelector('[data-testid="panel-${TS.axesHidden}"] .skel__plot'); const s = document.querySelector('[data-testid="panel-${TS.axesHidden}"] .panel__skeleton'); if (!e) return null; const b = e.getBoundingClientRect(); const o = s.getBoundingClientRect(); return { x: b.x - o.x, w: b.width, box: s.clientWidth }; })()`;
      await p.waitFor(`(${plot}) !== null`, { what: "the preview of the panel", timeout: 30000 });
      await sleep(300);
      const before = await p.eval(plot);
      await p.send("Emulation.setDeviceMetricsOverride", { width: 1000, height: HEIGHT, deviceScaleFactor: 1, mobile: false });
      await sleep(700);
      const after = await p.eval(plot);
      t.assert(after.box < before.box, `the box got narrower: ${before.box} then ${after.box}`);
      t.eq([after.x, after.x + after.w + 8], [8, after.box], "the plot follows the box: left gutter 8, right gutter 8");
      // A font that arrives late (the event loadingdone of document.fonts) builds the preview again with new measures (R13).
      const rebuilt = await p.eval(`(() => { const sk = document.querySelector('[data-testid="panel-${TS.axesHidden}"] .panel__skeleton'); const old = sk._preview; document.fonts.dispatchEvent(new Event("loadingdone")); return !!sk._preview && sk._preview !== old; })()`);
      t.eq(rebuilt, true, "a font load builds the preview again");
      hold.release();
      await p.waitFor(`document.querySelector('[data-testid="panel-${TS.axesHidden}"]').dataset.state === "ready"`, { what: "the panel", timeout: 60000 });
      await sleep(900);
      const loaded = await p.eval(`(() => { const core = document.querySelector('[data-testid="panel-${TS.axesHidden}"] .chartCore'); const [l, t, w, h] = core.dataset.plot.split(" ").map(Number); return { l, w }; })()`);
      t.eq([loaded.l, loaded.l + loaded.w + 8], [8, after.box], "the loaded plot agrees with the preview at the new size");
    },
  },
  {
    id: "B339",
    skip: SKIP,
    timeout: 400000,
    name: "the decorative form of the series follows the style of the document (line, fill, dash, dots, bars), and the preview of the charts uses neutral tones in the light and the dark theme, no colour of the document, no style attribute and no text; the captures are written when HUNE_PREVIEW_DUMP is set (9.11)",
    async run(t) {
      // The marks of the measured dashboards. The style of the document sets the width, the dash, the fill and the dots; nothing sets a count or a value.
      const { hosts } = await measured(t, tsDoc());
      const styles = hosts[TS.styles];
      t.eq([styles.marks.series, styles.marks.area, styles.marks.dots, styles.marks.stops], [1, 1, 11, 2], "smooth line, a fill that fades, dots");
      t.eq([styles.width, styles.dash, styles.dot], ["3px", "10px, 10px", "7px"], "line_width 3, a dash, point_size 7");
      const points = hosts[TS.points];
      t.eq([points.marks.series, points.marks.dots, points.dot], [0, 11, "9px"], "draw points: dots and no line");
      for (const id of [TS.bars, TS.barsOff]) t.assert(hosts[id].marks.bars >= 40 && hosts[id].marks.series === 0, `draw bars: bars and no line in panel ${id}`);
      t.eq([hosts[TS.list].marks.area, hosts[TS.list].marks.dots], [0, 0], "a plain line: no fill and no dot");
      const doc = timeSeriesVariantsDoc("pv339");
      const r = await t.api.create("transit", doc);
      t.assert(r.status === 201, `create pv339: ${r.status}`);
      const bars = barVariantsDoc("pv339b");
      const rb = await t.api.create("transit", bars);
      t.assert(rb.status === 201, `create pv339b: ${rb.status}`);
      const shots = [["pv339", [TS.table, TS.listRight, TS.axesHidden, TS.axisWidth, TS.shortTable, TS.narrowTable, TS.fixedOff, TS.styles, TS.points, TS.bars]], ["pv339b", [BAR.list, BAR.tableRight, BAR.hBottom, BAR.hRight, BAR.turned, BAR.xyTable, BAR.hTable, BAR.hFallbackTable, BAR.hPercent]]];
      for (const [uid, ids] of shots) {
        const p = await t.page({ width: 1440, height: HEIGHT });
        const hold = p.holdWhen(HOLD);
        await p.goto(t.dash(uid));
        await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-type] .skel').length > 0`, { what: "the previews", timeout: 30000 });
        await sleep(900);
        for (const theme of ["light", "dark"]) {
          await p.eval(`localStorage.setItem("hune.theme", ${JSON.stringify(theme)}); document.documentElement.setAttribute("data-theme", ${JSON.stringify(theme)}); document.dispatchEvent(new Event("hune-theme"));`);
          await sleep(300);
          const probe = await p.eval(`(() => {
            const out = { styled: 0, text: 0, bad: [], checked: 0 };
            for (const sk of document.querySelectorAll(".panel__skeleton")) {
              if (getComputedStyle(sk).display === "none") continue;
              const root = sk.firstElementChild;
              out.styled += [...root.querySelectorAll("[style]")].filter((e) => [...e.style].some((n) => !n.startsWith("--"))).length;
              out.text += [...root.querySelectorAll("*")].filter((e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.nodeValue.trim())).length;
              const tone = (name) => { const e = document.createElement("i"); e.style.background = "var(" + name + ")"; sk.append(e); const c = getComputedStyle(e).backgroundColor; e.remove(); return c; };
              const tones = new Set([tone("--skel-fill"), tone("--skel-edge"), tone("--skel-grid"), "rgba(0, 0, 0, 0)", "none"]);
              const check = (sel, prop) => { for (const e of root.querySelectorAll(sel)) { const c = getComputedStyle(e)[prop]; out.checked += 1; if (!tones.has(c)) out.bad.push(sel + " " + prop + " " + c); } };
              check(".skel__plot, .chartCore__legendItem i, .skel__hb, .hbar__grid, .hbar__legendItem > i, .skel__dot, .skel__tick, .skel__b", "backgroundColor");
              check(".skel__bar, .skel__area", "fill");
              for (const e of root.querySelectorAll(".skel__fade")) { out.checked += 1; if (!/^url\(/.test(getComputedStyle(e).fill)) out.bad.push("fade " + getComputedStyle(e).fill); }
              check(".skel__series, .skel__line", "stroke");
              check(".skel__stop", "stopColor");
            }
            return out;
          })()`);
          t.assert(probe.checked > 50, `${uid} ${theme}: the probe checked ${probe.checked} elements`);
          t.eq({ styled: probe.styled, text: probe.text, bad: probe.bad }, { styled: 0, text: 0, bad: [] }, `${uid} ${theme}: neutral tones, no style but custom properties, no text`);
          if (DUMP) {
            mkdirSync(DUMP, { recursive: true });
            for (const id of ids) {
              await p.eval(`document.querySelector('[data-testid="panel-${id}"]').scrollIntoView({ block: "center" })`);
              await sleep(200);
              const b = await p.eval(`(() => { const b = document.querySelector('[data-testid="panel-${id}"]').getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; })()`);
              const shot = await p.send("Page.captureScreenshot", { format: "png", clip: { x: b.x, y: b.y, width: b.w, height: b.h, scale: 1 } });
              writeFileSync(`${DUMP}/${uid}-${id}-${theme}.png`, Buffer.from(shot.data, "base64"));
            }
          }
        }
        hold.release();
      }
    },
  },
];
