// Browser cases B220 and B221 (4.33f, 7.16e, 9.19h, T104; decision ID-K326, items P05 and D06): the logs panel.
// B220: the lines in both themes (the time, the level colours of Grafana 12.4.2, the gaps), the order, the option wrap, the details row
//       with the mouse and with the keys.
// B221: hostile lines stay text, the caps of lines and of characters, "No data", a frame with no text column, and the save refuses the
//       format `logs` outside a logs panel. All SQL is literal: no table is read.
import { doc, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "b220-logs";
const UID_HOSTILE = "b221-logs";
const T0 = 1794211200;

/** A logs panel with its own SQL. */
const logs = (id, title, sql, options = {}, grid = { x: 0, y: 0, w: 12, h: 8 }) => ({
  id, type: "logs", title, description: "", transparent: false, grid,
  data: { query: { source: "prod", format: "logs", sql: Array.isArray(sql) ? sql : [sql], min_interval: null, max_data_points: null } },
  transforms: [], fields: { defaults: {}, overrides: [] }, options, links: [],
});

const LEVELS = "['info','error','warn','debug','critical','trace','unknown']";
const ROWS = `SELECT toDateTime64(${T0} + number*60, 3, 'UTC') AS timestamp, concat('line ', toString(number), ' of the test') AS body, arrayElement(${LEVELS}, number % 7 + 1) AS level, concat('h', toString(number % 2)) AS host FROM numbers(7)`;
const LONG = `SELECT toDateTime64(${T0} + number*60, 3, 'UTC') AS timestamp, concat('long line ', toString(number), ' ', repeat('word ', 80)) AS body, 'debug' AS level FROM numbers(3)`;
const time = { from: "now-6h", to: "now", timezone: "utc", refresh: null };

const DOC = doc(UID, "Logs", [
  section(null, [
    logs(1, "Newest first", ROWS, { show_time: true }, { x: 0, y: 0, w: 12, h: 9 }),
    logs(2, "Oldest first", ROWS, { order: "oldest_first" }, { x: 12, y: 0, w: 12, h: 9 }),
    logs(3, "Wrap", LONG, { wrap: true, show_time: true }, { x: 0, y: 9, w: 12, h: 8 }),
    logs(4, "No wrap", LONG, { show_time: true }, { x: 12, y: 9, w: 12, h: 8 }),
    logs(5, "No details", ROWS, { details: false }, { x: 0, y: 17, w: 12, h: 6 }),
  ]),
], { time });

const HOSTILE = ["<img src=x onerror=window.__pwned=1>", "<script>window.__pwned=2</script>", "<b>bold</b>", "rtl \\xE2\\x80\\xAEabc", "&amp; &lt;", "javascript:alert(1)"];
const HOSTILE_SQL = `SELECT toDateTime64(${T0} + number, 3, 'UTC') AS timestamp, arrayElement([${HOSTILE.map((h) => `'${h}'`).join(", ")}], number + 1) AS body, arrayElement([${HOSTILE.map((h) => `'${h}'`).join(", ")}], number + 1) AS level, arrayElement([${HOSTILE.map((h) => `'${h}'`).join(", ")}], number + 1) AS host FROM numbers(6)`;
const HOSTILE_TEXT = ["<img src=x onerror=window.__pwned=1>", "<script>window.__pwned=2</script>", "<b>bold</b>", "rtl ‮abc", "&amp; &lt;", "javascript:alert(1)"];

const DOC_HOSTILE = doc(UID_HOSTILE, "Logs hostile", [
  section(null, [
    logs(1, "Hostile", HOSTILE_SQL, { order: "oldest_first" }, { x: 0, y: 0, w: 12, h: 8 }),
    logs(2, "Lines cap", `SELECT toDateTime64(${T0} + number, 3, 'UTC') AS timestamp, concat('n', toString(number)) AS body FROM numbers(10001)`, {}, { x: 12, y: 0, w: 12, h: 8 }),
    logs(3, "Characters cap", `SELECT toDateTime64(${T0} + number, 3, 'UTC') AS timestamp, repeat('x', 20000) AS body FROM numbers(201)`, {}, { x: 0, y: 8, w: 12, h: 6 }),
    logs(4, "Empty", `SELECT toDateTime64(${T0}, 3, 'UTC') AS timestamp, 'x' AS body FROM numbers(0)`, {}, { x: 12, y: 8, w: 12, h: 6 }),
    logs(5, "No text", `SELECT toDateTime64(${T0}, 3, 'UTC') AS timestamp, 1 AS n`, {}, { x: 0, y: 14, w: 12, h: 6 }),
  ]),
], { time });

// The facts of the lines of a panel: the texts, the colours, the boxes and the hook.
const FACTS = (id) => `(() => {
  const list = document.querySelector('[data-testid="logs-${id}"]');
  if (!list) return null;
  const lines = [...list.querySelectorAll(".logs__line")];
  const box = (e) => { if (!e) return null; const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; };
  return {
    hook: JSON.parse(list.dataset.logs),
    scroll: { sw: list.scrollWidth, cw: list.clientWidth },
    lines: lines.map((l) => {
      const t = l.querySelector(".logs__time"), v = l.querySelector(".logs__level"), b = l.querySelector(".logs__body");
      return { time: t ? t.textContent : null, level: v.textContent, color: getComputedStyle(v).color, weight: getComputedStyle(v).fontWeight, body: b.textContent,
        box: box(l), t: box(t), v: box(v), b: box(b), lh: getComputedStyle(l).lineHeight, font: getComputedStyle(l).fontSize, expanded: l.getAttribute("aria-expanded"), role: l.getAttribute("role"), tab: l.tabIndex };
    }),
  };
})()`;

// The light facts of a panel with many lines: the hook, the count of the lines, the first body and the note.
const LIGHT = (id) => `(() => {
  const list = document.querySelector('[data-testid="logs-${id}"]');
  const first = list.querySelector(".logs__line .logs__body");
  const note = list.querySelector(".logs__note");
  return { hook: JSON.parse(list.dataset.logs), count: list.querySelectorAll(".logs__line").length, first: first ? first.textContent : null, note: note ? note.textContent : null };
})()`;

const COLORS = {
  dark: { ERROR: "rgb(255, 82, 134)", WARN: "rgb(251, 173, 55)", INFO: "rgb(108, 207, 142)", DEBUG: "rgb(110, 159, 255)", CRIT: "rgb(184, 119, 217)" },
  light: { ERROR: "rgb(207, 14, 91)", WARN: "rgb(251, 173, 55)", INFO: "rgb(108, 207, 142)", DEBUG: "rgb(110, 159, 255)", CRIT: "rgb(184, 119, 217)" },
};

async function openDoc(t, uid, scheme, ids, state = "ready") {
  const p = await t.page();
  await p.emulateColorScheme(scheme);
  await p.goto(t.url(`d/${uid}/x`));
  await p.idle();
  await waitPanelsReady(p, ids, state);
  return p;
}

export const cases = [
  {
    id: "B220",
    name: "the logs panel: time, level colours and gaps of Grafana 12.4.2 in both themes, the order, wrap, and the details row with the mouse and the keys (9.19h)",
    async run(t) {
      const r = await t.api.create("transit", DOC);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      for (const scheme of ["dark", "light"]) {
        const p = await openDoc(t, UID, scheme, [1, 2, 3, 4, 5]);
        const a = await p.eval(FACTS(1));
        t.eq(a.hook, { lines: 7, total: 7, cut: null }, `${scheme}: the hook`);
        t.eq(a.lines.map((l) => l.body), [6, 5, 4, 3, 2, 1, 0].map((i) => `line ${i} of the test`), `${scheme}: newest first`);
        t.eq(a.lines.map((l) => l.time), [6, 5, 4, 3, 2, 1, 0].map((i) => `2026-11-09 08:0${i}:00.000`), `${scheme}: the time in the zone of the dashboard, with ms`);
        t.eq(a.lines.map((l) => l.level), ["", "TRACE", "CRIT", "DEBUG", "WARN", "ERROR", "INFO"], `${scheme}: the level texts`);
        for (const l of a.lines) {
          if (COLORS[scheme][l.level]) t.eq(l.color, COLORS[scheme][l.level], `${scheme}: the colour of ${l.level}`);
          t.eq([l.lh, l.font, l.weight], ["22px", "14px", "500"], `${scheme}: the line height, the size and the weight of the level`);
          t.assert(Math.abs(l.box.h - 28) < 0.6, `${scheme}: a line is 28 px high: ${l.box.h}`);
          t.assert(Math.abs(l.v.x - (l.t.x + l.t.w) - 12) < 0.6 && Math.abs(l.b.x - (l.v.x + l.v.w) - 12) < 0.6, `${scheme}: 12 px between the columns: ${JSON.stringify([l.t, l.v, l.b])}`);
        }
        const ch = a.lines[0].t.w / 23;
        t.assert(Math.abs(a.lines[0].v.w - 5 * ch) < 0.6, `${scheme}: the level column has the width of 5 characters: ${a.lines[0].v.w} for ${ch}`);
        const b = await p.eval(FACTS(2));
        t.eq(b.lines.map((l) => l.body)[0], "line 0 of the test", `${scheme}: oldest first`);
        t.eq(b.lines[0].time, null, `${scheme}: no time without show_time`);
        const w = await p.eval(FACTS(3));
        const nw = await p.eval(FACTS(4));
        t.assert(w.lines[0].box.h > 60 && w.scroll.sw <= w.scroll.cw, `${scheme}: wrap: the line takes several rows and the list has no side scroll: ${JSON.stringify([w.lines[0].box, w.scroll])}`);
        t.assert(Math.abs(nw.lines[0].box.h - 28) < 0.6 && nw.scroll.sw > nw.scroll.cw, `${scheme}: no wrap: one row and a side scroll: ${JSON.stringify([nw.lines[0].box, nw.scroll])}`);
        const off = await p.eval(FACTS(5));
        t.eq(off.lines.map((l) => [l.role, l.expanded]).slice(0, 2), [["listitem", null], ["listitem", null]], `${scheme}: details off: plain items`);
      }
      // The details row: a click opens it under the line, a second click closes it; the keys move the one tab stop and toggle the row.
      const p = await openDoc(t, UID, "dark", [1]);
      const line = '[data-testid="logs-1"] .logs__line';
      await p.click(`${line}:nth-child(1)`);
      await p.waitSel('[data-testid="logs-1-details"]');
      const open = await p.eval(`(() => {
        const box = document.querySelector('[data-testid="logs-1-details"]');
        const first = document.querySelector('${line}');
        return { after: first.nextElementSibling === box, expanded: first.getAttribute("aria-expanded"),
          names: [...box.querySelectorAll("dt")].map((n) => n.textContent), values: [...box.querySelectorAll("dd")].map((n) => n.textContent),
          top: box.getBoundingClientRect().top - first.getBoundingClientRect().bottom };
      })()`);
      t.eq([open.after, open.expanded, open.names, open.values], [true, "true", ["level", "host"], ["unknown", "h0"]], "the details row of the first line: the level and the host");
      t.assert(Math.abs(open.top) < 0.6, `the details row starts at the foot of the line: ${open.top}`);
      await p.click(`${line}:nth-child(1)`);
      await p.waitGone('[data-testid="logs-1-details"]');
      await p.focus(`${line}:nth-child(1)`);
      await p.press("ArrowDown");
      await p.press("ArrowDown");
      const keys = await p.eval(`(() => { const ls = [...document.querySelectorAll('${line}')]; return { active: ls.indexOf(document.activeElement), tabs: ls.map((l) => l.tabIndex) }; })()`);
      t.eq(keys.active, 2, "two presses of ArrowDown move the focus to the third line");
      t.eq(keys.tabs.filter((x) => x === 0).length, 1, "one tab stop in the list");
      await p.press("Enter");
      await p.waitSel('[data-testid="logs-1-details"]');
      t.eq(await p.eval(`document.activeElement.getAttribute("aria-expanded")`), "true", "Enter opens the details of the focused line");
      await p.press("End");
      t.eq(await p.eval(`[...document.querySelectorAll('${line}')].indexOf(document.activeElement)`), 6, "End moves to the last line");
    },
  },
  {
    id: "B221",
    name: "the logs panel: hostile lines stay text, the caps of lines and characters, No data, a frame with no text column, and the format logs outside a logs panel (T104)",
    async run(t) {
      // The save refuses the format `logs` in a table.
      const bad = structuredClone(DOC_HOSTILE);
      bad.uid = "b221-bad";
      bad.sections[0].panels.push({ ...logs(9, "Table", "SELECT 1 AS a", {}, { x: 12, y: 14, w: 12, h: 6 }), type: "table", options: { show_header: true, sort: [] } });
      const refused = await t.api.create("transit", bad);
      t.assert(refused.status === 422 && refused.text.includes("/sections/0/panels/5/data/query/format") && refused.text.includes("not_permitted"), `the format logs in a table: ${refused.status} ${refused.text.slice(0, 300)}`);
      const r = await t.api.create("transit", DOC_HOSTILE);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      const p = await t.page();
      await p.goto(t.url(`d/${UID_HOSTILE}/x`));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3, 5]);
      await waitPanelsReady(p, [4], "empty");
      const h = await p.eval(FACTS(1));
      t.eq(h.lines.map((l) => l.body), HOSTILE_TEXT, "each hostile body is its own text");
      await p.click('[data-testid="logs-1"] .logs__line:nth-child(1)');
      await p.waitSel('[data-testid="logs-1-details"]');
      const dom = await p.eval(`(() => {
        const panel = document.querySelector('[data-testid="logs-1"]');
        return { markup: panel.querySelectorAll("img, script, b, svg, a").length, pwned: window.__pwned === undefined ? null : window.__pwned,
          dd: [...panel.querySelectorAll("dd")].map((n) => n.textContent),
          attrs: [...panel.querySelectorAll("*")].some((e) => [...e.attributes].some((a) => a.name.startsWith("on"))) };
      })()`);
      t.eq([dom.markup, dom.pwned, dom.attrs], [0, null, false], "no element and no event attribute come from the text, and no script ran");
      t.eq(dom.dd, [HOSTILE_TEXT[0], HOSTILE_TEXT[0]], "the details hold the hostile values as text");
      const lines = await p.eval(LIGHT(2));
      t.eq([lines.hook, lines.count], [{ lines: 10000, total: 10001, cut: "lines" }, 10000], "10,001 rows give 10,000 lines");
      t.eq(lines.first, "n10000", "the newest line is the first");
      t.eq(lines.note, "The panel shows the first 10,000 of 10,001 lines (the limit of lines).", "the note of the cap of lines");
      const chars = await p.eval(LIGHT(3));
      t.eq([chars.hook, chars.count], [{ lines: 199, total: 201, cut: "chars" }, 199], "201 bodies of 20,000 characters give 199 lines of 10,001 characters");
      t.eq([chars.first.length, chars.first.slice(-2)], [10001, "x…"], "a body shows 10,000 characters and the sign of the cut");
      t.eq(chars.note, "The panel shows the first 199 of 201 lines (the limit of characters).", "the note of the cap of characters");
      t.eq(await p.eval(`document.querySelector('[data-testid="panel-4"]').textContent.includes("No data")`), true, "a query with no row shows No data");
      t.eq(await p.eval(`document.querySelector('[data-testid="logs-5-shape"]').textContent`), "Cannot show log linesThe data needs a time column and a text column to show log lines.", "a frame with no text column");
    },
  },
];
