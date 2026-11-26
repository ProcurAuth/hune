// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B390 to B392 (9.19h, 9.20): the facts of the loaded `logs` and `table` panels that the loading preview copies.
// B390: the logs panel with `wrap`: the time has one row and the columns keep their places and their gaps (9.19h, section 2, rules 2 and 7).
// B391: a logs line with all cells empty is 28 px high, with and without `wrap` (9.19h, section 2, rule 3).
// B392: the header of a table is 34 px high, and 47 px on a coarse pointer with a filter button (9.20).
// All SQL is literal: no table is read.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const T0 = 1794211200;
const time = { from: "now-6h", to: "now", timezone: "utc", refresh: null };
const g = (x, y, w, h) => ({ x, y, w, h });

const logs = (id, title, sql, options, grid) => ({
  id, type: "logs", title, description: "", transparent: false, grid,
  data: { query: { source: "prod", format: "logs", sql: [sql], min_interval: null, max_data_points: null } },
  transforms: [], fields: { defaults: {}, overrides: [] }, options, links: [],
});

// Short bodies, so that each line has one row of text. The longest level text is ERROR (5 characters).
const SHORT = `SELECT toDateTime64(${T0} + number*60, 3, 'UTC') AS timestamp, concat('line ', toString(number)) AS body, arrayElement(['info','error','debug'], number % 3 + 1) AS level FROM numbers(3)`;
// Lines with no body and no level text.
const EMPTY = `SELECT toDateTime64(${T0} + number*60, 3, 'UTC') AS timestamp, '' AS body FROM numbers(3)`;

const UID_LOGS = "b390-logs-loaded";
const UID_EMPTY = "b391-logs-empty";
const logsDoc = (uid) => doc(uid, "Logs loaded", [
  section(null, [
    logs(1, "Wrap with time", SHORT, { wrap: true, show_time: true }, g(0, 0, 12, 8)),
    logs(2, "Wrap with no time", SHORT, { wrap: true }, g(12, 0, 12, 8)),
    logs(3, "No wrap with time", SHORT, { show_time: true }, g(0, 8, 12, 8)),
    logs(4, "Empty lines", EMPTY, {}, g(12, 8, 12, 8)),
    logs(5, "Empty lines with wrap", EMPTY, { wrap: true }, g(0, 16, 12, 8)),
  ]),
], { time });

// The boxes of the first line of a logs panel and the width of 1ch in its font.
const LINE = (id) => `(() => {
  const list = document.querySelector('[data-testid="logs-${id}"]');
  const probe = document.createElement("span");
  probe.style.cssText = "position:absolute;visibility:hidden;width:100ch;height:1px";
  list.append(probe);
  const ch = probe.getBoundingClientRect().width / 100;
  probe.remove();
  const rect = (e) => { if (!e) return null; const r = e.getClientRects()[0]; return { x: r.x, y: r.y, w: r.width, h: r.height }; };
  const l = list.querySelector(".logs__line");
  const lb = l.getBoundingClientRect();
  const lines = [...list.querySelectorAll(".logs__line")].map((x) => Math.round(x.getBoundingClientRect().height * 10) / 10);
  return { ch, list: rect(list), line: { x: lb.x, y: lb.y, w: lb.width, h: lb.height }, lines,
    t: rect(l.querySelector(".logs__time")), v: rect(l.querySelector(".logs__level")), b: rect(l.querySelector(".logs__body")),
    levelCh: getComputedStyle(list).getPropertyValue("--logs-level") };
})()`;

async function open(t, uid, ids, scheme = "dark") {
  const p = await t.page();
  await p.emulateColorScheme(scheme);
  await p.goto(t.url(`d/${uid}/x`));
  await p.idle();
  await waitPanelsReady(p, ids);
  return p;
}

const near = (a, b, tol = 0.6) => Math.abs(a - b) < tol;

const UID_TABLE = "b392-table-head";
const tableDoc = (uid, panels) => doc(uid, "Table head", [section(null, panels)], { time });
const SQL2 = "SELECT 1 AS a, 'x' AS b UNION ALL SELECT 2, 'y'";

/** Make the page match `(pointer: coarse)`. Returns false when this browser cannot. */
async function coarse(p) {
  await p.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 1 });
  await p.send("Emulation.setEmulatedMedia", { features: [{ name: "pointer", value: "coarse" }, { name: "hover", value: "none" }] });
}

