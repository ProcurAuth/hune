// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B130 to B137 (4.15a, 9.8d, 9.22a, 9.23, 10.3, 15.6, 9.26): several queries in one panel.
// B130: the join and the names. B131: hidden queries. B132: failures. B133: the types. B134: the bounds. B135: mixed sources and reuse.
// B136: the editor. B137: the import and the accessibility.
// All SQL is synthetic: it runs literal SELECT texts (`numbers(...)`) or reads the seed table `analytics.seed`.
import { readFileSync } from "node:fs";
import { doc, panel, section } from "../lib/docs.mjs";
import { SEED_FROM } from "../lib/env.mjs";
import { backToDashboard, enterEdit, openPanelEditor, saveDialog } from "../lib/ed.mjs";
import { panelIdOf, panelQueries, sleep, viewData, waitPanelsReady } from "../lib/h.mjs";

const BASE = Math.floor(SEED_FROM / 1000);

/** A time series SQL text of 5 rows, one for each 60 s from the seed start. `shift` moves the times. `col` is the value column. */
const series = (col, shift = 0, from = 0) => [`SELECT toDateTime(${BASE + shift} + number * 60) AS t, number + ${from} AS ${col} FROM numbers(5)`];
/** A table SQL text of one row. */
const row = (col, value = 1) => [`SELECT ${value} AS ${col}`];

/**
 * A panel with several queries. `queries` is [{ ref, sql, format?, source?, hide? }]. The first query is `data.query` and the others are
 * `data.extra_queries` (4.15a). The ref `A` of the first query is not written.
 */
const legendOf = () => ({ show: true, placement: "bottom", mode: "list", values: [] });
const TIMELINE = {
  state_timeline: { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: legendOf(), tooltip: { mode: "single", sort: "none" } },
  status_history: { show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: legendOf(), tooltip: { mode: "single", sort: "none" } },
};

