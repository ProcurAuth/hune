// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B264 to B266 (9.8e): the copy of the lines of the logs panel and of the rows of a table, the cursor over text and the right button,
// with real mouse events.
// B264: a drag over one cell is the plain text of the cell. A drag over cells and lines, a triple click on a line and Ctrl+A in the panel give one text line
//       for each log line, with the cells joined by one space. A triple click leaves the details as they were. A plain click still toggles them.
// B265: the rows of a table keep the cells separated by a tab, and the lines of a logs panel with the option wrap copy as the others.
// B266: the cursor over the text of data is the text cursor, and the controls keep the pointer. The right button on a selection keeps the selection and
//       changes nothing (no toggle of a log line, of a legend item or of a sort), and the copy of the context menu gives the same text as Ctrl+C.
// The copied text is read with a spy on the event `copy`: the page puts the text in `event.clipboardData`, and the spy reads it after the listeners of
// the page. A copy that the page does not handle has `defaultPrevented` false: the browser then copies the selection as it is.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const T0 = 1794211200;
const reduce = (over = {}) => ({ calc: "last_not_null", fields: null, values: false, ...over });
const g = (x, y, w, h) => ({ x, y, w, h });

const logs = (id, title, sql, options, grid) => ({
  id, type: "logs", title, description: "", transparent: false, grid,
  data: { query: { source: "prod", format: "logs", sql: [sql], min_interval: null, max_data_points: null } },
  transforms: [], fields: { defaults: {}, overrides: [] }, options, links: [],
});
const ROWS = `SELECT toDateTime64(${T0} + number*60, 3, 'UTC') AS timestamp, concat('line ', toString(number), ' of the test') AS body, arrayElement(['warn','error','info'], number % 3 + 1) AS level, concat('h', toString(number)) AS host FROM numbers(6)`;
const time = { from: "now-6h", to: "now", timezone: "utc", refresh: null };

function copyDoc(uid) {
  return doc(uid, `Copy ${uid}`, [
    section(null, [
      logs(1, "Lines", ROWS, { show_time: true }, g(0, 0, 12, 9)),
      logs(2, "Wrap", ROWS, { show_time: true, wrap: true }, g(12, 0, 12, 9)),
      panel(3, "table", "Table", SQL.table, { grid: g(0, 9, 12, 8) }),
      panel(4, "timeseries", "Lines", SQL.series, { grid: g(12, 9, 12, 8), options: { draw: "line" } }),
      panel(5, "barchart", "Bars", SQL.bars, { grid: g(0, 17, 8, 8), options: { orientation: "horizontal" } }),
      panel(6, "bargauge", "Gauge", SQL.bars, { grid: g(8, 17, 8, 8), options: { reduce: reduce({ values: true }) } }),
    ]),
  ], { time });
}

const mouse = (p, type, x, y, extra = {}) => p.send("Input.dispatchMouseEvent", { type, x, y, button: "left", ...extra });

const box = (p, selector, nth = 0) => p.eval(`(() => {
  const e = document.querySelectorAll(${JSON.stringify(selector)})[${nth}];
  if (!e) return null;
  e.scrollIntoView({ block: "center", inline: "nearest" });
  const b = e.getBoundingClientRect();
  return { x0: b.x + 1, x1: b.x + b.width - 1, y0: b.y + 1, y1: b.y + b.height - 1, cx: b.x + b.width / 2, cy: b.y + b.height / 2, text: e.textContent };
})()`);

async function drag(p, x0, y0, x1, y1) {
  await p.eval(`window.getSelection().removeAllRanges()`);
  await mouse(p, "mousePressed", x0, y0, { clickCount: 1 });
  for (let i = 1; i <= 8; i++) await mouse(p, "mouseMoved", x0 + ((x1 - x0) * i) / 8, y0 + ((y1 - y0) * i) / 8, { buttons: 1 });
  await mouse(p, "mouseReleased", x1, y1, { clickCount: 1 });
  await sleep(250);
}

// n quick clicks at a point (a double click, a triple click). Chrome takes `clickCount` as the number of the click in the series, so the series is
// n presses with the counts 1 to n. The BiDi layer of Firefox makes n clicks for one press with `clickCount` n, so one press gives the series there.
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
  await sleep(300);
}

