// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B280 to B282 (9.11): the preview of a panel that loads for the first time follows the real panel.
// B280: a time series previews LINES (no dots), the axes and the legend of its options, and the plot of the preview has the place of the plot of the
//       loaded chart (the legend at the bottom as a list or a table, at the right, off, and a hidden y axis).
// B281: a bar chart (vertical and horizontal) and a pie chart (the legend at the bottom and at the right) keep the same place for their plot.
// B282: each queried panel type has a preview that follows its options; the light that sweeps moves with a transform only, and the reduced motion
//       stops it; the preview is visible in both themes; a reload of a loaded panel shows no preview.
// The queries of the panels are held, so that the preview stays on the screen. A held request needs the interception of Chrome.
import { readFileSync } from "node:fs";
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { sleep } from "../lib/h.mjs";

// The options and the fields of the types that the helper `panel` does not know (a gauge, a heatmap, a histogram, a state timeline) come from the
// demo dashboard of latency, which the validator of the server accepts.
const DEMO = JSON.parse(readFileSync(new URL("../../../examples/demo/dashboards/operations/latency.json", import.meta.url), "utf8"));
const demoPanel = (type) => DEMO.sections.flatMap((s) => s.panels).find((p) => p.type === type);
function typed(id, type, title, sql, grid, over = {}) {
  const src = demoPanel(type);
  const base = panel(id, "table", title, sql, { grid, format: type === "state_timeline" || type === "heatmap" ? "table" : "table" });
  return { ...base, type, fields: src.fields, options: { ...src.options, ...(over.options || {}) } };
}

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const time = { from: "now-6h", to: "now", timezone: "utc", refresh: null };
const g = (i, w = 12, h = 8) => ({ x: (i % 2) * 12, y: Math.floor(i / 2) * h, w, h });
const legend = (show, placement = "bottom", mode = "list", values = []) => ({ show, placement, mode, values });
const reduce = (over = {}) => ({ calc: "last_not_null", fields: null, values: false, ...over });
const HOLD = (req) => /\/panels\/[^/]+\/query/.test(req.url);

// The rectangles are relative to the box of the chart: the origin is the top left corner of the preview (or of the plot container).
const MEASURE_PREVIEW = (id) => `(() => {
  const p = document.querySelector('[data-testid="panel-${id}"]');
  const sk = p.querySelector(".panel__skeleton");
  if (!sk || getComputedStyle(sk).display === "none") return null;
  const o = sk.getBoundingClientRect();
  const rel = (e) => { if (!e) return null; const b = e.getBoundingClientRect(); return [b.x - o.x, b.y - o.y, b.width, b.height]; };
  const plot = sk.querySelector(".skel__vgrids") || sk.querySelector(".skel__area") || sk.querySelector(".skel__chart--pie");
  const lg = sk.querySelector(".skel__legend");
  const root = sk.firstElementChild;
  return {
    kind: sk.dataset.kind, place: root.dataset.legend, mode: root.dataset.mode || null,
    plot: rel(plot), legend: rel(lg),
    lines: sk.querySelectorAll("path.skel__line").length, fills: sk.querySelectorAll("path.skel__fill").length,
    dots: [...sk.querySelectorAll("*")].filter((e) => /dot/.test(String(e.getAttribute("class") || "")) && !/skel__sw--dot/.test(String(e.getAttribute("class")))).length,
    items: sk.querySelectorAll(".skel__legend .skel__item, .skel__legend .skel__lrow").length,
    ylabs: sk.querySelectorAll(".skel__ylab").length, xlabs: sk.querySelectorAll(".skel__xlab").length,
    hidden: sk.getAttribute("aria-hidden"),
  };
})()`;

const MEASURE_REAL = (id) => `(() => {
  const p = document.querySelector('[data-testid="panel-${id}"]');
  const box = p.querySelector(".panel__plot");
  const o = box.getBoundingClientRect();
  const rel = (e) => { if (!e) return null; const b = e.getBoundingClientRect(); return [b.x - o.x, b.y - o.y, b.width, b.height]; };
  const core = p.querySelector(".chartCore");
  if (core && core.dataset.plot) {
    const [l, t, w, h] = core.dataset.plot.split(" ").map(Number);
    const lg = core.querySelector(".chartCore__legend");
    return { plot: [l, t, w, h], legend: lg && !lg.hidden && lg.offsetHeight ? rel(lg) : null };
  }
  const grid = p.querySelector(".hbar__grid");
  if (grid) return { plot: rel(grid), legend: null };
  const pie = p.querySelector(".pie__plot");
  if (pie) { const lg = p.querySelector(".pie__legend"); return { plot: rel(pie), legend: lg ? rel(lg) : null }; }
  return null;
})()`;