export const cases = [
  {
    id: "B390",
    name: "the logs panel with wrap: the time has one row, and the level and the body start after the 12 px gaps, as without wrap (9.19h)",
    async run(t) {
      const r = await t.api.create("transit", logsDoc(UID_LOGS));
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      const p = await open(t, UID_LOGS, [1, 2, 3]);
      for (const id of [1, 3]) {
        const a = await p.eval(LINE(id));
        const wrap = id === 1;
        // The time column is 23 characters wide. The level starts 12 px after it. The body starts 12 px after the level column.
        t.assert(near(a.t.w, 23 * a.ch, 0.6) || near(a.t.w, 23 * a.ch + 12, 0.6), `panel ${id}: the time box is 23 characters (plus the gap with wrap): ${a.t.w} for ${a.ch}`);
        t.assert(near(a.t.h, 22), `panel ${id}: the time has one row of 22 px: ${a.t.h}`);
        t.assert(near(a.v.x - a.line.x, 23 * a.ch + 12), `panel ${id}: the level starts at 23ch + 12 px: ${a.v.x - a.line.x} for ${a.ch}`);
        t.assert(near(a.b.x - a.v.x, 5 * a.ch + 12), `panel ${id}: the body starts at the level plus 5ch + 12 px: ${a.b.x - a.v.x} for ${a.ch}`);
        t.eq(a.lines.map((h) => Math.round(h)), [28, 28, 28], `panel ${id}${wrap ? " (wrap)" : ""}: each line is one row of 28 px`);
      }
      const b = await p.eval(LINE(2));
      t.assert(near(b.v.x - b.line.x, 0), `no time: the level starts at the left edge: ${b.v.x - b.line.x}`);
      t.assert(near(b.b.x - b.v.x, 5 * b.ch + 12), `wrap, no time: the body starts at the level plus 5ch + 12 px: ${b.b.x - b.v.x} for ${b.ch}`);
      t.assert(near(b.v.h, 22), `wrap, no time: the level has one row of 22 px: ${b.v.h}`);
    },
  },
  {
    id: "B391",
    name: "a logs line with all cells empty is 28 px high, with and without wrap (9.19h)",
    async run(t) {
      const r = await t.api.create("transit", logsDoc(UID_EMPTY));
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      const p = await open(t, UID_EMPTY, [1, 2, 3, 4, 5]);
      for (const id of [4, 5]) {
        const a = await p.eval(LINE(id));
        t.eq(a.lines.map((h) => Math.round(h * 10) / 10), [28, 28, 28], `panel ${id}: a line with no text is 28 px high`);
      }
    },
  },
  {
    id: "B392",
    name: "the header of a table is 34 px high, and 47 px on a coarse pointer when a column has a filter button (9.20)",
    async run(t) {
      const filt = { defaults: { filterable: true }, overrides: [] };
      const d = tableDoc(UID_TABLE, [
        panel(1, "table", "Filter", SQL2, { grid: g(0, 0, 12, 8), fields: filt }),
        panel(2, "table", "No filter", SQL2, { grid: g(12, 0, 12, 8) }),
        panel(3, "table", "No header", SQL2, { grid: g(0, 8, 12, 8), options: { show_header: false }, fields: filt }),
      ]);
      const r = await t.api.create("transit", d);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      const heads = (p) => p.eval(`(() => [1, 2, 3].map((id) => {
        const tr = document.querySelector('[data-testid="table-' + id + '"] thead tr');
        const row = document.querySelector('[data-testid="table-' + id + '"] tbody tr[data-row]');
        return { head: tr ? tr.getBoundingClientRect().height : 0, row: row ? row.getBoundingClientRect().height : 0, top: row ? row.getBoundingClientRect().top - row.closest(".vtable__scroll").getBoundingClientRect().top : 0 };
      }))()`);
      // A fine pointer: 34 px with and without a filter button.
      let p = await open(t, UID_TABLE, [1, 2, 3]);
      t.eq(await p.eval(`matchMedia("(pointer: coarse)").matches`), false, "the default page has a fine pointer");
      let h = await heads(p);
      t.eq([h[0].head, h[1].head, h[2].head, h[0].row, h[0].top], [34, 34, 0, 36, 34], "a fine pointer: the header is 34 px, no header gives 0, a row is 36 px");
      // A coarse pointer: the filter button is 40 px high, so a header with a filter is 47 px (40 + 6 + the 1 px rule).
      p = await t.page();
      await coarse(p);
      await p.goto(t.url(`d/${UID_TABLE}/x`));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3]);
      t.eq(await p.eval(`matchMedia("(pointer: coarse)").matches`), true, "the page has a coarse pointer");
      h = await heads(p);
      t.eq([h[0].head, h[1].head, h[2].head, h[0].row, h[0].top], [47, 34, 0, 36, 47], "a coarse pointer: 47 px with a filter, 34 px with none, 0 with no header; a row is 36 px");
    },
  },
];
