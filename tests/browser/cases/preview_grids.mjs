// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B360 to B369 (9.11): the loading preview of the grid family, the heatmap, the histogram, the state timeline and the status history.
// Each case measures the preview of the panels of a fixture dashboard (lib/grids_fixtures.mjs) while the queries are held, then measures the loaded
// panels. A part that the document fixes (exact) agrees within 0.5 px. A part that the document bounds (least, most) agrees on one side only: the plot
// top never moves, the plot left only moves right, the plot right and the plot bottom only move up or left. A part that the data decide has no shape.
// HUNE_PREVIEW_DUMP=<dir> writes the table of the harness of each case to <dir>/<case>.txt. HUNE_PREVIEW_SHOTS=<dir> writes the captures of B368.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { formatTable, measureDashboard } from "../lib/preview_measure.mjs";
import { RANGE, heatmapDoc, histogramDoc, timelineDoc } from "../lib/grids_fixtures.mjs";
import { sleep } from "../lib/h.mjs";

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const TOL = 0.5;
// The error that the page logs for a panel that the document proves to be never drawn.
const NEVER_DRAWN = /The data has no (number column|field to draw|time column)/;
// The page of a fixture dashboard in the range of its data (the helper t.dash gives the range of the seed table).
const pageOf = (t, uid) => t.url(`d/${uid}?from=${RANGE.from}&to=${RANGE.to}&timezone=utc`);

const create = async (t, d) => {
  if ((await t.api.dashboard(d.uid)).status === 200) return;
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${d.uid}: ${r.status} ${r.text.slice(0, 600)}`);
};

// The measure of a fixture dashboard, once for each run of the suite.
const cache = new Map();
async function measure(t, name, build, { width = 1440, height = 4000 } = {}) {
  const key = `${name}:${width}`;
  if (cache.has(key)) return cache.get(key);
  const d = build(`${name}-${width}`);
  await create(t, d);
  const p = await t.page({ width, height });
  // A panel that the document proves to be never drawn ends in a message: the page logs the error of the model for it.
  t.allow(p, NEVER_DRAWN);
  const result = await measureDashboard(p, pageOf(t, d.uid), { width, height });
  if (process.env.HUNE_PREVIEW_DUMP) {
    mkdirSync(process.env.HUNE_PREVIEW_DUMP, { recursive: true });
    writeFileSync(join(process.env.HUNE_PREVIEW_DUMP, `${name}-${width}.txt`), `${formatTable(result.rows, { all: true })}\n`);
  }
  cache.set(key, result);
  return result;
}

const rowsOf = (result, id, part) => result.rows.filter((r) => String(r.id) === String(id) && r.part === part);
const claimed = (result, id) => result.rows.filter((r) => String(r.id) === String(id) && r.verdict !== "unclaimed");

// The edges of a plot or of a rows envelope by its class: exact = the four edges agree; most = the top agrees, the left is at the left of the
// loaded left, the right and the bottom are at or after the loaded ones.
function edges(t, row, label) {
  t.assert(row && row.preview && row.loaded, `${label}: the part is in both`);
  const [px, py, pw, ph] = row.preview;
  const [lx, ly, lw, lh] = row.loaded;
  if (row.cls === "exact") {
    t.assert([px - lx, py - ly, pw - lw, ph - lh].every((d) => Math.abs(d) <= TOL), `${label}: exact ${row.preview} against ${row.loaded}`);
  } else {
    t.assert(Math.abs(py - ly) <= TOL, `${label}: the top never moves (${py} against ${ly})`);
    t.assert(px <= lx + TOL, `${label}: the left only moves right (${px} against ${lx})`);
    t.assert(px + pw >= lx + lw - TOL, `${label}: the right only moves left (${px + pw} against ${lx + lw})`);
    t.assert(py + ph >= ly + lh - TOL, `${label}: the bottom only moves up (${py + ph} against ${ly + lh})`);
  }
}

const noFailure = (t, result, what) => {
  const bad = result.rows.filter((r) => r.verdict === "fail" || r.verdict === "no-loaded");
  t.eq(bad.map((r) => `${r.id} ${r.part}#${r.index} ${r.verdict} ${JSON.stringify(r.delta)}`), [], `${what}: no claimed part differs from the loaded panel`);
};