// The labels of the axes depend on the data: the queries give values of three digits and names of the length of a service name, which are the typical
// ones that the preview estimates.
const SERIES = ["SELECT $__timeInterval(event_time) AS t, node, toFloat64(sum(weight)) * 25 AS weight", "FROM analytics.seed", "WHERE $__timeFilter(event_time)", "GROUP BY t, node", "ORDER BY t"];
const BARS = ["SELECT concat('checkout-', node) AS node, sum(blocks) * 4 AS blocks", "FROM analytics.seed", "GROUP BY node", "ORDER BY node"];
// The names of a horizontal bar chart set the width of its label column: eight characters, as "checkout" in the demo.
const BARS_H = ["SELECT concat('checko', node) AS node, sum(blocks) * 4 AS blocks", "FROM analytics.seed", "GROUP BY node", "ORDER BY node"];

const readyAll = (ids) => `[${ids.join(",")}].every((id) => { const p = document.querySelector('[data-testid="panel-' + id + '"]'); return p && p.dataset.state === "ready"; })`;

// Opens the dashboard with the queries held. Gives the page, the release of the queries and the measures of the previews.
async function openHeld(t, uid, d, ids) {
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
  const p = await t.page({ width: 1440, height: 2400 });
  const hold = p.holdWhen(HOLD);
  await p.goto(t.dash(uid));
  await p.waitFor(`${JSON.stringify(ids)}.every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"] .panel__skeleton'); return e && getComputedStyle(e).display !== "none"; })`, { what: "the previews", timeout: 20000 });
  await sleep(500);
  const previews = {};
  for (const id of ids) previews[id] = await p.eval(MEASURE_PREVIEW(id));
  return { p, hold, previews };
}

async function realRects(t, p, hold, ids) {
  hold.release();
  await p.waitFor(readyAll(ids), { what: "the charts", timeout: 30000 });
  await sleep(700);
  const out = {};
  for (const id of ids) out[id] = await p.eval(MEASURE_REAL(id));
  return out;
}

const near = (t, a, b, tol, what) => {
  t.assert(a && b, `${what}: a rectangle is missing ${JSON.stringify([a, b])}`);
  for (let i = 0; i < 4; i++) t.assert(Math.abs(a[i] - b[i]) <= tol, `${what}: the preview ${JSON.stringify(a.map((n) => Math.round(n * 10) / 10))} and the chart ${JSON.stringify(b.map((n) => Math.round(n * 10) / 10))} differ by more than ${tol} px`);
};

const series = (id, title, i, options, fields = { defaults: { unit: "none", decimals: 0 }, overrides: [] }) =>
  panel(id, "timeseries", title, SERIES, { grid: g(i), options: { draw: "line", legend: legend(true), ...options }, fields });