// A click of the right button at a point: a press and a release. A real browser opens its context menu then, with the item Copy.
async function rightClick(p, x, y) {
  await p.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "right", buttons: 2, clickCount: 1 });
  await p.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "right", buttons: 0, clickCount: 1 });
  await sleep(250);
}
const selected = (p) => p.eval(`window.getSelection().toString()`);

// What a copy gives now: `{ data, prevented }`.
const copy = (p) => p.eval(`(() => {
  let out = { data: null, prevented: null };
  const spy = (e) => { out = { data: e.clipboardData.getData("text/plain"), prevented: e.defaultPrevented }; };
  document.addEventListener("copy", spy);
  document.execCommand("copy");
  document.removeEventListener("copy", spy);
  return out;
})()`);

// The text of the line n of a logs panel as one text line: the cells joined by one space.
const lineText = (p, panelId, n) => p.eval(`[...document.querySelectorAll('[data-testid="panel-${panelId}"] .logs__line')][${n}].children.length && [...[...document.querySelectorAll('[data-testid="panel-${panelId}"] .logs__line')][${n}].children].map((c) => c.textContent).join(" ")`);
const expanded = (p, panelId, n) => p.eval(`document.querySelectorAll('[data-testid="panel-${panelId}"] .logs__line')[${n}].getAttribute("aria-expanded")`);
const detailsCount = (p, panelId) => p.count(`[data-testid="panel-${panelId}"] .logs__details`);

async function openCopy(t, uid) {
  const r = await t.api.create("transit", copyDoc(uid));
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
  const p = await t.open(t.dash(uid));
  await waitPanelsReady(p, [1, 2, 3, 4, 5, 6]);
  await p.waitFor(`document.querySelectorAll('[data-testid="panel-1"] .logs__line').length === 6 && document.querySelectorAll('[data-testid="panel-2"] .logs__line').length === 6`, { what: "the log lines" });
  await sleep(400);
  return p;
}

