// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B55 (16.10, 9.19a): no scroll bar for a size error at a browser zoom that is not 100 %.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { enterEdit, openPanelEditor, moreItem } from "../lib/ed.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

// The panels of the case: six stat panels in one row (a usual layout of a report), a stat with three
// values, a bar gauge, a pie chart, a bar chart, a time series and a table.
function zoomDoc() {
  const stats = [
    ["Active Requests", 17], ["Active Users", 16], ["New Requests", 0],
    ["Closed Requests", 0], ["Error Retry Rate", 0.25], ["Backlog Age Peak", 29.7],
  ].map(([title, v], i) => panel(10 + i, "stat", title, `SELECT ${v} AS value`, { grid: { x: i * 4, y: 0, w: 4, h: 4 } }));
  return doc("zoom55", "Zoom 55", [
    section("Panels", [
      ...stats,
      panel(20, "stat", "Three values", "SELECT 17 AS a, 16 AS b, 0 AS c", { grid: { x: 0, y: 4, w: 6, h: 5 } }),
      panel(21, "bargauge", "Bars", SQL.bars, { grid: { x: 6, y: 4, w: 6, h: 5 } }),
      panel(22, "piechart", "Pie", SQL.bars, { grid: { x: 12, y: 4, w: 6, h: 5 } }),
      panel(23, "barchart", "Bar chart", SQL.bars, { grid: { x: 18, y: 4, w: 6, h: 5 } }),
      panel(24, "timeseries", "Series", SQL.series, { grid: { x: 0, y: 9, w: 12, h: 6 } }),
      panel(25, "table", "Table", SQL.table, { grid: { x: 12, y: 9, w: 12, h: 6 } }),
    ]),
  ]);
}
const IDS = [10, 11, 12, 13, 14, 15, 20, 21, 22, 23, 24, 25];

// The detector runs in the page. For each box with overflow auto or scroll it reads the real (fractional) box and
// the rects of its children. It returns the problems:
// - a child larger than the box by less than 1 px (a size error) in any panel;
// - any excess in a panel type that never scrolls, except a legend or an error text that is really longer (1 px or more);
// - a scroll bar with no excess (except a reserved gutter: scrollbar-gutter stable);
// - a horizontal scroll of the page.
const DETECT = `(() => {
  const problems = [];
  const scrolls = new Set(["table", "text"]);
  const de = document.documentElement;
  if (de.scrollWidth > de.clientWidth) problems.push("page: horizontal scroll " + (de.scrollWidth - de.clientWidth) + " px");
  for (const p of document.querySelectorAll(".panel")) {
    const type = p.dataset.type || "";
    const name = (p.dataset.testid || "panel") + "(" + type + ")";
    for (const el of [p, ...p.querySelectorAll("*")]) {
      const cs = getComputedStyle(el);
      const sx = cs.overflowX === "auto" || cs.overflowX === "scroll";
      const sy = cs.overflowY === "auto" || cs.overflowY === "scroll";
      if (!sx && !sy) continue;
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height) continue;
      const f = (k) => parseFloat(cs[k]) || 0;
      const barY = el.offsetWidth - el.clientWidth - Math.round(f("borderLeftWidth") + f("borderRightWidth"));
      const barX = el.offsetHeight - el.clientHeight - Math.round(f("borderTopWidth") + f("borderBottomWidth"));
      const right = r.right - f("borderRightWidth") - (barY >= 2 ? barY : 0);
      const bottom = r.bottom - f("borderBottomWidth") - (barX >= 2 ? barX : 0);
      let fx = 0, fy = 0;
      for (const c of el.children) {
        const cr = c.getBoundingClientRect();
        if (!cr.width && !cr.height) continue;
        fx = Math.max(fx, cr.right - right);
        fy = Math.max(fy, cr.bottom - bottom);
      }
      fx = Math.max(fx, el.scrollWidth - el.clientWidth);
      fy = Math.max(fy, el.scrollHeight - el.clientHeight);
      const what = name + " ." + String(el.className).split(" ")[0];
      const longText = /legend|error-text/.test(String(el.className));
      if ((fx > 0.01 && fx < 1) || (fy > 0.01 && fy < 1)) problems.push(what + ": size error " + fx.toFixed(3) + "/" + fy.toFixed(3) + " px");
      else if (!scrolls.has(type) && !longText && (fx > 0.01 || fy > 0.01)) problems.push(what + ": excess " + fx.toFixed(3) + "/" + fy.toFixed(3) + " px in a panel that does not scroll");
      if ((barX >= 2 || barY >= 2) && fx <= 0.01 && fy <= 0.01 && cs.scrollbarGutter !== "stable" && !cs.scrollbarGutter.startsWith("stable")) problems.push(what + ": scroll bar with no excess");
    }
  }
  return problems;
})()`;

const PAGE_DETECT = `(() => {
  const de = document.documentElement;
  return de.scrollWidth > de.clientWidth ? ["page: horizontal scroll " + (de.scrollWidth - de.clientWidth) + " px"] : [];
})()`;

async function zoomPage(t, width, dsf, theme) {
  const cssW = Math.round(width / dsf);
  const cssH = Math.round(1000 / dsf);
  const p = await t.page({ width: cssW, height: cssH });
  await p.send("Emulation.setDeviceMetricsOverride", { width: cssW, height: cssH, deviceScaleFactor: dsf, mobile: false });
  await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
  return p;
}

export const cases = [
  {
    id: "B55",
    name: "no scroll bar for a size error at the zoom levels 75, 90, 110 and 125 % (9.19a); no horizontal page scroll on the list, the editor and a dialog",
    async run(t) {
      const r = await t.api.create("transit", zoomDoc());
      t.assert(r.status === 201, `create zoom55: ${r.status} ${r.text.slice(0, 300)}`);
      const found = [];
      for (const width of [1920, 1280]) {
        for (const dsf of [0.75, 0.9, 1.1, 1.25]) {
          const theme = dsf === 0.9 ? "light" : "dark";
          const p = await zoomPage(t, width, dsf, theme);
          await p.goto(t.dash("zoom55"));
          await p.idle();
          await waitPanelsReady(p, IDS);
          // Two frames after the last fit of the panels.
          await p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");
          for (const x of await p.eval(DETECT)) found.push(`${width}@${dsf} dashboard: ${x}`);
          if (dsf === 0.9 || dsf === 1.25) {
            await p.goto(t.url(""));
            await p.idle();
            for (const x of await p.eval(PAGE_DETECT)) found.push(`${width}@${dsf} list: ${x}`);
            await p.goto(t.dash("zoom55"));
            await p.idle();
            await waitPanelsReady(p, IDS);
            await enterEdit(p);
            for (const x of await p.eval(PAGE_DETECT)) found.push(`${width}@${dsf} edit mode: ${x}`);
            await openPanelEditor(p, 10);
            for (const x of await p.eval(PAGE_DETECT)) found.push(`${width}@${dsf} panel editor: ${x}`);
            await p.goto(t.dash("zoom55"));
            await p.idle();
            await moreItem(p, "history-button");
            await p.waitSel(p.tid("history-list"));
            for (const x of await p.eval(PAGE_DETECT)) found.push(`${width}@${dsf} history dialog: ${x}`);
          }
          await p.close();
        }
      }
      t.eq(found, [], "stray scroll");
    },
  },
];
