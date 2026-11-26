// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B380 to B384 (9.19, 4.26): the chart panels against the specification. The cases B380 to B382 test the legend of the
// time series, the xy chart and the vertical bars. B383 tests the x axis of bars. B384 tests the legend of the horizontal bars.
// All SQL is synthetic (`numbers(...)`).
import { doc, panel, section } from "../lib/docs.mjs";
import { SEED_FROM } from "../lib/env.mjs";
import { axisTexts, waitPanelsReady } from "../lib/h.mjs";

const g = (x, y, w, h) => ({ x, y, w, h });
const panelSel = (id) => `[data-testid="panel-${id}"]`;
const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const hide = (o) => ({ legend: false, tooltip: false, viz: false, ...o });

// Ten points, one a minute from the start of the seed range, with the series a and b.
const SERIES = ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(number) AS a, toFloat64(number * 2 + 1) AS b", "FROM numbers(10)"];
// Five points of the first five minutes only: the data cover a quarter of the seed range of 20 minutes.
const SHORT = ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(number + 1) AS a", "FROM numbers(5)"];
// Twelve series with names of equal length.
const TWELVE = [`SELECT toDateTime(1794211200 + number * 60) AS t, ${Array.from({ length: 12 }, (_, k) => `toFloat64(number + ${k}) AS series_number_${String(k).padStart(2, "0")}`).join(", ")}`, "FROM numbers(10)"];
const BARS = ["SELECT toString(number) AS k, toFloat64(number + 1) AS a, toFloat64(number * 2) AS b", "FROM numbers(4)"];
const XY = ["SELECT toFloat64(number) AS x, toFloat64(number * 2) AS y", "FROM numbers(10)"];
const BARS12 = [`SELECT toString(number) AS k, ${Array.from({ length: 12 }, (_, k) => `toFloat64(number + ${k}) AS series_number_${String(k).padStart(2, "0")}`).join(", ")}`, "FROM numbers(4)"];