export const cases = [
  {
    id: "B264",
    name: "select and copy: a log line copies as one text line, a triple click selects the whole line and leaves the details, Ctrl+A selects the lines (9.8e)",
    async run(t) {
      const p = await openCopy(t, "sel264");
      const first = await lineText(p, 1, 0);
      const second = await lineText(p, 1, 1);
      t.assert(/^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3} (WARN|ERROR|INFO) line \d of the test$/.test(first), `a line has the time, the level and the body: ${first}`);

      // A drag inside one cell is the plain text of the cell: the page does not handle the copy.
      const body = await box(p, '[data-testid="panel-1"] .logs__line:nth-of-type(1) .logs__body');
      await drag(p, body.x0, body.cy, body.x0 + 40, body.cy);
      const oneCell = await copy(p);
      t.eq(oneCell.prevented, false, "a selection inside one cell uses the copy of the browser");
      t.eq(await expanded(p, 1, 0), "false", "the drag did not open the details");

      // A drag over the three cells of a line, and over two lines: one text line for each log line.
      const row0 = await box(p, '[data-testid="panel-1"] .logs__line', 0);
      const row1 = await box(p, '[data-testid="panel-1"] .logs__line', 1);
      await drag(p, row0.x0 + 1, row0.cy, row0.x1 - 2, row0.cy);
      const oneLine = await copy(p);
      const info = await p.eval(`(() => { const s = getSelection(); const r = s.rangeCount ? s.getRangeAt(0) : null; const n = (x) => x ? (x.nodeName + "." + (x.className || "") + ":" + String(x.data || "").slice(0, 12)) : null; return JSON.stringify({ text: s.toString(), anchor: n(s.anchorNode), ao: s.anchorOffset, focus: n(s.focusNode), fo: s.focusOffset, common: r && n(r.commonAncestorContainer) }); })()`);
      t.eq(oneLine.prevented, true, `a selection over cells is handled by the page ${info}`);
      t.eq(oneLine.data, first, "a drag over one whole line copies one text line: the time, the level and the body with one space");
      const lines = await p.eval(`(() => { const rows = [...document.querySelectorAll('[data-testid="panel-1"] .logs__line')]; const r0 = rows[0].getBoundingClientRect(), r1 = rows[1].getBoundingClientRect(); return { x0: r0.x + 1, y0: r0.y + r0.height / 2, x1: r1.x + r1.width - 1, y1: r1.y + r1.height / 2 }; })()`);
      await drag(p, lines.x0, lines.y0, lines.x1, lines.y1);
      const twoLines = await copy(p);
      t.eq(twoLines.data, `${first}\n${second}`, "a drag over two lines copies two text lines with one line end between them");
      t.eq(await detailsCount(p, 1), 0, "the drags opened no details");

      // A drag that starts inside a cell keeps the partial text of that cell.
      const startBody = await box(p, '[data-testid="panel-1"] .logs__line:nth-of-type(1) .logs__body');
      const secondTime = await box(p, '[data-testid="panel-1"] .logs__line:nth-of-type(2) .logs__time');
      await drag(p, startBody.x0 + 30, startBody.cy, secondTime.x0 + 30, secondTime.cy);
      const partial = await copy(p);
      const lines2 = partial.data.split("\n");
      t.eq(lines2.length, 2, `a partial drag over two lines gives two text lines: ${JSON.stringify(partial.data)}`);
      t.assert(lines2[0].length > 0 && lines2[0].length < startBody.text.length && startBody.text.endsWith(lines2[0]), `the first line holds the end of the body: ${lines2[0]}`);
      t.assert(lines2[1].length > 0 && lines2[1].length < secondTime.text.length && second.startsWith(lines2[1]), `the second line holds the start of the time: ${lines2[1]}`);

      // A triple click selects the whole line, and the details stay as they were (closed).
      const row3 = await box(p, '[data-testid="panel-1"] .logs__line', 2);
      const third = await lineText(p, 1, 2);
      await p.eval(`window.__details = []; document.addEventListener("click", (e) => window.__details.push(e.detail), true);`);
      await clicks(p, row3.x0 + 30, row3.cy, 3);
      const seen = await p.eval(`JSON.stringify(window.__details)`);
      t.eq((await copy(p)).data, third, "a triple click selects the whole line: the time, the level and the body on one text line");
      t.eq(await expanded(p, 1, 2), "false", `a triple click leaves the details closed (the details of the clicks: ${seen})`);
      t.eq(await detailsCount(p, 1), 0, "no details row after the triple click");
      // A triple click on a line that is open leaves it open.
      await p.eval(`window.getSelection().removeAllRanges()`);
      await p.click('[data-testid="panel-1"] .logs__line:nth-of-type(2) .logs__body');
      await p.waitFor(`document.querySelector('[data-testid="panel-1"] .logs__line[data-i="1"]').getAttribute("aria-expanded") === "true"`, { what: "a plain click opens the details" });
      const open = await box(p, '[data-testid="panel-1"] .logs__line[data-i="1"]');
      await clicks(p, open.x0 + 30, open.cy, 3);
      t.eq(await expanded(p, 1, 1), "true", "a triple click leaves an open line open");
      t.eq((await copy(p)).data, second, "the triple click on an open line copies the line only, with no detail");
      // A double click selects a word and leaves the details as they were. A plain click toggles them.
      await clicks(p, open.x0 + 30, open.cy, 2);
      t.eq(await expanded(p, 1, 1), "true", "a double click leaves an open line open");
      await p.eval(`window.getSelection().removeAllRanges()`);
      await p.click('[data-testid="panel-1"] .logs__line:nth-of-type(2) .logs__body');
      await p.waitFor(`document.querySelector('[data-testid="panel-1"] .logs__line[data-i="1"]').getAttribute("aria-expanded") === "false"`, { what: "a plain click closes the details" });

      // Ctrl+A with the focus on a line selects the lines of the panel, and the copy gives one text line for each.
      await p.eval(`window.getSelection().removeAllRanges(); document.querySelector('[data-testid="panel-1"] .logs__line').focus()`);
      await p.press("a", { modifiers: 2 });
      await sleep(250);
      const all = await copy(p);
      const expected = [];
      for (let i = 0; i < 6; i++) expected.push(await lineText(p, 1, i));
      t.eq(all.data, expected.join("\n"), "Ctrl+A in the panel copies the six lines, one text line for each");
      t.eq(await p.eval(`window.getSelection().toString().includes("Wrap")`), false, "Ctrl+A did not select the rest of the page");
    },
  },
  {
    id: "B265",
    name: "select and copy: the lines of a logs panel with wrap copy as one text line, the cells of a table copy with a tab (9.8e)",
    async run(t) {
      const p = await openCopy(t, "sel265");
      const wrapLine = await lineText(p, 2, 0);
      const row = await box(p, '[data-testid="panel-2"] .logs__line', 0);
      await clicks(p, row.x0 + 30, row.cy, 3);
      t.eq((await copy(p)).data, wrapLine, "a triple click on a line with wrap copies one text line");
      const a = await box(p, '[data-testid="panel-2"] .logs__line', 0);
      const b = await box(p, '[data-testid="panel-2"] .logs__line', 1);
      await drag(p, a.x0 + 1, a.cy, b.x1, b.cy);
      const two = await copy(p);
      t.eq(two.data.split("\n").length, 2, `a drag over two lines with wrap gives two text lines ${JSON.stringify(two)} ${JSON.stringify(await selected(p))}`);
      // The cells of a table: the copy is the one of the browser, with a tab between two cells of a row and a line end between rows.
      const c1 = await box(p, '[data-testid="panel-3"] tbody tr:nth-child(1) td:nth-child(1)');
      const c2 = await box(p, '[data-testid="panel-3"] tbody tr:nth-child(2) td:nth-child(2)');
      await drag(p, c1.x0, c1.cy, c2.x1, c2.cy);
      const table = await copy(p);
      t.eq(table.prevented, false, "a selection in a table uses the copy of the browser");
      const tableSelection = await selected(p);
      t.assert(tableSelection.includes("\t") && tableSelection.includes("\n"), `the selection of table cells has a tab and a line end: ${JSON.stringify(tableSelection)}`);
      // The rows of the horizontal bar chart and of the bar gauge: the name and the value on one text line, one line for each row.
      for (const [container, row, what] of [['[data-testid="panel-5"] .hbar__rows', ".hbar__row", "the horizontal bar chart"], ['[data-testid="panel-6"] .bargauge', ".bargauge__item", "the bar gauge"]]) {
        const rows = await p.count(`${container} ${row}`);
        t.assert(rows >= 2, `${what} has rows: ${rows}`);
        await p.eval(`(() => { const c = document.querySelector(${JSON.stringify(container)}); c.scrollIntoView({ block: "center" }); const r = document.createRange(); r.selectNodeContents(c); const s = getSelection(); s.removeAllRanges(); s.addRange(r); })()`);
        const rowsCopy = await copy(p);
        t.eq(rowsCopy.prevented, true, `${what}: a selection of its rows is handled by the page`);
        const lines = rowsCopy.data.split("\n");
        t.eq(lines.length, rows, `${what}: one text line for each row: ${JSON.stringify(rowsCopy.data)}`);
        for (const line of lines) t.assert(/^\S+ \S+$/.test(line), `${what}: a row is the name and the value with one space: ${JSON.stringify(line)}`);
      }
    },
  },
  {
    id: "B266",
    name: "select and copy: the text of data has the text cursor and the controls keep the pointer, the right button keeps the selection and changes nothing (9.8e)",
    async run(t) {
      const p = await openCopy(t, "sel266");
      await p.waitFor(`document.querySelectorAll(".chartCore__legendItem").length > 0 && document.querySelectorAll('[data-testid="panel-4"] .textLayer span').length > 0`, { what: "the legend and the text layer" });
      // The cursor over the text of data is the text cursor: the cells of a log line, the label of a legend item, a table cell, a panel title and a label of an axis.
      const cursor = (selector) => p.eval(`getComputedStyle(document.querySelector(${JSON.stringify(selector)})).cursor`);
      for (const [selector, what] of [
        ['[data-testid="panel-1"] .logs__line', "a log line"],
        ['[data-testid="panel-1"] .logs__body', "the message of a log line"],
        ['[data-testid="panel-4"] .chartCore__legendItem > span', "the label of a legend item"],
        ['[data-testid="panel-3"] tbody td', "a table cell"],
        ['[data-testid="panel-1"] .panel__title', "a panel title"],
        ['[data-testid="panel-4"] .textLayer span', "a label of an axis"],
      ]) t.eq(await cursor(selector), "text", `the cursor over ${what} is the text cursor`);
      // The controls keep the pointer: the sortable header, and the button of a legend item outside its label.
      t.eq(await cursor('[data-testid="panel-3"] .dataTable__sort'), "pointer", "a sortable header keeps the pointer");
      t.eq(await cursor('[data-testid="panel-4"] .chartCore__legendItem'), "pointer", "a legend item keeps the pointer");
      // No page code stops the menu of the browser.
      await p.eval(`window.__menu = []; document.addEventListener("contextmenu", (e) => window.__menu.push(e.defaultPrevented));`);

      // The right button on a selection of two log lines: the selection stays, no line opens, and the copy is the same text.
      const lines = await p.eval(`(() => { const rows = [...document.querySelectorAll('[data-testid="panel-1"] .logs__line')]; rows[0].scrollIntoView({ block: "center" }); const r0 = rows[0].getBoundingClientRect(), r1 = rows[1].getBoundingClientRect(); return { x0: r0.x + 1, y0: r0.y + r0.height / 2, x1: r1.x + r1.width - 2, y1: r1.y + r1.height / 2, mx: r0.x + 60, my: r0.y + r0.height / 2 }; })()`);
      await drag(p, lines.x0, lines.y0, lines.x1, lines.y1);
      const first = await lineText(p, 1, 0);
      const second = await lineText(p, 1, 1);
      const before = await selected(p);
      t.assert(before.length > 0, "the drag selected the two lines");
      await rightClick(p, lines.mx, lines.my);
      t.eq(await selected(p), before, "the right button on the selected log lines keeps the selection");
      t.eq(await expanded(p, 1, 0), "false", "the right button did not open a log line");
      t.eq(await detailsCount(p, 1), 0, "the right button opened no details row");
      t.eq((await copy(p)).data, `${first}\n${second}`, "the copy after the right button is one text line for each log line");

      // The right button on a selected legend label: the selection stays and the series do not change.
      const pressedBefore = await p.eval(`[...document.querySelectorAll('[data-testid="panel-4"] .chartCore__legendItem')].map((e) => e.getAttribute("aria-pressed")).join(",")`);
      const label = await box(p, '[data-testid="panel-4"] .chartCore__legendItem > span');
      await drag(p, label.x0, label.cy, label.x1, label.cy);
      const legendSelection = await selected(p);
      t.eq(legendSelection, label.text.trim(), "a drag over a legend label selects it");
      await rightClick(p, label.cx, label.cy);
      t.eq(await selected(p), legendSelection, "the right button on the selected legend label keeps the selection");
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="panel-4"] .chartCore__legendItem')].map((e) => e.getAttribute("aria-pressed")).join(",")`), pressedBefore, "the right button toggled no series");

      // The right button on selected labels of an axis, and on selected table cells.
      const axis = await p.eval(`(() => { const layer = document.querySelector('[data-testid="panel-4"] .textLayer'); layer.scrollIntoView({ block: "center" }); return [...layer.children].map((s) => { const b = s.getBoundingClientRect(); return { t: s.textContent, x0: b.x, x1: b.x + b.width, cy: b.y + b.height / 2 }; }); })()`);
      const yl = axis.filter((l) => /ms$|^\d+$/.test(l.t) && Math.abs(l.x0 - axis[0].x0) < 25);
      t.assert(yl.length >= 3, `the axis has labels: ${yl.map((l) => l.t)}`);
      await drag(p, yl[0].x0 + 1, yl[0].cy, yl[yl.length - 1].x1 - 1, yl[yl.length - 1].cy);
      const axisSelection = await selected(p);
      await rightClick(p, (yl[1].x0 + yl[1].x1) / 2, yl[1].cy);
      t.eq(await selected(p), axisSelection, "the right button on selected axis labels keeps the selection");
      const c1 = await box(p, '[data-testid="panel-3"] tbody tr:nth-child(1) td:nth-child(1)');
      const c2 = await box(p, '[data-testid="panel-3"] tbody tr:nth-child(2) td:nth-child(2)');
      const sortsBefore = await p.eval(`[...document.querySelectorAll('[data-testid="panel-3"] thead th')].map((e) => e.getAttribute("aria-sort")).join(",")`);
      await drag(p, c1.x0, c1.cy, c2.x1, c2.cy);
      const cellSelection = await selected(p);
      await rightClick(p, c1.cx, c1.cy);
      t.eq(await selected(p), cellSelection, "the right button on selected table cells keeps the selection");
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="panel-3"] thead th')].map((e) => e.getAttribute("aria-sort")).join(",")`), sortsBefore, "the right button sorted no column");
      t.assert((await p.eval(`window.__menu`)).every((prevented) => prevented === false), "no code of the page stops the menu of the browser");
    },
  },
];