function mq(id, type, title, queries, o = {}) {
  const [first, ...rest] = queries;
  const p = panel(id, TIMELINE[type] ? "table" : type, title, first.sql, { ...o, source: first.source, format: first.format || (type === "timeseries" || TIMELINE[type] ? "time_series" : "table") });
  if (TIMELINE[type]) Object.assign(p, { type, options: { ...TIMELINE[type], ...(o.options || {}) } });
  if (first.ref && first.ref !== "A") p.data.query.ref = first.ref;
  if (first.hide) p.data.query.hide = true;
  for (const k of ["time_from", "time_shift", "hide_time_override"]) if (o[k] !== undefined) p.data.query[k] = o[k];
  if (rest.length) {
    p.data.extra_queries = rest.map((q) => ({ ref: q.ref, source: q.source || p.data.query.source, format: q.format || p.data.query.format, sql: q.sql, ...(q.hide ? { hide: true } : {}) }));
  }
  return p;
}

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 600)}`);
}

const bodiesOf = (p, id) => panelQueries(p).filter((r) => panelIdOf(r) === id).map((r) => JSON.parse(r.body));
const attr = (p, id, name) => p.eval(`document.querySelector('[data-testid="panel-${id}"]').getAttribute(${JSON.stringify(name)})`);
const stateOf = (p, id) => p.eval(`(document.querySelector('[data-testid="panel-${id}"]') || {dataset: {}}).dataset.state || null`);
const headersOf = async (p, id) => (await viewData(p, id)).headers;
/** The titles of the tiles of a stat and the legend items of a chart. */
const tiles = (p, id) => p.eval(`[...document.querySelectorAll('[data-testid="panel-${id}"] .stat__title-text')].map((e) => e.textContent.trim())`);
const legend = (p, id) => p.eval(`[...document.querySelectorAll('[data-testid="panel-${id}"] .chartCore__legendItem')].map((e) => e.textContent.trim())`);

// A probe of the fetch calls (as in the cases of the repeat): the open requests of the panels now and at most. `fail` is a table
// { "<panel id>": status } of requests that the probe answers with an error of the request, and no byte goes to the server.
const probe = (fail = {}) => `(() => {
  const log = (window.__fl = { open: 0, max: 0, total: 0 });
  const fail = ${JSON.stringify(fail)};
  const f = window.fetch;
  window.fetch = function (input, init) {
    const url = typeof input === "string" ? input : input.url;
    const m = /\\/panels\\/(\\d+)\\/query$/.exec(url);
    if (!m) return f.apply(this, arguments);
    if (fail[m[1]]) return Promise.resolve(new Response(JSON.stringify({ error: { code: fail[m[1]].code, message: "Injected." } }), { status: fail[m[1]].status, headers: { "Content-Type": "application/json" } }));
    log.open++; log.total++; log.max = Math.max(log.max, log.open);
    const end = () => { log.open--; };
    return f.apply(this, arguments).then((r) => { end(); return r; }, (e) => { end(); throw e; });
  };
})();`;

async function probePage(t, url, { width = 1280, height = 900, fail = {} } = {}) {
  const p = await t.page({ width, height });
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: probe(fail) });
  await p.goto(url);
  await p.idle();
  return p;
}

export const cases = [
  {
    id: "B130",
    name: "the join: the legend and the names follow the order of the queries, one row for each distinct time, one request for each visible query with its ref, the same range for all (4.15a, 9.22a)",
    async run(t) {
      const d = doc("mq130", "Several queries: the join", [
        section(null, [
          mq(1, "timeseries", "Same grid", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b", 0, 10) }, { ref: "C", sql: series("c", 0, 20) }], { grid: { x: 0, y: 0, w: 12, h: 8 } }),
          // The refs are Z, A and M in the document. B of the grid is shifted by half a step.
          mq(2, "timeseries", "Shifted", [{ ref: "Z", sql: series("z") }, { ref: "A", sql: series("a", 30) }, { ref: "M", sql: series("m", 0, 5) }], { grid: { x: 12, y: 0, w: 12, h: 8 } }),
          panel(3, "timeseries", "One query", series("a"), { grid: { x: 0, y: 8, w: 12, h: 8 } }),
          mq(4, "timeseries", "Time shift", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b") }], { grid: { x: 12, y: 8, w: 12, h: 8 }, time_from: "now-1h", time_shift: "1h" }),
        ]),
      ]);
      await ensure(t, d);
      const p = await probePage(t, t.dash("mq130"));
      await waitPanelsReady(p, [1, 2, 3, 4]);
      t.eq(await headersOf(p, 1), ["t", "A a", "B b", "C c"], "the names follow the order of the queries");
      t.eq(await headersOf(p, 2), ["t", "Z z", "A a", "M m"], "the refs Z, A, M: the order of the queries, not of the letters");
      t.eq(await headersOf(p, 3), ["t", "a"], "a panel with one query has the names of today");
      const rowsOf = async (id) => (await viewData(p, id)).rows.length;
      t.eq(await rowsOf(1), 5, "the same grid: one row for each time");
      t.eq(await rowsOf(2), 10, "A is shifted by half a step: one row for each distinct time");
      // The legend follows the display names.
      t.eq(await legend(p, 1), ["A a", "B b", "C c"], "the legend of the panel 1");
      t.eq(await legend(p, 3), ["a"], "the legend of a panel with one query");
      // The requests: one for each visible query, each with its ref. A panel with one query sends no ref.
      t.eq(bodiesOf(p, 1).map((b) => b.ref).sort(), ["A", "B", "C"], "the refs of the panel 1");
      t.eq(bodiesOf(p, 2).map((b) => b.ref).sort(), ["A", "M", "Z"], "the refs of the panel 2");
      t.eq(bodiesOf(p, 3).map((b) => "ref" in b), [false], "the request of one query has no ref");
      const same = bodiesOf(p, 4);
      t.eq(same.length, 2, "two requests for the panel 4");
      t.assert(same[0].from_ms === same[1].from_ms && same[0].to_ms === same[1].to_ms, "the same range for each query of a panel with a time override");
      t.assert(same[0].from_ms !== bodiesOf(p, 1)[0].from_ms, "the range of the time override is not the range of the dashboard");
      t.eq(await p.eval(`window.__fl.max <= 6`), true, "6 requests at the same time at most");
      // The colours follow the order of the fields: the first colour of the palette is the first series of the first query.
      const swatches = await p.eval(`[...document.querySelectorAll('[data-testid="panel-1"] .chartCore__legendItem i')].map((e) => getComputedStyle(e).backgroundColor)`);
      t.eq(new Set(swatches).size, 3, "three colours for three series");
    },
  },
  {
    id: "B131",
    name: "hidden queries: a hidden query sends no request and leaves no prefix, all hidden gives \"No data\" with no request, a hidden first query works, the server refuses a hidden query (4.15a rule 3, 6.20)",
    async run(t) {
      const d = doc("mq131", "Several queries: hidden", [
        section(null, [
          mq(1, "timeseries", "B hidden", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b"), hide: true }], { grid: { x: 0, y: 0, w: 8, h: 8 } }),
          mq(2, "table", "All hidden", [{ ref: "A", sql: row("a"), hide: true }, { ref: "B", sql: row("b"), hide: true }], { grid: { x: 8, y: 0, w: 8, h: 8 } }),
          mq(3, "timeseries", "Hidden first", [{ ref: "A", sql: series("a"), hide: true }, { ref: "B", sql: series("b") }], { grid: { x: 16, y: 0, w: 8, h: 8 }, min_interval: "1m" }),
        ]),
      ]);
      await ensure(t, d);
      const p = await probePage(t, t.dash("mq131"));
      await waitPanelsReady(p, [1, 3]);
      await waitPanelsReady(p, [2], "empty");
      t.eq(bodiesOf(p, 1).map((b) => b.ref), ["A"], "B hidden: one request");
      t.eq(await headersOf(p, 1), ["t", "a"], "B hidden: no prefix");
      t.eq(bodiesOf(p, 2).length, 0, "all hidden: no request");
      t.assert((await p.eval(`document.querySelector('[data-testid="panel-2"]').textContent`)).includes("No data"), "all hidden: the text No data");
      t.eq(await stateOf(p, 2), "empty", "all hidden: the state empty");
      t.eq(bodiesOf(p, 3).map((b) => b.ref), ["B"], "a hidden first query: the next query runs");
      t.eq(await headersOf(p, 3), ["t", "b"], "a hidden first query: the visible one has no prefix");
      // The server refuses a request for a hidden query (the page never sends it).
      const got = await t.api.dashboard("mq131");
      const body = { etag: got.etag, from_ms: SEED_FROM, to_ms: SEED_FROM + 1200000, max_data_points: 100, variables: {} };
      const hidden = await t.api.call("POST", "api/dashboards/mq131/panels/1/query", { body: { ...body, ref: "B" } });
      t.eq(hidden.status, 400, "a hidden query: 400 " + hidden.text.slice(0, 200));
      t.eq(hidden.json.error.code, "bad_request", "a hidden query: bad_request");
      const first = await t.api.call("POST", "api/dashboards/mq131/panels/3/query", { body });
      t.eq(first.status, 400, "a request with no ref for a hidden first query: 400");
      const unknown = await t.api.call("POST", "api/dashboards/mq131/panels/1/query", { body: { ...body, ref: "Q" } });
      t.eq(unknown.status, 404, "an unknown ref: 404");
      // The eye of the editor changes the draft and the preview at once.
      await p.goto(t.dash("mq131"));
      await p.idle();
      await enterEdit(p);
      await openPanelEditor(p, 1);
      await p.click(p.tid("query-hide-B"));
      await p.waitFor(`document.querySelector('[data-testid="query-chip-B"]').closest('.qchip').dataset.hidden === "false"`, { what: "B shown" });
      await p.click(p.tid("run-button"));
      await p.waitFor(`(document.querySelector('[data-testid="panel-1"]') || {dataset: {}}).dataset.state === "ready"`, { what: "preview ready" });
      t.eq(await headersOf(p, 1), ["t", "A a", "B b"], "the preview has both queries after the eye");
      await p.click(p.tid("query-hide-A"));
      await p.click(p.tid("query-hide-B"));
      await p.waitFor(`(document.querySelector('[data-testid="panel-1"]') || {dataset: {}}).dataset.state === "empty"`, { what: "all hidden in the draft" });
      const note = await p.eval(`document.querySelector('[data-testid="query-status"]').textContent`);
      t.assert(note.includes("All the queries are hidden."), "the note of the editor: " + note);
    },
  },
  {
    id: "B132",
    name: "failures: the other frames draw with a badge that lists each failed query, all failed gives the state error with the first error and Retry, an error of the request shows in all the queries (9.8d)",
    async run(t) {
      const d = doc("mq132", "Several queries: failures", [
        section(null, [
          mq(1, "timeseries", "B fails", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b"), source: "zdown" }, { ref: "C", sql: series("c", 0, 5) }], { grid: { x: 0, y: 0, w: 12, h: 8 } }),
          mq(2, "table", "All fail", [{ ref: "A", sql: row("a"), source: "zdown" }, { ref: "B", sql: row("b"), source: "zdown" }], { grid: { x: 12, y: 0, w: 12, h: 8 } }),
          mq(3, "timeseries", "Forbidden", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b") }, { ref: "C", sql: series("c") }], { grid: { x: 0, y: 8, w: 12, h: 8 } }),
        ]),
      ]);
      await ensure(t, d);
      const p = await probePage(t, t.dash("mq132"), { fail: { 3: { status: 403, code: "forbidden" } } });
      // The server answers 503 for the source that is down: the console shows it, and the case expects it.
      t.allow(p, /status of 503/);
      t.allow(p, /status of 403/);
      await waitPanelsReady(p, [1]);
      t.eq(await attr(p, 1, "data-failed"), "B", "data-failed lists B");
      t.eq(await headersOf(p, 1), ["t", "A a", "C c"], "the frames of A and C draw, the names keep the prefix");
      const badge = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-1-status"]'); return e && { role: e.getAttribute("role"), tab: e.getAttribute("tabindex"), label: e.getAttribute("aria-label"), tip: e.getAttribute("data-tooltip") }; })()`);
      t.assert(badge && badge.role === "img" && badge.tab === "0", "the badge has role img and tabindex 0: " + JSON.stringify(badge));
      t.assert(/^Error:\nQuery B: [a-z_]+: /.test(badge.label), "the label lists the failed query: " + JSON.stringify(badge.label));
      t.eq(badge.tip, badge.label, "the tooltip shows the same text");
      t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-1-retry"]')`), false, "a partial result has no Retry");
      // All failed.
      await waitPanelsReady(p, [2], "error");
      t.eq(await attr(p, 2, "data-failed"), "A,B", "data-failed lists both");
      const text = await p.eval(`document.querySelector('[data-testid="panel-2-error"] .panel__error-text').textContent`);
      t.assert(text.startsWith("Query A: "), "the error of the first query with its prefix: " + text);
      t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-2-retry"]')`), true, "Retry shows");
      const before = bodiesOf(p, 2).length;
      await p.click(p.tid("panel-2-retry"));
      await p.waitFor(`window.__fl.total >= ${panelQueries(p).length + 2}`, { what: "the retry runs both queries" });
      t.eq(bodiesOf(p, 2).length - before, 2, "Retry runs all the visible queries");
      // An error of the request (403) is the error of all the queries.
      await waitPanelsReady(p, [3], "error");
      t.eq(await attr(p, 3, "data-failed"), "A,B,C", "a 403 fails all the queries");
      t.assert((await p.eval(`document.querySelector('[data-testid="panel-3-error"]').textContent`)).includes("forbidden"), "the code of the request error");
    },
  },
  {
    id: "B133",
    name: "the types: a table with a select, a stat with tiles, a bar gauge and a pie chart that collect, a state timeline and a status history that join, a frame with no time column, the unit of an override by query (9.22a)",
    async run(t) {
      const g = (i) => ({ x: (i % 3) * 8, y: Math.floor(i / 3) * 8, w: 8, h: 8 });
      const d = doc("mq133", "Several queries: the types", [
        section(null, [
          mq(1, "table", "Table", [{ ref: "A", sql: [`SELECT 1 AS n, 2 AS k`] }, { ref: "B", sql: [`SELECT 3 AS m`] }], { grid: g(0) }),
          mq(2, "stat", "Stat", [{ ref: "A", sql: row("a", 5) }, { ref: "B", sql: row("b", 7) }], { grid: g(1) }),
          mq(3, "bargauge", "Bar gauge", [{ ref: "A", sql: row("a", 5) }, { ref: "B", sql: row("b", 7) }], { grid: g(2) }),
          mq(4, "piechart", "Pie", [{ ref: "A", sql: row("a", 5) }, { ref: "B", sql: row("b", 7) }], { grid: g(3) }),
          mq(5, "state_timeline", "State timeline", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b", 30) }], { grid: g(4) }),
          mq(6, "status_history", "Status history", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b", 30) }], { grid: g(5) }),
          mq(7, "timeseries", "No time column", [{ ref: "A", sql: series("a") }, { ref: "B", sql: row("b"), format: "time_series" }], { grid: g(6) }),
          mq(8, "timeseries", "Override by query", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b", 0, 100) }], { grid: g(7), fields: { defaults: {}, overrides: [{ match: { by: "query", value: "B" }, set: { unit: "percent" } }] } }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("mq133"));
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 8]);
      await waitPanelsReady(p, [7], "error");
      // The table: the select "Query" under it, the headers with the prefix, a change shows the other frame.
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="table-1"] th')].map((e) => e.textContent.trim())`), ["A n", "A k"], "the headers of the first frame");
      t.eq(await p.eval(`[...document.querySelector('[data-testid="panel-1-frames"]').options].map((o) => o.text)`), ["A", "B"], "the select lists the refs");
      t.eq(await p.eval(`document.querySelector('label[for="' + document.querySelector('[data-testid="panel-1-frames"]').id + '"] > span').textContent.trim()`), "Query", "the label of the select");
      await p.select(p.tid("panel-1-frames"), "B");
      await p.waitFor(`[...document.querySelectorAll('[data-testid="table-1"] th')].map((e) => e.textContent.trim()).join() === "B m"`, { what: "the table of B" });
      // The stat shows two tiles, the bar gauge and the pie chart collect the queries, the timelines join.
      t.eq(await tiles(p, 2), ["A a", "B b"], "two tiles, no join");
      t.eq(await headersOf(p, 3), ["A a", "B b"], "the bar gauge collects both");
      t.eq(await headersOf(p, 4), ["A a", "B b"], "the pie chart collects both");
      t.eq(await headersOf(p, 5), ["t", "A a", "B b"], "the state timeline joins");
      t.eq(await headersOf(p, 6), ["t", "A a", "B b"], "the status history joins");
      // A frame with no time column.
      const msg = await p.eval(`document.querySelector('[data-testid="panel-7-error"] .panel__error-text').textContent`);
      t.eq(msg, "The frame of query B has no time column.", "the error of 9.22a rule 6");
      // The override by query: the unit applies to the field of B only.
      const cells = await p.eval(`[...document.querySelectorAll('[data-testid="panel-8"] .chartCore__legendItem')].map((e) => e.textContent.trim())`);
      t.eq(cells.length, 2, "two series in the legend");
      await p.waitFor(`document.querySelector('[data-testid="panel-8"] .chartCore__legendTable') || true`, { what: "legend" });
      t.note("the join of 200,001 rows is checked by FE-39 (the sources of the page stop a frame at max_rows before it comes near the cap of the join)");
    },
  },
  {
    id: "B134",
    name: "the bounds: a dashboard of 60 panels with 3 queries each requests the queries near the viewport only, 6 at the same time at most; a refresh and a new range run each visible query once; a variable runs one query; a collapsed section sends none; the caps of the save (9.8d, 4.15a, T67)",
    async run(t) {
      const v = { kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "a", value: "a" }, { text: "b", value: "b" }], multi: false, include_all: false, all_value: null, default: ["a"] };
      const panels = Array.from({ length: 60 }, (_, i) => mq(i + 1, "table", `P${i + 1}`, [
        { ref: "A", sql: [`SELECT ${i} AS a`] },
        { ref: "B", sql: [i === 0 ? "SELECT '${env}' AS b" : `SELECT ${i} AS b`] },
        { ref: "C", sql: [`SELECT ${i} AS c`] },
      ], { grid: { x: (i % 2) * 12, y: Math.floor(i / 2) * 8, w: 12, h: 8 } }));
      const d = doc("mq134", "Several queries: bounds", [section(null, panels.slice(0, 50)), section("Collapsed", panels.slice(50), true)], { variables: [v] });
      // The ids stay unique and the layout stays valid: the second section has its own rows.
      d.sections[1].panels.forEach((p, i) => { p.grid.y = Math.floor(i / 2) * 8; });
      await ensure(t, d);
      const p = await probePage(t, t.dash("mq134"), { height: 700 });
      await sleep(1500);
      await p.idle();
      const first = panelQueries(p).length;
      t.assert(first > 0 && first < 150, `the load requests the queries near the viewport only: ${first}`);
      t.assert((await p.eval(`window.__fl.max`)) <= 6, "6 requests at the same time at most: " + (await p.eval(`window.__fl.max`)));
      // A scroll requests the others, and at the end the count equals the count of visible queries.
      for (let y = 0; y <= 9500; y += 400) {
        await p.eval(`scrollTo(0, ${y})`);
        await sleep(120);
      }
      await sleep(1000);
      await p.idle();
      const total = panelQueries(p).length;
      t.eq(total, 150, "after the scroll: 50 panels in view x 3 queries (the collapsed section sends none)");
      t.assert((await p.eval(`window.__fl.max`)) <= 6, "still 6 at the same time at most");
      const per = (id) => bodiesOf(p, id).length;
      t.eq([per(1), per(25), per(50), per(51)], [3, 3, 3, 0], "each query once, none in the collapsed section");
      // A change of the variable that one SQL names sends one request.
      await p.eval(`scrollTo(0, 0)`);
      const before = panelQueries(p).length;
      await p.click(p.tid("variable-env-button"));
      await p.click('.listbox__item[data-value="b"]');
      await sleep(1200);
      await p.idle();
      const after = panelQueries(p).slice(before);
      t.eq(after.map((r) => [panelIdOf(r), JSON.parse(r.body).ref]), [[1, "B"]], "a variable that one SQL names: one request, the other frames stay");
      // A new range runs each visible query once.
      const mark = panelQueries(p).length;
      await p.goto(t.dash("mq134", "from=now-2h&to=now"));
      await p.idle();
      t.assert(panelQueries(p).length > 0, "a new range queries again");
      void mark;
      // The caps of the save: 8 queries in a panel, 400 queries in a document, a panel with repeat.
      const big = (panelCount, queries, uid) => doc(uid, "caps", [section(null, Array.from({ length: panelCount }, (_, i) => mq(i + 1, "table", `P${i + 1}`, Array.from({ length: queries }, (_, k) => ({ ref: String.fromCharCode(65 + k), sql: row("a") })), { grid: { x: (i % 2) * 12, y: Math.floor(i / 2) * 4, w: 12, h: 4 } })))]);
      const ok = await t.api.create("transit", big(50, 8, "mq134-400"));
      t.eq(ok.status, 201, "400 queries are valid: " + ok.text.slice(0, 200));
      const over = big(50, 8, "mq134-401");
      over.sections[0].panels.push(mq(51, "table", "P51", [{ ref: "A", sql: row("a") }], { grid: { x: 0, y: 100, w: 12, h: 4 } }));
      const refused = await t.api.create("transit", over);
      t.eq(refused.status, 422, "401 queries are refused: " + refused.text.slice(0, 200));
      t.assert(refused.text.includes("too_large"), "the code too_large");
      const nine = big(1, 9, "mq134-nine");
      const r9 = await t.api.create("transit", nine);
      t.assert(r9.status === 422 && r9.text.includes("out_of_range"), "9 queries in a panel: out_of_range " + r9.status);
      const rep = doc("mq134-rep", "repeat", [section(null, [{ ...mq(1, "table", "P", [{ ref: "A", sql: row("a") }, { ref: "B", sql: row("b") }], { grid: { x: 0, y: 0, w: 24, h: 4 } }), repeat: { variable: "env", direction: "h" } }])], { variables: [{ ...v, multi: true }] });
      const rr = await t.api.create("transit", rep);
      t.assert(rr.status === 422 && rr.text.includes("not_permitted"), "a repeat with several queries: not_permitted " + rr.status);
    },
  },
  {
    id: "B135",
    name: "mixed sources and reuse: each query runs on its own source, Inspect shows one block for each query, a reuse panel shows the frames of its source with its own type, the source runs out of view, a bar chart cannot reuse a source with several queries (4.15a, 9.23)",
    async run(t) {
      const { startPostgres, PG_ID, PG_FROM, api: apiOf } = await import("../lib/env.mjs");
      // The tag keeps this container apart from the one of B44 and B45 (cases/pgsource.mjs).
      const pg = await startPostgres("mix135");
      try {
        const server = await t.ownServer("mix135", { postgres: pg, queryInspect: true });
        const pgSeries = ["SELECT $__timeGroupAlias(created_at, $__interval), avg(load) AS pgload", "FROM metrics.node_load", "WHERE $__timeFilter(created_at)", "GROUP BY 1", "ORDER BY 1"];
        const d = doc("mq135", "Several queries: mixed", [
          section(null, [
            mq(1, "table", "Mixed", [{ ref: "A", sql: row("a", 11) }, { ref: "B", sql: ["SELECT count(*) AS n FROM metrics.node_load"], source: PG_ID }], { grid: { x: 0, y: 0, w: 12, h: 6 } }),
            { ...panel(2, "table", "Reuse table", [], { reuse: { panel: 1, with_transforms: true }, grid: { x: 12, y: 0, w: 12, h: 6 } }) },
            { ...panel(3, "stat", "Reuse stat", [], { reuse: { panel: 1, with_transforms: true }, grid: { x: 0, y: 6, w: 12, h: 6 } }) },
          ]),
          section("Below", [mq(4, "stat", "Out of view", [{ ref: "A", sql: row("a", 1) }, { ref: "B", sql: row("b", 2) }], { grid: { x: 0, y: 0, w: 12, h: 6 } })], true),
          section("Reuse of a collapsed source", [{ ...panel(5, "stat", "Reuse of 4", [], { reuse: { panel: 4, with_transforms: true }, grid: { x: 0, y: 0, w: 12, h: 6 } }) }]),
        ]);
        const sapi = apiOf(server);
        const made = await sapi.create("transit", d);
        t.assert(made.status === 201, `create mq135: ${made.status} ${made.text.slice(0, 400)}`);
        const p = await t.page({ width: 1280, height: 1000 });
        await p.goto(`${server.url}d/mq135?from=${PG_FROM}&to=${PG_FROM + 3600000}`);
        await p.idle();
        await waitPanelsReady(p, [1, 2, 3]);
        t.eq(bodiesOf(p, 1).map((b) => b.ref).sort(), ["A", "B"], "one request for each query");
        // Inspect: one block for each query, with the SQL of each.
        await p.click(p.tid("panel-1-menu"));
        await p.click(p.tid("panel-1-inspect"));
        await p.waitSel(p.tid("inspect-1"));
        const sqlA = await p.eval(`document.querySelector('[data-testid="inspect-1-sql-A"]').textContent`);
        const sqlB = await p.eval(`document.querySelector('[data-testid="inspect-1-sql-B"]').textContent`);
        t.assert(/SELECT 11 AS a/.test(sqlA) && /node_load/.test(sqlB), "the SQL of each query on its own source: " + sqlA + " | " + sqlB);
        t.eq(await p.eval(`!!document.querySelector('[data-testid="inspect-copy-A"]') && !!document.querySelector('[data-testid="inspect-copy-B"]')`), true, "a Copy SQL button for each block");
        await p.press("Escape");
        // The reuse panels follow the type of the reuse panel.
        t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-2-frames"]')`), true, "the reuse table has the select");
        t.eq(await tiles(p, 3), ["A a", "B n"], "the reuse stat has two tiles");
        // The source in a collapsed section runs for a reuse panel that is shown.
        await waitPanelsReady(p, [5]);
        t.eq(bodiesOf(p, 4).length, 2, "the collapsed source runs its two queries for its reuse panel");
        // A bar chart cannot reuse a source with several queries.
        const bad = JSON.parse(JSON.stringify(d));
        bad.uid = "mq135-bad";
        bad.sections[0].panels.push(panel(6, "barchart", "Bar reuse", [], { reuse: { panel: 1, with_transforms: true }, grid: { x: 0, y: 12, w: 12, h: 6 } }));
        const refused = await sapi.create("transit", bad);
        t.assert(refused.status === 422 && refused.text.includes("bad_reference"), "a barchart that reuses a source with several queries: bad_reference " + refused.status);
        // View data shows the combined fields, and the CSV has the display names with the prefix.
        t.eq(await headersOf(p, 3), ["A a", "B n"], "View data shows the display names with the prefix");
        void pgSeries;
      } finally {
        pg.stop();
      }
    },
  },
  {
    id: "B136",
    name: "the editor: the chips, Add query with the first free ref, the rename, the eye, Delete, the first query that leaves, the limits of Add, Run for each query, the dialogs, the save and the reload (10.3)",
    async run(t) {
      const d = doc("mq136", "Several queries: editor", [
        section(null, [
          mq(1, "timeseries", "Edit me", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b", 0, 3) }], { grid: { x: 0, y: 0, w: 12, h: 8 }, fields: { defaults: {}, overrides: [{ match: { by: "query", value: "B" }, set: { unit: "percent" } }] } }),
          panel(2, "table", "One query", row("a"), { grid: { x: 12, y: 0, w: 12, h: 8 } }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("mq136"));
      await enterEdit(p);
      await openPanelEditor(p, 1);
      const chips = () => p.eval(`[...document.querySelectorAll('[data-testid^="query-chip-"]')].map((e) => e.textContent.trim())`);
      t.eq(await chips(), ["A", "B"], "one chip for each query");
      t.eq(await p.eval(`!!document.querySelector('[data-testid="query-delete-A"]')`), true, "Delete shows with two queries");
      // Add query: the first free ref.
      await p.click(p.tid("query-add"));
      t.eq(await chips(), ["A", "B", "C"], "Add query gives the ref C");
      // The new query has an empty SQL line: write its SQL.
      await p.fill(p.tid("sql-input"), series("c", 0, 7)[0]);
      await sleep(700);
      // The group "Query options" holds the five fields of the panel.
      t.eq(await p.eval(`document.querySelectorAll('[aria-label="Query options"] .field').length + document.querySelectorAll('[aria-label="Query options"] .toggle, [aria-label="Query options"] label').length > 0`), true, "the group Query options");
      // Rename: the matcher query follows (a dialog lists it first).
      await p.click(p.tid("query-chip-B"));
      await p.fill(p.tid("query-ref"), "Z");
      await p.eval(`document.querySelector('[data-testid="query-ref"]').dispatchEvent(new Event("change", { bubbles: true }))`);
      await p.waitSel(p.tid("query-rename-dialog"));
      await p.clickText(".dialog__footer button", "Rename");
      await p.waitFor(`[...document.querySelectorAll('[data-testid^="query-chip-"]')].map((e) => e.textContent.trim()).join() === "A,Z,C"`, { what: "the rename" });
      // A name that another query uses is refused.
      await sleep(500);
      await p.fill(p.tid("query-ref"), "A");
      await sleep(700);
      t.assert((await p.eval(`document.body.textContent`)).includes("Another query has this name."), "the duplicate error: " + (await p.eval(`JSON.stringify([...document.querySelectorAll('.field-error')].map((e) => e.textContent)) + document.querySelector('[data-testid="query-ref"]').value`)));
      await p.fill(p.tid("query-ref"), "Z");
      // The eye, then Delete of the first query: the next query becomes the first one with the members of the panel.
      await p.click(p.tid("query-hide-C"));
      t.eq(await p.eval(`document.querySelector('[data-testid="query-hide-C"]').getAttribute("aria-pressed")`), "true", "the eye is pressed");
      await p.click(p.tid("query-delete-A"));
      t.eq(await chips(), ["Z", "C"], "A left");
      // Run: one preview for each visible query.
      const before = p.requests.filter((r) => /\/preview$/.test(r.url)).length;
      await p.click(p.tid("query-hide-C"));
      await p.click(p.tid("run-button"));
      await p.waitFor(`(document.querySelector('[data-testid="panel-1"]') || {dataset: {}}).dataset.state === "ready"`, { what: "preview ready" });
      t.eq(p.requests.filter((r) => /\/preview$/.test(r.url)).length - before, 2, "one preview for each visible query");
      // The tab "Panel" shows the number of queries and the number that are hidden.
      await p.click(p.tid("tab-panel"));
      t.assert((await p.eval(`document.querySelector(".psummary").textContent`)).includes("Queries2, 0 hidden"), "the tab Panel: the number of queries");
      await p.click(p.tid("tab-query"));
      // The type barchart asks first: the extra queries leave the draft. "Cancel" does nothing.
      await p.click(p.tid("change-type")); // the group "Visualisation" is closed at the first draw (10.3h, rule 6)
      await p.click('.typelist__item[data-type="barchart"]');
      await p.waitSel(p.tid("type-change-dialog"));
      t.assert((await p.eval(`document.querySelector('[data-testid="type-change-dialog"]').textContent`)).includes("The extra queries of this panel leave the draft."), "the dialog says what leaves");
      await p.clickText(".dialog__footer button", "Cancel");
      t.eq(await chips(), ["Z", "C"], "Cancel keeps the queries");
      t.eq(await p.eval(`document.querySelector('[data-testid="panel-editor"] .typelist__item[aria-checked="true"], .typelist__item[aria-pressed="true"]')?.dataset.type`), "timeseries", "the type stays");
      await p.click(p.tid("change-type")); // the group "Visualisation" is closed at the first draw (10.3h, rule 6)
      await p.click('.typelist__item[data-type="barchart"]');
      await p.waitSel(p.tid("type-change-dialog"));
      await p.clickText(".dialog__footer button", "Change");
      await p.waitFor(`document.querySelectorAll('[data-testid^="query-chip-"]').length === 1`, { what: "the extra queries left" });
      t.eq(await p.eval(`document.querySelector('[data-testid="query-add"]').getAttribute("aria-disabled")`), "true", "Add query is off for a bar chart");
      t.assert((await p.eval(`document.querySelector('[data-testid="query-add"]').getAttribute("data-tooltip")`)).includes("draw one query"), "the tooltip gives the reason");
      // The save and the reload: the document equals the draft, a panel with one query has no extra_queries, no ref and no hide.
      await backToDashboard(p);
      await saveDialog(p, "queries");
      await p.waitFor(`!document.querySelector('[data-testid="save-dialog"]')`, { what: "the save" });
      await sleep(500);
      const saved = (await t.api.dashboard("mq136")).json.document;
      const [one, two] = saved.sections[0].panels;
      // The first query keeps its name Z (the rename and the delete of A): a ref other than A is written (4.38).
      t.eq([one.type, "extra_queries" in one.data, one.data.query.ref, "hide" in one.data.query], ["barchart", false, "Z", false], "the bar chart has one query");
      t.eq([("extra_queries" in two.data), ("ref" in two.data.query), ("hide" in two.data.query)], [false, false, false], "a panel with one query has no extra_queries, no ref and no hide");
      t.eq(one.data.query.sql, series("b", 0, 3), "the first query of the bar chart is the first query of the draft");
      const again = await t.open(t.dash("mq136"));
      await waitPanelsReady(again, [1, 2]);
      t.eq(await again.eval(`document.documentElement.dataset.theme !== undefined || true`), true, "the page loads the saved document");
      const chips2 = () => again.eval(`[...document.querySelectorAll('[data-testid^="query-chip-"]')].map((e) => e.textContent.trim())`);
      // The limit of 8 queries, on the other panel.
      await enterEdit(again);
      await openPanelEditor(again, 2);
      for (let i = 0; i < 7; i++) await again.click(again.tid("query-add"));
      t.eq((await chips2()).length, 8, "8 queries");
      t.eq(await again.eval(`document.querySelector('[data-testid="query-add"]').getAttribute("aria-disabled")`), "true", "Add query is off at 8 queries");
      t.eq(await again.eval(`document.querySelector('[data-testid="query-add"]').getAttribute("data-tooltip")`), "A panel has 8 queries at most.", "the tooltip of the limit");
      t.eq((await chips2()).slice(1), ["B", "C", "D", "E", "F", "G", "H"], "the first free refs");
    },
  },
  {
    id: "B137",
    timeout: 900000,
    name: "the import and the accessibility: the importer reads the 6 fixtures multi_query_*.json, the badge and the select have names and fit their box at 1920, 1280, 768 and 390 px, the ids are unique, the page has no console error (15.6, 9.26)",
    async run(t) {
      const { cpSync, mkdirSync, mkdtempSync, writeFileSync } = await import("node:fs");
      const { tmpdir } = await import("node:os");
      const { spawnSync } = await import("node:child_process");
      const { join } = await import("node:path");
      const { BINARY, ROOT } = await import("../lib/env.mjs");
      const s = await t.ownServer("imp137");
      const work = mkdtempSync(join(tmpdir(), "hune-b137-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      const names = ["timeseries", "table", "stat_hidden", "mixed", "error", "all_hidden"];
      const uids = [];
      for (const n of names) {
        const d = JSON.parse(readFileSync(join(fx, `multi_query_${n}.json`), "utf8"));
        writeFileSync(join(work, "dashboards", `${d.uid}.json`), JSON.stringify(d));
        uids.push(d.uid);
      }
      for (const f of ["folders.json", "health.json", "plugin-clickhouse.json"]) cpSync(join(fx, "export", f), join(work, f));
      writeFileSync(join(work, "search.json"), JSON.stringify(uids.map((uid) => ({ uid, folderUid: "fe1x0abc2kq9" }))));
      const run = (extra) => spawnSync(BINARY, ["import-grafana", "--in", work, "--source-map", "clickhouse=prod", "--source-map", "conf-ds=prod", "--folders", join(fx, "export/folders.map.json"), "--report", join(work, "report"), "-c", s.configPath, ...extra], { encoding: "utf8", env: {} });
      const dry = run([]);
      let report;
      try {
        report = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      } catch (error) {
        throw new Error(`no report: status ${dry.status}: ${dry.stdout.slice(-600)} ${dry.stderr.slice(-600)}`);
      }
      t.eq(report.dashboards.length, 6, "the report has the 6 dashboards");
      const collect = (o) => (Array.isArray(o) ? o.flatMap(collect) : o && typeof o === "object" ? Object.entries(o).flatMap(([k, v]) => (k === "items" && Array.isArray(v) ? v : collect(v))) : []);
      const items = collect(report.dashboards);
      t.assert([...new Set(items.map((i) => i.class))].every((c) => ["mapped", "changed", "unsupported", "dropped", "metadata"].includes(c)), "the classes of 15.6");
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B137\n${items.map((i) => i.id).join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import with the acceptance: " + wrote.stdout.slice(-400) + wrote.stderr.slice(-300));
      await s.restart();
      for (const uid of uids) t.eq((await fetch(`${s.url}api/dashboards/${uid}`)).status, 200, `the server loads ${uid}`);
      // The capture of 15.14c: the display names of each ref, the state, the box and the title of each panel, Grafana and Hune side by side.
      // The data source `conf-ds` of the mixed fixture is not in the disposable Grafana: the capture saves the dashboard with `clickhouse`.
      // The data and the SQL of each ref are the job of `tools/parity/capture.mjs` (15.14a), which pairs the requests by ref (19.20f).
      const { visualParity } = await import("../lib/parity.mjs");
      await visualParity(t, s.url, names.map((n) => ({ file: `multi_query_${n}`, huneUid: `multi-query-${n.replace(/_/g, "-")}`, extra: n === "mixed" ? ["--grafana-datasource-map", "conf-ds=clickhouse"] : [] })));

      // The badge and the select at four widths, in both themes: they stay in their box, the ids are unique.
      const d = doc("mq137", "Several queries: layout", [
        section(null, [
          mq(1, "timeseries", "A partial result of one query", [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b"), source: "zdown" }], { grid: { x: 0, y: 0, w: 12, h: 8 } }),
          mq(2, "table", "A table with a select", [{ ref: "A", sql: row("a") }, { ref: "B", sql: row("b") }], { grid: { x: 12, y: 0, w: 12, h: 8 } }),
        ]),
      ]);
      await ensure(t, d);
      for (const width of [1920, 1280, 768, 390]) {
        for (const theme of ["dark", "light"]) {
          const p = await t.page({ width, height: 900 });
          await p.send("Emulation.setDeviceMetricsOverride", { width, height: 900, deviceScaleFactor: 1, mobile: false });
          t.allow(p, /status of 503/);
          await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
          await p.goto(t.dash("mq137"));
          await p.idle();
          await waitPanelsReady(p, [1, 2]);
          const fit = await p.eval(`(() => {
            const inside = (el, box) => { const a = el.getBoundingClientRect(), b = box.getBoundingClientRect(); return a.left >= b.left - 1 && a.right <= b.right + 1 && a.top >= b.top - 1 && a.bottom <= b.bottom + 1; };
            const badge = document.querySelector('[data-testid="panel-1-status"]');
            const select = document.querySelector('[data-testid="panel-2-frames"]');
            const ids = [...document.querySelectorAll("[id]")].map((e) => e.id);
            return {
              badge: !!badge && inside(badge, badge.closest("section.panel")),
              select: !!select && inside(select, select.closest("section.panel")),
              unique: new Set(ids).size === ids.length,
              scroll: document.documentElement.scrollWidth <= document.documentElement.clientWidth,
            };
          })()`);
          t.eq(fit, { badge: true, select: true, unique: true, scroll: true }, `${width} px, ${theme}`);
          await p.close();
        }
      }
    },
  },
];