const ids = (result) => Object.keys(result.previews);
const state = (result, id) => result.loaded[id].state;

export const cases = [
  {
    id: "B360",
    skip: SKIP,
    timeout: 240000,
    name: "heatmap preview: the plot agrees with the loaded plot on a hidden axis, a fixed axis and a bounded axis (9.11)",
    async run(t) {
      const r = await measure(t, "pv-heat", heatmapDoc);
      noFailure(t, r, "heatmap");
      // Hidden axis (panels 4 and 5): left 8, top 0, exact. Fixed axis (2, 3, 9, 11, 12): exact with the measured gutter. Bounded (1, 10): left only moves right.
      for (const id of [2, 3, 4, 5, 9, 11, 12]) edges(t, rowsOf(r, id, "plot")[0], `panel ${id} plot`);
      for (const id of [1, 10]) {
        const row = rowsOf(r, id, "plot")[0];
        t.eq(row.cls, "most", `panel ${id}: the left edge is bounded`);
        edges(t, row, `panel ${id} plot`);
        t.assert(row.loaded[0] >= 9 - TOL, `panel ${id}: the loaded left edge is 9 or more`);
      }
      // The classes: a hidden axis and a fixed axis are exact. The note names the rule.
      for (const id of [2, 3, 4, 5, 9, 11, 12]) t.eq(rowsOf(r, id, "plot")[0].cls, "exact", `panel ${id}: the plot is exact`);
      t.assert(rowsOf(r, 4, "plot")[0].loaded[1] === 0 || Math.abs(rowsOf(r, 4, "plot")[0].loaded[1]) <= TOL, "a hidden axis starts the plot at the top");
      // The short body: the canvas has a floor of 60 px (panel 9).
      const short = rowsOf(r, 9, "plot")[0];
      t.assert(Math.abs(short.preview[3] - short.loaded[3]) <= TOL && short.loaded[3] >= 40, `short body: the plot height ${short.loaded[3]} agrees`);
    },
  },
  {
    id: "B361",
    skip: SKIP,
    timeout: 240000,
    name: "heatmap preview: the y labels and the grid of a fixed axis, the colour bar and the legend ends agree with the loaded panel (9.11)",
    async run(t) {
      const r = await measure(t, "pv-heat", heatmapDoc);
      // The labels and the grid of the fixed axis: the same count, the same texts' widths and places.
      for (const id of [2, 3, 9, 11, 12]) {
        const labels = rowsOf(r, id, "label").filter((x) => x.preview);
        const grid = rowsOf(r, id, "grid").filter((x) => x.preview);
        t.assert(labels.length >= 2, `panel ${id}: the fixed axis has y labels (${labels.length})`);
        t.eq(grid.length, labels.length, `panel ${id}: one grid line for each label`);
        for (const l of labels) t.eq(l.verdict, "ok", `panel ${id} label ${l.index} ${l.text}: ${JSON.stringify(l.delta)}`);
        for (const g of grid) t.eq(g.verdict, "ok", `panel ${id} grid ${g.index}: ${JSON.stringify(g.delta)}`);
        const loadedLabels = rowsOf(r, id, "label").length;
        t.assert(loadedLabels >= labels.length, `panel ${id}: the loaded panel has the labels of the preview (${loadedLabels})`);
      }
      // No label and no grid line outside the fixed axis.
      for (const id of [1, 4, 5, 10]) {
        t.eq(rowsOf(r, id, "label").filter((x) => x.preview).length, 0, `panel ${id}: no y label bar`);
        t.eq(rowsOf(r, id, "grid").filter((x) => x.preview).length, 0, `panel ${id}: no grid line`);
      }
      // The colour bar: exact for a body of 350 px or more and for a narrow body (panel 5 is 6 columns wide: under 350 px).
      for (const id of [1, 2, 3, 5, 9, 12]) {
        const scale = rowsOf(r, id, "scale")[0];
        t.assert(scale && scale.verdict === "ok", `panel ${id}: the colour bar agrees ${scale ? JSON.stringify(scale.delta) : "-"}`);
        const legend = rowsOf(r, id, "legend")[0];
        t.assert(legend && legend.verdict === "ok", `panel ${id}: the legend agrees ${legend ? JSON.stringify(legend.delta) : "-"}`);
      }
      t.assert(r.previews[5].box[0] < 350, `panel 5 is narrower than 350 px (${r.previews[5].box[0]})`);
      t.assert(r.previews[1].box[0] >= 350, `panel 1 is 350 px or wider (${r.previews[1].box[0]})`);
      // The legend ends: both ends numbers (panels 2, 5) give two bars, the left end only (panel 3) gives one, no number gives none.
      t.eq(rowsOf(r, 2, "legend-label").filter((x) => x.preview).map((x) => x.verdict), ["ok", "ok"], "panel 2: both ends");
      t.eq(rowsOf(r, 5, "legend-label").filter((x) => x.preview).map((x) => x.verdict), ["ok", "ok"], "panel 5: both ends, narrow");
      t.eq(rowsOf(r, 3, "legend-label").filter((x) => x.preview).map((x) => x.verdict), ["ok"], "panel 3: the left end");
      t.eq(rowsOf(r, 1, "legend-label").filter((x) => x.preview).length, 0, "panel 1: no bar for an end that the data decide");
      // No legend: no band, no bar (panel 4).
      t.eq(claimed(r, 4).filter((x) => x.part === "legend" || x.part === "scale").length, 0, "panel 4: no legend");
    },
  },
  {
    id: "B362",
    skip: SKIP,
    timeout: 240000,
    name: "heatmap preview: the neutral form where the document proves that no picture comes, and nothing for the cells and the x labels (9.11)",
    async run(t) {
      const r = await measure(t, "pv-heat", heatmapDoc);
      // E2, E3 and E4 (panels 6, 7, 8): the loaded body is never a picture, and the preview has no shape.
      for (const id of [6, 7, 8]) {
        t.assert(["empty", "error", "bucket-limit"].includes(state(r, id)), `panel ${id}: the loaded body is a message (${state(r, id)})`);
        t.eq(r.previews[id].children, 0, `panel ${id}: the preview has no shape`);
        t.eq(r.previews[id].parts.length, 0, `panel ${id}: no claimed part`);
        t.assert(/^no-picture:E[234]$/.test(r.previews[id].notes), `panel ${id}: the note names the rule (${r.previews[id].notes})`);
      }
      // The other panels: no cell, no row and no x label is claimed. The x band stays empty.
      for (const id of [1, 2, 3, 4, 5, 9, 10, 11, 12]) {
        const parts = r.previews[id].parts.map((p) => p.part);
        for (const forbidden of ["row", "column", "axis-x", "tile"]) t.assert(!parts.includes(forbidden), `panel ${id}: no ${forbidden}`);
        const plot = r.previews[id].parts.find((p) => p.part === "plot");
        // Nothing of the preview lies in the band of the x labels, under the plot.
        const band = plot.rect[1] + plot.rect[3];
        for (const p of r.previews[id].parts) {
          if (p.part === "legend" || p.part === "scale" || p.part === "legend-label") continue;
          // A y label is centred on its tick, so its centre is above the base of the plot, and its bar can pass it by half its height.
          const edge = p.part === "label" ? p.rect[1] + p.rect[3] / 2 : p.part === "grid" ? p.rect[1] : p.rect[1] + p.rect[3];
          t.assert(edge <= band + TOL, `panel ${id}: the part ${p.part} is not in the x band`);
        }
      }
    },
  },
  {
    id: "B363",
    skip: SKIP,
    timeout: 240000,
    name: "histogram preview: the plot agrees with the loaded plot on each legend variant, the left edge only moves right (9.11)",
    async run(t) {
      const r = await measure(t, "pv-hist", histogramDoc);
      noFailure(t, r, "histogram");
      const drawn = [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13];
      for (const id of drawn) edges(t, rowsOf(r, id, "plot")[0], `panel ${id} plot`);
      // The top is 8 and the right margin is 8 for every histogram, whatever the legend. The legend off and an empty legend take no room under the plot.
      for (const id of [2, 7]) {
        const p = rowsOf(r, id, "plot")[0];
        t.assert(Math.abs(p.loaded[1] - 8) <= TOL, `panel ${id}: the plot top is 8`);
      }
      const off = rowsOf(r, 2, "plot")[0];
      t.assert(Math.abs(off.preview[0] + off.preview[2] - (r.previews[2].box[0] - 8)) <= TOL, "legend off: the right edge is the box less 8");
      t.assert(Math.abs(off.loaded[0] + off.loaded[2] - (r.previews[2].box[0] - 8)) <= TOL, "legend off: the loaded right edge is the box less 8");
      // An empty legend at the right keeps the gap of the grid: the right edge is the box less 16.
      const empty = rowsOf(r, 7, "plot")[0];
      t.assert(Math.abs(empty.loaded[0] + empty.loaded[2] - (r.previews[7].box[0] - 16)) <= TOL, `empty legend: the loaded right edge is the box less 16 (${empty.loaded[0] + empty.loaded[2]})`);
      t.assert(Math.abs(empty.preview[0] + empty.preview[2] - (r.previews[7].box[0] - 16)) <= TOL, "empty legend: the preview right edge is the box less 16");
      // The label 0 and its grid line are the marks that every histogram has: the 0 ends 5 px before the plot, the line is at the base of the plot.
      for (const id of drawn) {
        const label = rowsOf(r, id, "label")[0];
        t.assert(label && label.preview && label.verdict === "ok", `panel ${id}: the label 0 agrees ${label ? JSON.stringify(label.delta) : "-"}`);
        const line = rowsOf(r, id, "grid")[0];
        t.assert(line && line.preview && line.verdict === "ok", `panel ${id}: the base line agrees ${line ? JSON.stringify(line.delta) : "-"}`);
      }
    },
  },
  {
    id: "B364",
    skip: SKIP,
    timeout: 240000,
    name: "histogram preview: the legend, its pill, its name and the fallback agree with the loaded legend (9.11)",
    async run(t) {
      const r = await measure(t, "pv-hist", histogramDoc);
      // The legend of each ON panel: the preview legend takes at most the loaded room, and the pill is the same size.
      for (const id of [1, 3, 4, 5, 6, 8, 9, 12, 13]) {
        const legend = rowsOf(r, id, "legend")[0];
        t.assert(legend && legend.verdict === "ok", `panel ${id}: the legend agrees ${legend ? JSON.stringify(legend.delta) : "-"}`);
      }
      for (const id of [1, 3, 6, 8, 9, 12]) {
        const mark = rowsOf(r, id, "mark")[0];
        t.assert(mark && mark.verdict === "ok", `panel ${id}: the pill agrees ${mark ? JSON.stringify(mark.delta) : "-"}`);
      }
      // The name that the document fixes: Count (panels 8 and 9), as a bar of the measured width.
      for (const id of [8, 9]) {
        const name = rowsOf(r, id, "legend-label")[0];
        t.assert(name && name.preview && name.verdict === "ok", `panel ${id}: the name Count agrees ${name ? JSON.stringify(name.delta) : "-"}`);
        t.eq(name.text, "Count", `panel ${id}: the loaded name`);
      }
      // A list that the document does not name: no bar for the name.
      for (const id of [1, 3, 12]) t.eq(rowsOf(r, id, "legend-label").filter((x) => x.preview).length, 0, `panel ${id}: no name bar`);
      // The table legend: the head Name.
      for (const id of [4, 5, 13]) {
        const head = rowsOf(r, id, "legend-head")[0];
        t.assert(head && head.verdict === "ok", `panel ${id}: the head Name agrees ${head ? JSON.stringify(head.delta) : "-"}`);
      }
      // The legend that moves under the plot (panel 6 is 4 columns wide): the preview knows it, and so does the loaded panel.
      t.assert(/legend:fallback/.test(r.previews[6].notes), `panel 6: the preview states the fallback (${r.previews[6].notes})`);
      const fallback = rowsOf(r, 6, "legend")[0];
      t.assert(fallback.loaded[2] >= r.previews[6].box[0] - TOL, "panel 6: the loaded legend is under the plot, full width");
      // The right legends that stay beside the plot.
      for (const id of [3, 5, 9]) t.assert(/legend:right/.test(r.previews[id].notes), `panel ${id}: the legend is beside the plot (${r.previews[id].notes})`);
    },
  },
  {
    id: "B365",
    skip: SKIP,
    timeout: 240000,
    name: "histogram preview: the neutral form where the document does not decide, and the panel that never draws a chart (9.11)",
    async run(t) {
      const r = await measure(t, "pv-hist", histogramDoc);
      // No chart (panel 10): the loaded body is a message, and the preview has no shape.
      t.assert(["empty", "error"].includes(state(r, 10)), `panel 10: the loaded body is a message (${state(r, 10)})`);
      t.eq(r.previews[10].children, 0, "panel 10: the preview has no shape");
      t.eq(r.previews[10].notes, "no-chart", "panel 10: the note names the rule");
      // Every entry hidden at the right (panel 7): no legend, whatever the show option says. The legend is not drawn.
      t.eq(rowsOf(r, 7, "legend").filter((x) => x.preview).length, 0, "panel 7: no legend shape");
      // The bars, the x labels and the other y labels are data: no bar of the plot, no x label, no more than the label 0 unless the labels are exact.
      for (const id of [1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 13]) {
        const parts = r.previews[id].parts.map((p) => p.part);
        t.eq(parts.filter((p) => p === "label").length, 1, `panel ${id}: the label 0 only`);
        t.eq(parts.filter((p) => p === "grid").length, 1, `panel ${id}: the line of the tick 0 only`);
        t.assert(!parts.includes("axis-x") && !parts.includes("row") && !parts.includes("column"), `panel ${id}: no bar, no x label`);
      }
      // COUNT with percent (panel 9): every share is 1, so the labels and the lines follow from the height: exact, with every tick.
      const ticks = rowsOf(r, 9, "label").filter((x) => x.preview);
      t.assert(ticks.length >= 2, `panel 9: the percent labels follow from the height (${ticks.length})`);
      for (const l of ticks) t.eq(l.verdict, "ok", `panel 9 label ${l.index} ${l.text}: ${JSON.stringify(l.delta)}`);
      t.eq(rowsOf(r, 9, "plot")[0].cls, "exact", "panel 9: the plot is exact");
      // A percent chart that is not COUNT (panel 11): the gutter is bounded.
      t.eq(rowsOf(r, 11, "plot")[0].cls, "most", "panel 11: the plot is bounded");
      // A short body (panel 12, H 48): the canvas has a floor of 60 px.
      const short = rowsOf(r, 12, "plot")[0];
      t.assert(short.loaded[3] >= 40 && Math.abs(short.preview[3] - short.loaded[3]) <= TOL + 40, "short body: the plot is the floor");
    },
  },
  {
    id: "B366",
    skip: SKIP,
    timeout: 240000,
    name: "state timeline preview: the canvas, the legend and the clip box of the plot agree with the loaded panel (9.11)",
    async run(t) {
      const r = await measure(t, "pv-timeline", timelineDoc);
      noFailure(t, r, "timeline");
      // The state timelines (1 to 5, 10, 11): the canvas is exact in the cases A, B and C and bounded in D. The clip box follows the gutter.
      for (const id of [1, 2, 3, 4, 5, 10, 11]) {
        const plot = rowsOf(r, id, "plot")[0];
        t.assert(plot && plot.preview, `panel ${id}: the canvas is claimed`);
        t.assert(plot.verdict === "ok", `panel ${id}: the canvas agrees ${JSON.stringify(plot.delta)}`);
        edges(t, rowsOf(r, id, "rows")[0], `panel ${id} rows`);
      }
      t.eq(rowsOf(r, 4, "plot")[0].cls, "most", "panel 4 (case D): the canvas is bounded");
      for (const id of [1, 2, 3, 5]) t.eq(rowsOf(r, id, "plot")[0].cls, "exact", `panel ${id}: the canvas is exact`);
      // The gutter: 8 without names (panel 2), the exact gutter of one fixed name (panel 5), else the least 21 (panels 1, 3, 4).
      t.eq(rowsOf(r, 2, "rows")[0].cls, "exact", "panel 2: the rows box is exact");
      t.eq(rowsOf(r, 5, "rows")[0].cls, "exact", "panel 5: the rows box is exact with a fixed name");
      for (const id of [1, 3, 4]) {
        t.eq(rowsOf(r, id, "rows")[0].cls, "most", `panel ${id}: the rows box is bounded`);
        t.assert(rowsOf(r, id, "rows")[0].loaded[0] >= 21 - TOL, `panel ${id}: the loaded gutter is 21 or more`);
      }
      t.assert(Math.abs(rowsOf(r, 2, "rows")[0].loaded[0] - 8) <= TOL, "panel 2: the loaded gutter is 8");
    },
  },
  {
    id: "B367",
    skip: SKIP,
    timeout: 240000,
    name: "status history preview: the thresholds legend, its texts and the fallback agree with the loaded panel (9.11)",
    async run(t) {
      const r = await measure(t, "pv-timeline", timelineDoc);
      // Case C: the legend is exact: its box and the texts of its items. Panel 6 has the default thresholds ("< 80", "80+"); panel 8 is at the right.
      for (const id of [3, 5, 6, 8, 10, 11]) {
        const legend = rowsOf(r, id, "legend")[0];
        t.assert(legend && legend.preview && legend.verdict === "ok", `panel ${id}: the legend agrees ${legend ? JSON.stringify(legend.delta) : "-"}`);
        t.eq(legend.cls, "exact", `panel ${id}: the legend is exact`);
        const texts = rowsOf(r, id, "legend-label").filter((x) => x.preview);
        t.assert(texts.length >= 2, `panel ${id}: the items have text bars (${texts.length})`);
        for (const x of texts) t.eq(x.verdict, "ok", `panel ${id} item ${x.index} ${x.text}: ${JSON.stringify(x.delta)}`);
        t.eq(rowsOf(r, id, "legend-label").length, texts.length, `panel ${id}: the loaded panel has no other item`);
      }
      t.eq(rowsOf(r, 6, "legend-label").map((x) => x.text), ["< 80", "80+"], "panel 6: the default thresholds");
      t.eq(rowsOf(r, 8, "legend-label").map((x) => x.text), ["< 80.0%", "80.0%+"], "panel 8: the percent unit with a decimal");
      t.eq(rowsOf(r, 3, "legend-label").map((x) => x.text), ["< 50.0%", "50.0%+", "90.0%+"], "panel 3: three steps");
      // The status history: the plot and the rows are agreed, with the legend at the bottom or at the right.
      for (const id of [6, 7, 8]) {
        t.assert(rowsOf(r, id, "plot")[0].verdict === "ok", `panel ${id}: the canvas agrees`);
        edges(t, rowsOf(r, id, "rows")[0], `panel ${id} rows`);
      }
      // The fallback: the narrow panel 10 has its legend under the plot, in both the preview and the loaded panel.
      t.assert(/legend:fallback/.test(r.previews[10].notes), `panel 10: the preview states the fallback (${r.previews[10].notes})`);
      t.assert(rowsOf(r, 10, "legend")[0].loaded[2] >= r.previews[10].box[0] - TOL, "panel 10: the loaded legend is under the plot");
      // The legend that stays beside the plot (panels 3 and 8).
      for (const id of [3, 8]) t.assert(/legend:right/.test(r.previews[id].notes), `panel ${id}: the legend is beside the plot (${r.previews[id].notes})`);
    },
  },
  {
    id: "B368",
    skip: SKIP,
    timeout: 300000,
    name: "timeline preview: no legend where none can come, no shape for the rows, the names and the axis, and nothing for a panel that never draws (9.11)",
    async run(t) {
      const r = await measure(t, "pv-timeline", timelineDoc);
      // A, B: no legend room: the loaded legend is hidden and the canvas is the host. D: no legend in the preview.
      for (const id of [1, 2, 4, 7]) {
        t.eq(rowsOf(r, id, "legend").filter((x) => x.preview).length, 0, `panel ${id}: no legend shape`);
        const plot = rowsOf(r, id, "plot")[0];
        t.assert(plot.preview[3] >= plot.loaded[3] - TOL && plot.preview[2] >= plot.loaded[2] - TOL, `panel ${id}: the loaded canvas is inside the preview canvas`);
      }
      for (const id of [1, 2, 7]) t.eq(rowsOf(r, id, "legend").length, 0, `panel ${id}: the loaded legend is hidden`);
      // The time axis and the row names are not drawn: the preview parts are the canvas, the rows box and the legend of the case C only.
      for (const id of [1, 2, 3, 4, 5, 6, 7, 8, 10, 11]) {
        const parts = new Set(r.previews[id].parts.map((p) => p.part));
        for (const forbidden of ["axis-x", "axis-y", "label", "row", "column", "grid"]) t.assert(!parts.has(forbidden), `panel ${id}: no ${forbidden}`);
      }
      // The panel that never draws (9): the loaded body is a message, the preview has no child.
      t.assert(["empty", "error"].includes(state(r, 9)), `panel 9: the loaded body is a message (${state(r, 9)})`);
      t.eq(r.previews[9].children, 0, "panel 9: the preview has no shape");
      t.eq(r.previews[9].notes, "never-drawn", "panel 9: the note names the rule");
    },
  },
  {
    id: "B369",
    skip: SKIP,
    timeout: 300000,
    name: "grid previews: captures of the loading state in the light and the dark theme, and the same agreement at another width (9.11)",
    async run(t) {
      // The agreement at another width: the same fixtures in a viewport of 1000 px.
      const heat = await measure(t, "pv-heat", heatmapDoc, { width: 1000 });
      const hist = await measure(t, "pv-hist", histogramDoc, { width: 1000 });
      const line = await measure(t, "pv-timeline", timelineDoc, { width: 1000 });
      noFailure(t, heat, "heatmap at 1000 px");
      noFailure(t, hist, "histogram at 1000 px");
      noFailure(t, line, "timeline at 1000 px");
      for (const [r, id] of [[heat, 2], [heat, 4], [hist, 1], [hist, 9], [line, 5], [line, 6]]) edges(t, rowsOf(r, id, "plot")[0], `1000 px, panel ${id} plot`);
      // The captures: the loading state of each dashboard in each theme, with the queries held.
      const dir = process.env.HUNE_PREVIEW_SHOTS;
      if (!dir) return;
      mkdirSync(dir, { recursive: true });
      for (const [name, build] of [["heatmap", heatmapDoc], ["histogram", histogramDoc], ["timeline", timelineDoc]]) {
        const d = build(`pv-shot-${name}`);
        await create(t, d);
        for (const theme of ["light", "dark"]) {
          const p = await t.page({ width: 1440, height: 2400 });
          t.allow(p, NEVER_DRAWN);
          await p.emulateColorScheme(theme);
          const hold = p.holdWhen((req) => /\/panels\/[^/]+\/query/.test(req.url));
          await p.goto(pageOf(t, d.uid));
          await p.waitFor(`document.querySelectorAll('.panel__skeleton').length > 0 && [...document.querySelectorAll('.panel__skeleton')].some((e) => getComputedStyle(e).display !== "none" && e.firstElementChild && e.firstElementChild.childElementCount > 0)`, { what: "the previews", timeout: 30000 });
          await sleep(600);
          // One call sets both features: a second call would reset the colour scheme.
          await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }, { name: "prefers-reduced-motion", value: "reduce" }] });
          await sleep(300);
          t.eq(await p.eval(`document.documentElement.dataset.theme`), theme, `${name}: the ${theme} theme applies`);
          const shot = await p.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
          writeFileSync(join(dir, `${name}-${theme}.png`), Buffer.from(shot.data, "base64"));
          hold.release();
        }
      }
    },
  },
];