export const cases = [
  {
    id: "B280",
    skip: SKIP,
    name: "the preview of a time series is made of lines and keeps the place of the axes and of the legend of the loaded chart (9.11)",
    async run(t) {
      const uid = "pv280";
      const d = doc(uid, "Preview of series", [
        section(null, [
          series(1, "bottom list", 0, { legend: legend(true, "bottom", "list") }),
          series(2, "right list", 1, { legend: legend(true, "right", "list") }),
          series(3, "bottom table", 2, { legend: legend(true, "bottom", "table", ["mean", "max"]) }),
          series(4, "no legend", 3, { legend: legend(false) }),
          series(5, "hidden y axis", 4, { legend: legend(true) }, { defaults: { unit: "none", decimals: 0, axis_placement: "hidden" }, overrides: [] }),
          series(6, "right table", 5, { legend: legend(true, "right", "table", ["mean", "max"]) }),
        ]),
      ], { time });
      const ids = [1, 2, 3, 4, 5, 6];
      const { p, hold, previews } = await openHeld(t, uid, d, ids);
      for (const id of ids) {
        const v = previews[id];
        t.assert(v, `panel ${id} shows a preview`);
        t.eq(v.kind, "series", `panel ${id} kind`);
        t.eq(v.lines, 3, `panel ${id}: three lines, not dots`);
        t.eq(v.fills, 1, `panel ${id}: one area under the first line`);
        t.eq(v.dots, 0, `panel ${id}: no dot`);
        t.eq(v.hidden, "true", `panel ${id}: the preview is hidden from assistive technology`);
        t.eq(v.xlabs, 6, `panel ${id}: the labels of the x axis`);
        t.eq(v.ylabs, id === 5 ? 0 : 5, `panel ${id}: the labels of the y axis (none when the axis is hidden)`);
      }
      t.eq([previews[1].place, previews[2].place, previews[3].place, previews[4].place], ["bottom", "right", "bottom", "none"], "the place of the legend follows the options");
      t.eq(previews[3].mode, "table", "a legend in table mode");
      t.eq(previews[1].items, 4, "a list of four items");
      t.eq(previews[3].items, 3, "a head and two rows");
      t.eq(previews[4].legend, null, "no legend: nothing is reserved");
      const real = await realRects(t, p, hold, ids);
      // The plot of the preview has the place of the plot of the chart: 4 px at most in each of left, top, width and height (the left gutter holds the
      // labels, which the preview estimates from the unit; the rest comes from the options).
      for (const id of ids) {
        if (id === 2 || id === 6) near(t, [previews[id].plot[0], previews[id].plot[1], 0, previews[id].plot[3]], [real[id].plot[0], real[id].plot[1], 0, real[id].plot[3]], 4, `panel ${id} plot (left, top and height)`);
        else near(t, previews[id].plot, real[id].plot, 4, `panel ${id} plot`);
      }
      for (const id of [1, 2, 3]) near(t, [0, previews[id].legend[1], 0, previews[id].legend[3]], [0, real[id].legend[1], 0, real[id].legend[3]], 4, `panel ${id} legend (top and height)`);
      // The legend at the right takes its width from the plot. The width of a legend comes from the names of the series, which the preview cannot know:
      // it reserves the width of a typical legend (141 px for a list). The plot keeps its top and its height, and its width follows the names.
      const edge = (r) => r.plot[0] + r.plot[2];
      t.assert(Math.abs(edge(previews[2]) - edge(real[2])) <= 30, `the right edge of the plot with a legend at the right: ${edge(previews[2])} and ${edge(real[2])}`);
      t.assert(Math.abs(edge(previews[6]) - edge(real[6])) <= 30, `the right edge of the plot with a legend as a table at the right: ${edge(previews[6])} and ${edge(real[6])}`);
    },
  },
  {
    id: "B281",
    skip: SKIP,
    name: "the preview of a bar chart and of a pie chart keeps the place of the plot and of the legend of the loaded chart (9.11)",
    async run(t) {
      const uid = "pv281";
      const bars = (id, title, i, options) => panel(id, "barchart", title, options.orientation === "horizontal" ? BARS_H : BARS, { grid: g(i), options: { orientation: "vertical", stacking: "none", legend: legend(false), ...options }, fields: { defaults: { unit: "none", decimals: 0 }, overrides: [] } });
      const pie = (id, title, i, lg) => panel(id, "piechart", title, BARS, { grid: g(i), options: { pie_type: "donut", reduce: reduce({ values: true }), labels: [], legend: lg }, fields: { defaults: { unit: "none", decimals: 0 }, overrides: [] } });
      const d = doc(uid, "Preview of bars and pies", [
        section(null, [
          bars(1, "vertical, legend at the bottom", 0, { legend: legend(true, "bottom", "list") }),
          bars(2, "horizontal", 1, { orientation: "horizontal", show_values: "always" }),
          pie(3, "pie, legend at the bottom", 2, legend(true, "bottom", "list")),
          pie(4, "pie, legend at the right", 3, legend(true, "right", "list")),
          pie(5, "pie, no legend", 4, legend(false)),
          pie(6, "pie, table at the right", 5, legend(true, "right", "table", ["value", "percent"])),
        ]),
      ], { time });
      const ids = [1, 2, 3, 4, 5, 6];
      const { p, hold, previews } = await openHeld(t, uid, d, ids);
      t.eq([previews[1].kind, previews[2].kind, previews[3].kind], ["bars", "bars", "pie"], "the kinds");
      t.eq(previews[2].place, "none", "a bar chart without a legend reserves none");
      t.eq(previews[3].place, "bottom", "the legend of the pie");
      t.eq(previews[4].place, "right", "the legend of the pie at the right");
      const real = await realRects(t, p, hold, ids);
      near(t, previews[1].plot, real[1].plot, 4, "the vertical bar chart plot");
      near(t, previews[2].plot, real[2].plot, 4, "the horizontal bar chart plot (the label column and the value axis)");
      for (const id of [3, 5]) near(t, previews[id].plot, real[id].plot, 4, `the pie ${id} plot`);
      // A legend at the right has the width of its names, which the preview cannot know: the plot keeps its top, its left and its height, and its width
      // differs by the difference of the width of a typical legend and of this one (30 px at most).
      for (const id of [4, 6]) {
        const a = previews[id].plot;
        const b = real[id].plot;
        near(t, [a[0], a[1], 0, a[3]], [b[0], b[1], 0, b[3]], 4, `the pie ${id} plot (left, top and height)`);
        t.assert(Math.abs(a[2] - b[2]) <= 30, `the pie ${id} plot width: ${a[2]} and ${b[2]}`);
      }
      near(t, [0, previews[3].legend[1], 0, previews[3].legend[3]], [0, real[3].legend[1], 0, real[3].legend[3]], 4, "the legend of the pie at the bottom (top and height)");
    },
  },
  {
    id: "B282",
    skip: SKIP,
    name: "each queried panel type has a preview that follows its options, the light moves with a transform and stops with the reduced motion, and a reload shows none (9.11)",
    async run(t) {
      const uid = "pv282";
      const f = { defaults: { unit: "none", decimals: 0 }, overrides: [] };
      const d = doc(uid, "Preview of types", [
        section(null, [
          panel(1, "stat", "stat", SQL.total, { grid: g(0, 12, 4), options: { graph_mode: "area" } }),
          typed(2, "gauge", "gauge", SQL.bars, g(1, 12, 4), { options: { reduce: reduce({ values: true }) } }),
          panel(3, "bargauge", "bar gauge", SQL.bars, { grid: g(2, 12, 4), options: { reduce: reduce({ values: true }), orientation: "vertical" } }),
          panel(4, "table", "table", SQL.table, { grid: g(3, 12, 6) }),
          typed(5, "heatmap", "heatmap", SQL.series, g(4, 12, 8), { options: { show_legend: true } }),
          typed(6, "histogram", "histogram", SQL.bars, g(5, 12, 8), { options: { legend: legend(true) } }),
          typed(7, "state_timeline", "timeline", SQL.series, g(6, 12, 8)),
          panel(8, "timeseries", "bars", SQL.series, { grid: g(7, 12, 8), options: { draw: "bars", stacking: "normal", legend: legend(false) }, fields: f }),
        ]),
      ], { time });
      const ids = [1, 2, 3, 4, 5, 6, 7, 8];
      const { p, hold, previews } = await openHeld(t, uid, d, ids);
      t.eq(ids.map((id) => previews[id].kind), ["stat", "gauge", "bar-gauge", "table", "heatmap", "histogram", "timeline", "series"], "a preview of each type");
      const shape = (id, sel) => p.count(`[data-testid="panel-${id}"] .panel__skeleton ${sel}`);
      t.eq(await shape(1, ".skel__spark"), 1, "a stat with a graph shows a line");
      t.eq(await shape(2, ".skel__gauge"), 3, "a gauge for each value");
      t.eq(await shape(3, ".skel__bg"), 4, "a bar gauge for each value");
      t.eq(await p.attr('[data-testid="panel-3"] .panel__skeleton .skel', "data-orient"), "vertical", "the orientation of the bar gauge");
      t.assert((await shape(4, ".skel__th")) >= 3, "a table has a head");
      t.eq(await shape(5, ".skel__scale"), 1, "a heatmap with its legend on shows a color scale");
      t.eq(previews[7].place, "none", "a timeline reserves no legend");
      t.eq(await shape(8, ".skel__bar--stack"), 12, "a bar series that stacks");
      t.eq(previews[8].lines, 0, "bars, no line");
      // The light: one band that moves with a transform; with the reduced motion it is gone. The shapes show in both themes.
      const light = await p.eval(`(() => { const a = getComputedStyle(document.querySelector('[data-testid="panel-1"] .panel__skeleton'), "::after"); return { name: a.animationName, display: a.display }; })()`);
      t.eq(light, { name: "panel-skel", display: "block" }, "the light sweeps");
      const bg = (theme) => p.eval(`(async () => { document.documentElement.setAttribute("data-theme", ${JSON.stringify(theme)}); await new Promise((r) => setTimeout(r, 100)); const e = document.querySelector('[data-testid="panel-4"] .skel__cell'); return getComputedStyle(e).backgroundColor; })()`);
      for (const theme of ["light", "dark"]) {
        const color = await bg(theme);
        const alpha = Number((/\/ ([0-9.]+)\)$/.exec(color) || /, ([0-9.]+)\)$/.exec(color) || [0, "1"])[1]);
        t.assert(alpha > 0.03, `a shape has a visible color in the ${theme} theme: ${color}`);
      }
      await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
      await sleep(200);
      const still = await p.eval(`(() => { const a = getComputedStyle(document.querySelector('[data-testid="panel-1"] .panel__skeleton'), "::after"); return { name: a.animationName, display: a.display }; })()`);
      t.eq(still, { name: "none", display: "none" }, "the reduced motion stops the light");
      await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
      // After the load, a reload keeps the content and shows no preview (the dots only).
      hold.release();
      await p.waitFor(readyAll(ids), { what: "the panels", timeout: 30000 });
      const again = p.holdWhen(HOLD);
      await p.click('[data-testid="refresh-now"]');
      await sleep(900);
      const reload = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-8"]'); return { wait: e.dataset.wait || null, shows: getComputedStyle(e.querySelector(".panel__skeleton")).display }; })()`);
      t.eq(reload, { wait: "reload", shows: "none" }, "a reload shows no preview");
      again.release();
    },
  },
];