async function create(t, document) {
  const r = await t.api.create("transit", document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

/** The texts of the heads of the legend table that show (a head with `display: none` does not count), and the cells of the first row. */
const heads = (p, id) => p.eval(`(() => {
  const shows = (e) => getComputedStyle(e).display !== "none";
  const root = document.querySelector(${JSON.stringify(panelSel(id))});
  const hs = [...root.querySelectorAll("thead th")].filter(shows).map((th) => th.textContent.trim());
  const row = root.querySelector("tbody tr");
  const cells = row ? [...row.children].filter(shows).length : 0;
  return { heads: hs, cells, marks: root.querySelectorAll(".chartCore__axisMark").length };
})()`);

export const cases = [
  {
    id: "B380",
    name: "a table legend with no value column shows the column Name only, in the time series, the xy chart and the vertical and horizontal bars; with every axis hidden no series is on the right (4.26, 9.19)",
    timeout: 180000,
    async run(t) {
      const table = (values) => legend({ mode: "table", values });
      const ts = (id, values, o = {}) => panel(id, "timeseries", `ts ${id}`, SERIES, { grid: o.grid || g(((id - 1) % 2) * 12, Math.floor((id - 1) / 2) * 8, 12, 8), options: { legend: table(values), ...o.options }, fields: o.fields });
      const document = doc("b380-legend-values", "Table legend with no value column", [
        section(null, [
          ts(1, []),
          ts(2, ["max", "last"]),
          panel(3, "barchart", "bars", BARS, { grid: g(0, 16, 12, 8), options: { legend: table([]), show_values: "never" } }),
          panel(4, "xychart", "xy", XY, { grid: g(12, 16, 12, 8), options: { legend: table([]) } }),
          panel(5, "barchart", "h empty", BARS, { grid: g(0, 24, 12, 8), options: { orientation: "horizontal", legend: table([]) } }),
          panel(6, "barchart", "h columns", BARS, { grid: g(12, 24, 12, 8), options: { orientation: "horizontal", legend: table(["max", "last"]) } }),
          // Every y axis hidden: no axis shows, so no series is on the right scale and the table writes no "(right y-axis)".
          ts(7, ["max"], { grid: g(0, 32, 12, 8), fields: { defaults: { axis_placement: "hidden" }, overrides: [] } }),
          ts(8, ["max"], { grid: g(12, 32, 12, 8), fields: { defaults: {}, overrides: [{ match: { by: "name", value: "b" }, set: { unit: "ms" } }] } }),
        ]),
      ], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
      await create(t, document);
      const p = await t.page({ width: 1280, height: 1400 });
      await p.goto(t.dash(document.uid));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 8]);
      for (const id of [1, 3, 4, 5]) {
        const h = await heads(p, id);
        t.eq(h.heads, ["Name"], `panel ${id}: values = [] gives the column Name only`);
        t.eq(h.cells, 1, `panel ${id}: a row has the name cell only`);
      }
      for (const id of [2, 6]) {
        const h = await heads(p, id);
        t.eq(h.heads, ["Name", "Max", "Last"], `panel ${id}: the columns of the document`);
        t.eq(h.cells, 3, `panel ${id}: a row has a cell for each column`);
      }
      // The horizontal table sorts: a click on "Max" sorts up, a second click sorts down, a third gives the series order.
      const order = () => p.eval(`[...document.querySelectorAll('${panelSel(6)} tbody th')].map((e) => e.textContent.trim())`);
      t.eq(await order(), ["a", "b"], "panel 6: the series order");
      const click = (name) => p.eval(`document.querySelector('${panelSel(6)} thead [data-column="${name}"]').click()`);
      await click("max");
      t.eq(await order(), ["a", "b"], "panel 6: sorted up by Max (a: 4, b: 6)");
      await click("max");
      t.eq(await order(), ["b", "a"], "panel 6: sorted down by Max");
      await click("max");
      t.eq(await order(), ["a", "b"], "panel 6: the series order again");
      // Every axis hidden (panel 7): both series on the left scale, no mark. Another unit with an axis that shows (panel 8): a mark.
      const sides = (id) => p.eval(`JSON.parse(document.querySelector('${panelSel(id)} .panel__plot').getAttribute("data-axes")).map((s) => s.side)`);
      t.eq(await sides(7), ["left"], "panel 7: one scale, on the left");
      t.eq((await heads(p, 7)).marks, 0, "panel 7: no series is marked as right");
      t.eq(await sides(8), ["left", "right"], "panel 8: a second unit goes to the right scale");
      t.eq((await heads(p, 8)).marks, 1, "panel 8: the series on the right scale has the mark");
    },
  },
  {
    id: "B381",
    name: "a right legend that every entry leaves keeps no gap: the plot has the whole width (9.19)",
    timeout: 180000,
    async run(t) {
      const right = legend({ placement: "right" });
      const hidden = { defaults: { hide_from: hide({ legend: true }) }, overrides: [] };
      const document = doc("b381-legend-hidden", "Right legend hidden", [
        section(null, [
          panel(1, "timeseries", "ts hidden", SERIES, { grid: g(0, 0, 12, 8), options: { legend: right }, fields: hidden }),
          panel(2, "timeseries", "ts shown", SERIES, { grid: g(12, 0, 12, 8), options: { legend: right } }),
          panel(3, "barchart", "bars hidden", BARS, { grid: g(0, 8, 12, 8), options: { legend: right, show_values: "never" }, fields: hidden }),
          panel(4, "xychart", "xy hidden", XY, { grid: g(12, 8, 12, 8), options: { legend: right }, fields: hidden }),
          panel(5, "timeseries", "ts bottom hidden", SERIES, { grid: g(0, 16, 12, 8), options: { legend: legend() }, fields: hidden }),
        ]),
      ]);
      await create(t, document);
      const p = await t.page({ width: 1280, height: 1400 });
      await p.goto(t.dash(document.uid));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3, 4, 5]);
      // The width of the host and the width of the canvas of the engine.
      const widths = (id) => p.eval(`(() => {
        const host = document.querySelector('${panelSel(id)} .panel__plot');
        const plot = host.querySelector(".chartCore__plot");
        return { host: host.clientWidth, plot: plot.getBoundingClientRect().width, placement: host.dataset.legendPlacement, legend: !!host.querySelector(".chartCore__legend:not([hidden])") };
      })()`);
      for (const id of [1, 3, 4]) {
        const w = await widths(id);
        t.eq(w.legend, false, `panel ${id}: the engine hides the legend`);
        t.assert(Math.abs(w.plot - w.host) <= 0.5, `panel ${id}: the plot has the width of the host (${w.plot} of ${w.host})`);
      }
      const bottom = await widths(5);
      t.assert(Math.abs(bottom.plot - bottom.host) <= 0.5, `panel 5: the same width with the placement bottom (${bottom.plot} of ${bottom.host})`);
      const shown = await widths(2);
      t.assert(shown.legend && shown.plot < shown.host - 8, `panel 2: a legend that shows takes its column (${shown.plot} of ${shown.host})`);
    },
  },
  {
    id: "B382",
    name: "a right legend that falls back under the plot keeps the cap of 35 % of the height and scrolls (9.19)",
    timeout: 180000,
    async run(t) {
      const right = (mode) => legend({ placement: "right", mode, values: mode === "table" ? ["max", "last"] : [] });
      const document = doc("b382-legend-fallback", "Legend fallback cap", [
        section(null, [
          panel(1, "timeseries", "list", TWELVE, { grid: g(0, 0, 5, 8), options: { legend: right("list") } }),
          panel(2, "timeseries", "table", TWELVE, { grid: g(5, 0, 5, 8), options: { legend: right("table") } }),
          panel(3, "barchart", "bars list", BARS12, { grid: g(10, 0, 5, 8), options: { legend: right("list"), show_values: "never" } }),
        ]),
      ]);
      await create(t, document);
      const p = await t.page({ width: 1280, height: 1000 });
      await p.goto(t.dash(document.uid));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3]);
      const read = (id) => p.eval(`(() => {
        const host = document.querySelector('${panelSel(id)} .panel__plot');
        const legend = host.querySelector(".chartCore__legend");
        const scroller = host.querySelector(".chartCore__legendTableWrap") || host.querySelector(".chartCore__legendList");
        return { fallback: host.hasAttribute("data-legend-fallback"), height: host.clientHeight, legend: legend.offsetHeight, scroller: scroller.clientHeight, scrolls: scroller.scrollHeight > scroller.clientHeight + 1, cap: Math.floor(host.clientHeight * 0.35), top: legend.getBoundingClientRect().top, plotBottom: host.querySelector(".chartCore__plot").getBoundingClientRect().bottom };
      })()`);
      for (const id of [1, 2, 3]) {
        const r = await read(id);
        t.assert(r.fallback, `panel ${id}: the legend is under the plot (the host is too narrow)`);
        t.assert(r.top >= r.plotBottom - 1, `panel ${id}: the legend starts under the plot`);
        t.assert(r.scrolls, `panel ${id}: twelve entries do not fit in the cap, so the legend scrolls`);
        t.assert(r.scroller <= r.cap + 1, `panel ${id}: the scroll area is at most 35 % of ${r.height} px (${r.scroller} of ${r.cap})`);
        t.assert(r.legend <= r.cap + 12 + 1, `panel ${id}: the legend is at most the cap and its padding (${r.legend})`);
      }
    },
  },
  {
    id: "B383",
    name: "the x axis of a time series covers the requested range, and the x axis of bars keeps the span of the data (9.19)",
    timeout: 180000,
    async run(t) {
      const document = doc("b383-bars-span", "X axis of bars", [
        section(null, [
          panel(1, "timeseries", "line", SHORT, { grid: g(0, 0, 24, 8), options: { draw: "line", legend: legend({ show: false }) } }),
          panel(2, "timeseries", "bars", SHORT, { grid: g(0, 8, 24, 8), options: { draw: "bars", fill_opacity: 80, legend: legend({ show: false }) } }),
        ]),
      ], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
      await create(t, document);
      const p = await t.page({ width: 1280, height: 1000 });
      await p.recordCanvasText();
      await p.goto(t.dash(document.uid));
      await p.idle();
      await waitPanelsReady(p, [1, 2]);
      // The labels are clocks in UTC. The minutes after the start of the seed range give the span of each axis.
      const start = new Date(SEED_FROM);
      const minutes = (label) => {
        const m = /^(\d\d):(\d\d)/.exec(label);
        t.assert(m, `an x label is a clock: ${label}`);
        return Number(m[1]) * 60 + Number(m[2]) - (start.getUTCHours() * 60 + start.getUTCMinutes());
      };
      const line = (await axisTexts(p, 1)).x.map(minutes);
      const bars = (await axisTexts(p, 2)).x.map(minutes);
      t.assert(line.length >= 3 && bars.length >= 2, `labels: ${line} / ${bars}`);
      t.assert(Math.max(...line) >= 15, `the line covers the requested range of 20 minutes (last label at ${Math.max(...line)} min)`);
      t.assert(Math.max(...bars) <= 5, `the bars keep the span of the data of 4 minutes (last label at ${Math.max(...bars)} min)`);
    },
  },
  {
    id: "B384",
    name: "the legend of horizontal bars follows the shared object: mode, values, sort, hide_from.legend, the cap and the narrow fallback (9.19)",
    timeout: 180000,
    async run(t) {
      const h = { orientation: "horizontal", show_values: "never" };
      const set = (name, s) => ({ match: { by: "name", value: name }, set: s });
      const document = doc("b384-hbar-legend", "Legend of horizontal bars", [
        section(null, [
          // A series with hide_from.legend has no entry, and its bars stay.
          panel(1, "barchart", "entry hidden", BARS, { grid: g(0, 0, 12, 8), options: { ...h, legend: legend() }, fields: { defaults: {}, overrides: [set("b", { hide_from: hide({ legend: true }) })] } }),
          // Every entry hidden: no legend.
          panel(2, "barchart", "all hidden", BARS, { grid: g(12, 0, 12, 8), options: { ...h, legend: legend() }, fields: { defaults: { hide_from: hide({ legend: true }) }, overrides: [] } }),
          // A table with a sort by name, down.
          panel(3, "barchart", "sorted table", BARS, { grid: g(0, 8, 12, 8), options: { ...h, legend: legend({ mode: "table", values: ["max"], sort_by: "name", sort_desc: true }) } }),
          // Twelve entries at the bottom: the legend keeps 35 % of the height and scrolls.
          panel(4, "barchart", "twelve", BARS12, { grid: g(12, 8, 6, 8), options: { ...h, legend: legend() } }),
          // A narrow host with a legend at the right: the legend goes under the plot.
          panel(5, "barchart", "narrow right", BARS12, { grid: g(0, 16, 5, 8), options: { ...h, legend: legend({ placement: "right" }) } }),
          // A wide host with a legend at the right stays beside the plot.
          panel(6, "barchart", "wide right", BARS, { grid: g(8, 16, 16, 8), options: { ...h, legend: legend({ placement: "right" }) } }),
        ]),
      ]);
      await create(t, document);
      const p = await t.page({ width: 1280, height: 1400 });
      await p.goto(t.dash(document.uid));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6]);
      const names = (id) => p.eval(`[...document.querySelectorAll('${panelSel(id)} .hbar__legendItem')].map((e) => e.textContent.trim())`);
      t.eq(await names(1), ["a"], "panel 1: the entry of b is out");
      t.eq(await p.eval(`document.querySelectorAll('${panelSel(1)} .hbar__row .hbar__bar').length`), 8, "panel 1: the bars of both series stay");
      t.eq(await p.eval(`document.querySelectorAll('${panelSel(2)} .hbar__legend').length`), 0, "panel 2: no legend");
      t.eq(await p.eval(`[...document.querySelectorAll('${panelSel(3)} thead th')].map((e) => e.textContent.trim())`), ["Name", "Max"], "panel 3: the table has the columns of the document");
      t.eq(await names(3), ["b", "a"], "panel 3: sorted down by name");
      t.eq(await p.eval(`document.querySelector('${panelSel(3)} thead th[aria-sort="descending"]').textContent.trim()`), "Name", "panel 3: the head of the sort has aria-sort");
      const geometry = (id) => p.eval(`(() => {
        const root = document.querySelector('${panelSel(id)} .hbar');
        const legend = root.querySelector(".hbar__legend");
        const plot = root.querySelector(".hbar__plot");
        return { fallback: root.hasAttribute("data-legend-fallback"), height: root.clientHeight, legend: legend.offsetHeight, scrolls: legend.scrollHeight > legend.clientHeight + 1, cap: Math.floor(root.clientHeight * 0.35), legendTop: legend.getBoundingClientRect().top, legendLeft: legend.getBoundingClientRect().left, plotBottom: plot.getBoundingClientRect().bottom, plotRight: plot.getBoundingClientRect().right };
      })()`);
      const twelve = await geometry(4);
      t.assert(twelve.scrolls, "panel 4: twelve entries scroll");
      t.assert(twelve.legend <= twelve.cap + 12 + 1, `panel 4: the legend is at most the cap and its padding (${twelve.legend}, cap ${twelve.cap})`);
      const narrow = await geometry(5);
      t.assert(narrow.fallback && narrow.legendTop >= narrow.plotBottom - 1, `panel 5: the narrow host puts the legend under the plot (${JSON.stringify(narrow)})`);
      t.assert(narrow.legend <= narrow.cap + 12 + 1, `panel 5: the fallback keeps the cap (${narrow.legend}, cap ${narrow.cap})`);
      const wide = await geometry(6);
      t.assert(!wide.fallback && wide.legendLeft >= wide.plotRight - 1, `panel 6: the wide host keeps the legend beside the plot (${JSON.stringify(wide)})`);
    },
  },
];
