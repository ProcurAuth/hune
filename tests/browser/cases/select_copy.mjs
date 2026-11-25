// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B260 to B263 (9.8e): a person can select and copy the text of a panel, with real mouse drags.
// B260: the legend items of a chart. A drag selects the text and toggles nothing. A plain click still toggles. A double click selects a word, a triple
//       click selects the text, and the key Ctrl+C on a focused item copies its text.
// B261: the links of a table. A drag over the text of a link selects it and does not follow the link. A plain click follows the link.
// B262: the text that a chart draws on its canvas. Each chart that draws text has a layer of transparent text over the canvas. A drag over the labels
//       of an axis selects them, a copy gives one label on each line, and the pointer over the plot still shows the tooltip.
// B263: the sortable headers of a table. A drag over a header selects its text and does not sort. A plain click sorts. A drag over cells selects text.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const g = (x, y, w, h) => ({ x, y, w, h });

// A dashboard with a table that has a link in the column node, a time series with a legend, a bar chart and a pie chart.
function selectDoc(uid) {
  const linkField = { defaults: {}, overrides: [{ match: { by: "name", value: "node" }, set: { links: [{ title: "The same node", url: "?var-selected=${__value.raw}", target_blank: false }] } }] };
  return doc(uid, `Select ${uid}`, [
    section(null, [
      panel(1, "table", "Table", SQL.table, { grid: g(0, 0, 12, 8), fields: linkField }),
      panel(2, "timeseries", "Lines", SQL.series, { grid: g(12, 0, 12, 8), options: { draw: "line" } }),
      panel(3, "barchart", "Bar chart", SQL.bars, { grid: g(0, 8, 8, 8) }),
      panel(4, "piechart", "Pie chart", SQL.bars, { grid: g(8, 8, 8, 8), options: { labels: ["name", "percent"] } }),
    ]),
  ]);
}

const mouse = (p, type, x, y, extra = {}) => p.send("Input.dispatchMouseEvent", { type, x, y, button: "left", ...extra });

// The box of the first element that matches, after it scrolled into view.
async function boxOf(p, selector, nth = 0) {
  return p.eval(`(() => {
    const e = document.querySelectorAll(${JSON.stringify(selector)})[${nth}];
    if (!e) return null;
    e.scrollIntoView({ block: "center", inline: "nearest" });
    const b = e.getBoundingClientRect();
    return { x0: b.x + 1, x1: b.x + b.width - 1, y0: b.y + 1, y1: b.y + b.height - 1, cx: b.x + b.width / 2, cy: b.y + b.height / 2, text: e.textContent };
  })()`);
}

// A drag from one point to another in small steps. It gives the selected text.
async function drag(p, x0, y0, x1, y1) {
  await p.eval(`window.getSelection().removeAllRanges()`);
  await mouse(p, "mousePressed", x0, y0, { clickCount: 1 });
  for (let i = 1; i <= 8; i++) await mouse(p, "mouseMoved", x0 + ((x1 - x0) * i) / 8, y0 + ((y1 - y0) * i) / 8, { buttons: 1 });
  await mouse(p, "mouseReleased", x1, y1, { clickCount: 1 });
  await sleep(200);
  return p.eval(`window.getSelection().toString()`);
}

// n quick clicks at a point (a double click, a triple click). It gives the selected text. Chrome takes `clickCount` as the number of the click in the
// series, so the series is n presses. The BiDi layer of Firefox makes n clicks for one press with `clickCount` n.
async function clicks(p, x, y, n) {
  await p.eval(`window.getSelection().removeAllRanges()`);
  if (process.env.HUNE_BROWSER === "firefox") {
    await mouse(p, "mousePressed", x, y, { clickCount: n });
    await mouse(p, "mouseReleased", x, y, { clickCount: n });
  } else {
    for (let i = 1; i <= n; i++) {
      await mouse(p, "mousePressed", x, y, { clickCount: i });
      await mouse(p, "mouseReleased", x, y, { clickCount: i });
    }
  }
  await sleep(200);
  return p.eval(`window.getSelection().toString()`);
}

