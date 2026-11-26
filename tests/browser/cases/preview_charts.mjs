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
const TS = { list: 1, table: 2, tableEmpty: 3, listRight: 4, tableRight: 5, hiddenBottom: 6, hiddenRight: 7, off: 8, axesHidden: 9, axisRight: 10, axisWidth: 11, fixed: 12, soft: 13, undecided: 14, undecidedHide: 15, queryOverride: 16, shortList: 17, shortTable: 18, narrowTable: 19, narrowList: 20, bars: 21 };
const BAR = { list: 1, table: 2, tableEmpty: 3, listRight: 4, tableRight: 5, off: 6, hidden: 7, axis: 8, title: 9, turned: 10, hBottom: 11, hRight: 12, hOff: 13, hBand: 14, hPercent: 15, autoTall: 16, autoWide: 17, short: 18, xyList: 19, xyTable: 20, xyRight: 21, xyOff: 22 };

// The host of the preview and the host of the loaded panel of each panel. After the load, the preview is hidden and its markup stays.
const hostInfo = (page, ids) => page.eval(`(() => {
  const out = {};
  for (const id of ${JSON.stringify(ids)}) {
    const p = document.querySelector('[data-testid="panel-' + id + '"]');
    const sk = p.querySelector(".panel__skeleton .skel__host");
    const lo = p.querySelector(".panel__plot");
    const at = (e) => (e ? { placement: e.getAttribute("data-legend-placement"), fallback: e.hasAttribute("data-legend-fallback") } : null);
    out[id] = { skeleton: at(sk), loaded: at(lo), hbar: !!p.querySelector(".panel__skeleton .hbar"), loadedHbar: !!p.querySelector(".panel__plot .hbar"), legends: p.querySelectorAll(".panel__skeleton .chartCore__legend").length };
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
      // No value in the document: the engine shows the five calculations (D1), and so does the preview.
      const empty = claimed(result, TS.tableEmpty).find((r) => r.part === "head");
      t.assert(empty && empty.verdict === "ok", "the head row of a table with no value");
      t.eq(rowsOf(result, TS.tableEmpty).filter((r) => r.part === "head-label").length, 2, "the first and the last head text of the five columns");
      // Every entry hidden: no legend, and at the right the loaded plot is 8 px narrower (the gap stays, defect D3).
      for (const id of [TS.hiddenBottom, TS.hiddenRight, TS.off]) t.eq(claimed(result, id).filter((r) => r.part === "legend").length, 0, `panel ${id}: no legend part`);
      const hr = rowOf(result, TS.hiddenRight, "plot");
      t.assert(Math.abs(hr.preview[0] + hr.preview[2] - (hr.loaded[0] + hr.loaded[2])) <= 0.5, `the right edge with every entry hidden at the right: ${JSON.stringify(hr)}`);
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
      allOk(t, result, [TS.axesHidden, TS.axisRight, TS.axisWidth, TS.fixed, TS.soft, TS.queryOverride, TS.bars], "panel");
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
    },
  },
  {
    id: "B332",
    skip: SKIP,
    timeout: 400000,
    name: "time series: an override that the document does not decide leaves the least form. The legend may be hidden by an override, so the preview draws none (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, tsDoc());
      allOk(t, result, [TS.undecided, TS.undecidedHide], "panel");
      // Overrides by name set the placement, the unit and the width: no equality, the plot holds the loaded plot, and the top is not claimed.
      const u = rowOf(result, TS.undecided, "plot");
      t.eq(u.cls, "contain:lrb", "overrides of the placement: the top margin is not claimed");
      t.assert(u.preview[0] <= u.loaded[0] + 0.5, `the least gutter ${u.preview[0]} is at most ${u.loaded[0]}`);
      // An override can hide some entries: the preview has no legend and takes the whole height.
      t.eq(claimed(result, TS.undecidedHide).filter((r) => r.part === "legend").length, 0, "no legend part");
      t.eq(hosts[TS.undecidedHide].legends, 0, "no legend element in the preview");
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
      const { result } = await measured(t, barDoc());
      allOk(t, result, [BAR.xyList, BAR.xyTable, BAR.xyRight, BAR.xyOff], "xy chart");
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
      const { result } = await measured(t, barDoc());
      allOk(t, result, [BAR.list, BAR.table, BAR.tableEmpty, BAR.listRight, BAR.tableRight, BAR.off, BAR.hidden, BAR.axis, BAR.title], "bar chart");
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
      // The table legend with no value: the five columns of the engine.
      t.assert(rowOf(result, BAR.tableEmpty, "head").verdict === "ok", "the head row of a table with no value");
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
    name: "bar chart, horizontal: the preview uses the markup of the loaded chart. The plot holds the loaded plot, with the label column of 22 px at least (9.11)",
    async run(t) {
      const { result, hosts } = await measured(t, barDoc());
      allOk(t, result, [BAR.hBottom, BAR.hRight, BAR.hOff, BAR.hBand, BAR.hPercent], "horizontal bar chart");
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
      // The legend of this form is a list that show and placement control: a bar chart with a right legend has a column at the right.
      const right = rowOf(result, BAR.hRight, "legend");
      t.assert(right.preview[2] <= right.loaded[2] + 0.5 && right.preview[3] <= right.loaded[3] + 0.5, `the right legend: ${JSON.stringify(right)}`);
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
    name: "the preview of the charts uses neutral tones in the light and the dark theme, no colour of the document, no style attribute and no text; the captures are written when HUNE_PREVIEW_DUMP is set (9.11)",
    async run(t) {
      const doc = timeSeriesVariantsDoc("pv339");
      const r = await t.api.create("transit", doc);
      t.assert(r.status === 201, `create pv339: ${r.status}`);
      const bars = barVariantsDoc("pv339b");
      const rb = await t.api.create("transit", bars);
      t.assert(rb.status === 201, `create pv339b: ${rb.status}`);
      const shots = [["pv339", [TS.table, TS.listRight, TS.axesHidden, TS.axisWidth, TS.shortTable, TS.narrowTable]], ["pv339b", [BAR.list, BAR.tableRight, BAR.hBottom, BAR.hRight, BAR.turned, BAR.xyTable]]];
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
            const out = { styled: 0, text: 0, bad: [] };
            for (const sk of document.querySelectorAll(".panel__skeleton")) {
              if (getComputedStyle(sk).display === "none") continue;
              const root = sk.firstElementChild;
              out.styled += [...root.querySelectorAll("[style]")].filter((e) => [...e.style].some((n) => !n.startsWith("--"))).length;
              out.text += [...root.querySelectorAll("*")].filter((e) => [...e.childNodes].some((n) => n.nodeType === 3 && n.nodeValue.trim())).length;
              const tone = (name) => { const e = document.createElement("i"); e.style.background = "var(" + name + ")"; sk.append(e); const c = getComputedStyle(e).backgroundColor; e.remove(); return c; };
              const tones = new Set([tone("--skel-fill"), tone("--skel-edge"), tone("--skel-grid"), "rgba(0, 0, 0, 0)"]);
              for (const e of root.querySelectorAll(".skel__plot, .chartCore__legendItem i, .skel__hb, .hbar__grid, .hbar__legendItem > i")) {
                const c = getComputedStyle(e).backgroundColor;
                if (!tones.has(c)) out.bad.push(e.className + " " + c);
              }
            }
            return out;
          })()`);
          t.eq(probe, { styled: 0, text: 0, bad: [] }, `${uid} ${theme}: neutral tones, no style but custom properties, no text`);
          if (DUMP) {
            mkdirSync(DUMP, { recursive: true });
            for (const id of ids) {
              const box = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-${id}"]'); e.scrollIntoView({ block: "center" }); const b = e.getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; })()`);
              await sleep(200);
              const b2 = await p.eval(`(() => { const b = document.querySelector('[data-testid="panel-${id}"]').getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; })()`);
              const shot = await p.send("Page.captureScreenshot", { format: "png", clip: { x: b2.x, y: b2.y, width: b2.w, height: b2.h, scale: 1 } });
              writeFileSync(`${DUMP}/${uid}-${id}-${theme}.png`, Buffer.from(shot.data, "base64"));
              void box;
            }
          }
        }
        hold.release();
      }
    },
  },
];