// The clipboard text that a copy gives now.
const copied = (p) => p.eval(`(() => { let got = null; const h = (e) => { got = e.clipboardData.getData("text/plain"); }; document.addEventListener("copy", h); document.execCommand("copy"); document.removeEventListener("copy", h); return got; })()`);

async function openSelect(t, uid) {
  const r = await t.api.create("transit", selectDoc(uid));
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
  const p = await t.open(t.dash(uid));
  await waitPanelsReady(p, [1, 2, 3, 4]);
  await p.waitFor(`document.querySelectorAll(".chartCore__legendItem").length > 0`, { what: "the legend items" });
  await sleep(500);
  return p;
}

const pressed = (p) => p.eval(`[...document.querySelectorAll(".chartCore__legendItem")].map((e) => e.getAttribute("aria-pressed")).join(",")`);

export const cases = [
  {
    id: "B260",
    name: "select and copy: the legend text of a chart is selected by a drag and by a multiple click with no toggle, a plain click toggles, Ctrl+C copies (9.8e)",
    async run(t) {
      const p = await openSelect(t, "sel260");
      const before = await pressed(p);
      t.assert(before.includes("true"), `the legend has items: ${before}`);
      const item = await boxOf(p, ".chartCore__legendItem span");
      const label = item.text.trim();
      t.eq(await drag(p, item.x0, item.cy, item.x1, item.cy), label, "a drag over the text of a legend item selects the text");
      t.eq(await pressed(p), before, "the drag toggled no series");
      await p.eval(`window.getSelection().removeAllRanges()`);
      await p.click(".chartCore__legendItem");
      await sleep(300);
      t.assert((await pressed(p)) !== before, "a plain click still toggles the series");
      await p.click(".chartCore__legendItem");
      await sleep(300);
      t.eq(await pressed(p), before, "a second plain click gives the first state again");
      // A triple click selects the text. The first click of the three acts as a plain click (the page cannot know that two more come), and the second and
      // the third click do nothing for the legend: one click more gives the first state again.
      t.eq(await clicks(p, item.cx, item.cy, 3), label, "a triple click selects the text of the item");
      t.assert((await pressed(p)) !== before, "the first click of the triple click toggled once and the other two did not toggle again");
      await p.eval(`window.getSelection().removeAllRanges()`);
      await p.click(".chartCore__legendItem");
      await sleep(300);
      t.eq(await pressed(p), before, "one plain click after the triple click gives the first state again");
      // The key Ctrl+C on an item with the focus and no selection copies the text of the item.
      await p.eval(`window.getSelection().removeAllRanges(); document.querySelector(".chartCore__legendItem").focus()`);
      t.eq(await copied(p), label, "Ctrl+C on a focused legend item copies its text");
    },
  },
  {
    id: "B261",
    name: "select and copy: the text of a link in a table cell is selected by a drag and the link is not followed, a plain click follows it (9.8e)",
    async run(t) {
      const p = await openSelect(t, "sel261");
      const link = await boxOf(p, '[data-testid="panel-1"] a.cell-link');
      const here = await p.eval(`location.pathname + location.search`);
      t.eq(await drag(p, link.x0, link.cy, link.x1, link.cy), link.text.trim(), "a drag over the text of a link selects the text");
      await sleep(300);
      t.eq(await p.eval(`location.pathname + location.search`), here, "the drag did not follow the link");
      await p.eval(`window.getSelection().removeAllRanges()`);
      await p.click('[data-testid="panel-1"] a.cell-link');
      await p.waitFor(`location.search.includes("var-selected=")`, { what: "the plain click follows the link" });
    },
  },
  {
    id: "B262",
    name: "select and copy: the text that a chart draws on its canvas is in a layer, a drag over an axis selects it, a copy gives one label on each line, the tooltip works (9.8e)",
    async run(t) {
      const p = await openSelect(t, "sel262");
      await p.waitFor(`document.querySelectorAll(".textLayer span").length > 0`, { what: "the text layers" });
      // Each chart that draws text has one layer, and each layer is hidden from assistive technology and has at most 400 texts.
      const layers = await p.eval(`[...document.querySelectorAll(".textLayer")].map((l) => ({ n: l.children.length, hidden: l.getAttribute("aria-hidden"), host: l.parentElement.className.split(" ")[0] }))`);
      t.assert(layers.length >= 3, `the line chart, the bar chart and the pie chart have a layer: ${JSON.stringify(layers)}`);
      for (const l of layers) t.assert(l.hidden === "true" && l.n >= 1 && l.n <= 400, `a layer is hidden and has 1 to 400 texts: ${JSON.stringify(l)}`);
      // The labels of the vertical axis of the line chart: a drag from the first label to the last one.
      const labels = await p.eval(`(() => {
        const layer = document.querySelector('[data-testid="panel-2"] .textLayer');
        layer.scrollIntoView({ block: "center" });
        return [...layer.children].map((s) => { const b = s.getBoundingClientRect(); return { t: s.textContent, x0: b.x, x1: b.x + b.width, cy: b.y + b.height / 2 }; });
      })()`);
      t.assert(labels.length >= 4, `the line chart has labels: ${labels.length}`);
      const axis = labels.filter((l) => l.x0 < labels[0].x1 + 5 && Math.abs(l.x0 - labels[0].x0) < 25);
      t.assert(axis.length >= 3, `the vertical axis has labels: ${axis.map((l) => l.t)}`);
      const first = axis[0];
      const last = axis[axis.length - 1];
      const selected = await drag(p, first.x0 + 1, first.cy, last.x1 - 1, last.cy);
      for (const l of axis) t.assert(selected.includes(l.t), `the selection holds the label ${l.t}: ${selected}`);
      const copy = await copied(p);
      t.eq(copy, axis.map((l) => l.t).join("\n"), "the copy gives one label on each line, in reading order");
      // A text of the pie chart: a triple click selects one label.
      const pie = await boxOf(p, '[data-testid="panel-4"] .textLayer span');
      t.eq((await clicks(p, pie.cx, pie.cy, 3)).trim(), pie.text.trim(), "a triple click on a text of the pie chart selects it");
      // The pointer over the plot (away from a label) still shows the tooltip and the layer does not take the plot.
      const plot = await boxOf(p, '[data-testid="panel-2"] .chartCore__plot');
      await mouse(p, "mouseMoved", plot.cx, plot.cy);
      await p.waitFor(`(() => { const t = document.querySelector('[data-testid="panel-2"] .chartCore__tooltip'); return !!t && getComputedStyle(t).visibility !== "hidden" && t.textContent.trim() !== ""; })()`, { what: "the tooltip over the plot" });
      const top = await p.eval(`(() => { const e = document.elementFromPoint(${plot.cx}, ${plot.cy}); return e ? (e.closest(".textLayer") ? "layer" : "other") : "none"; })()`);
      t.eq(top, "other", "the point of the plot is not under the layer");
    },
  },
  {
    id: "B263",
    name: "select and copy: a drag over a sortable header selects its text and does not sort, a plain click sorts, a drag over cells selects text (9.8e)",
    async run(t) {
      const p = await openSelect(t, "sel263");
      const sorts = () => p.eval(`[...document.querySelectorAll('[data-testid="panel-1"] thead th')].map((e) => e.getAttribute("aria-sort")).join(",")`);
      const before = await sorts();
      const head = await boxOf(p, '[data-testid="panel-1"] thead th:nth-child(2) .vtable__label');
      t.eq(await drag(p, head.x0, head.cy, head.x1, head.cy), head.text.trim(), "a drag over a header selects its text");
      t.eq(await sorts(), before, "the drag did not sort");
      await p.eval(`window.getSelection().removeAllRanges()`);
      await p.click('[data-testid="panel-1"] thead th:nth-child(2)');
      await p.waitFor(`document.querySelector('[data-testid="panel-1"] thead th:nth-child(2)').getAttribute("aria-sort") === "ascending"`, { what: "a plain click sorts" });
      // A drag over two cells selects the text of both.
      const cell = await boxOf(p, '[data-testid="panel-1"] tbody tr:nth-child(1) td:nth-child(1)');
      t.assert((await drag(p, cell.x0, cell.cy, cell.x1 + 120, cell.cy)).trim().length > 0, "a drag over cells selects text");
    },
  },
];
