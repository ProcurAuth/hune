// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B04, B05, B06, B07, B08, B09, B11 (16.10).
import { doc, panel, section, SQL } from "../lib/docs.mjs";
import { SEED_FROM, SEED_TO } from "../lib/env.mjs";
import { optionQueries, panelIdOf, panelQueries, panelStates, sleep, viewData, waitPanelsReady } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const rgb = (s) => (/rgba?\((\d+), (\d+), (\d+)/.exec(s) || []).slice(1, 4).map(Number);

export const cases = [
  {
    id: "B04",
    name: "table overrides by name and by regular expression, each property; reduce.fields /^Total$/",
    async run(t) {
      const sql = ["SELECT node, weight, blocks, nullIf(blocks, 3) AS nb, weight * 2 AS Total, active", "FROM analytics.seed", "ORDER BY event_time"];
      const steps = (a, b) => ({ mode: "absolute", steps: [{ value: null, color: "green" }, { value: a, color: "orange" }, { value: b, color: "red" }] });
      const d = doc("overrides-test", "Overrides", [
        section(null, [
          panel(1, "table", "Overrides", sql, {
            grid: { x: 0, y: 0, w: 24, h: 10 },
            fields: {
              defaults: { no_value: "n/a" },
              overrides: [
                { match: { by: "name", value: "weight" }, set: { unit: "suffix: pts", decimals: 2, width: 140, align: "center" } },
                { match: { by: "regex", value: "weig" }, set: { decimals: 5 } }, // plain text: exact match, so no match
                {
                  match: { by: "name", value: "blocks" },
                  set: {
                    thresholds: steps(10, 20), min: 0, max: 29, color: { mode: "thresholds", fixed: null },
                    cell: { type: "color_background", mode: "basic" },
                    mappings: [{ kind: "value", value: "0", text: "Zero", color: "blue" }],
                  },
                },
                { match: { by: "regex", value: "/^Total$/" }, set: { display_name: "Doubled", unit: "suffix: X", decimals: 1, cell: { type: "color_text", mode: null }, color: { mode: "fixed", fixed: "red" } } },
                { match: { by: "name", value: "node" }, set: { width: 120, align: "left", filterable: true, links: [{ title: "Open", url: "https://example.org/v/${__value.raw}", target_blank: true }] } },
                { match: { by: "name", value: "active" }, set: { hidden: true } },
              ],
            },
          }),
          panel(2, "stat", "Total only", ["SELECT sum(weight) AS Total, count() AS Rows", "FROM analytics.seed"], {
            grid: { x: 0, y: 10, w: 8, h: 4 },
            options: { reduce: { calc: "last", fields: "/^Total$/", values: false }, color_mode: "none", graph_mode: "none" },
          }),
          panel(3, "stat", "Rows only", ["SELECT sum(weight) AS Total, count() AS Rows", "FROM analytics.seed"], {
            grid: { x: 8, y: 10, w: 8, h: 4 },
            options: { reduce: { calc: "last", fields: "Rows", values: false }, color_mode: "none", graph_mode: "none", text_mode: "value_and_name" },
          }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("overrides-test"));
      await waitPanelsReady(p, [1, 2, 3]);
      const m = await p.eval(`(() => {
        const t = document.querySelector('[data-testid="table-1"] table');
        const heads = [...t.querySelectorAll("th")].map((h) => ({ name: h.dataset.field, text: h.textContent.trim(), align: h.style.textAlign, w: h.getBoundingClientRect().width }));
        const rows = [...t.querySelectorAll("tbody tr")].map((r) => [...r.children].map((c) => ({ text: c.textContent, bg: c.style.background, color: c.style.color, align: c.style.textAlign, link: c.querySelector("a") ? { href: c.querySelector("a").getAttribute("href"), rel: c.querySelector("a").rel } : null })));
        return { heads, rows, text: document.querySelector('[data-testid="panel-1"]').textContent };
      })()`);
      t.eq(m.heads.map((h) => h.text).join("|"), "node|weight|blocks|nb|Doubled", "headers: hidden column gone, display_name applied");
      const col = Object.fromEntries(m.heads.map((h, i) => [h.name, i]));
      const cell = (r, name) => m.rows[r][col[name]];
      t.eq(cell(0, "weight").text, "0.50 pts", "unit and decimals by name (plain-text regex did not apply)");
      t.eq(cell(0, "weight").align, "center", "align");
      t.assert(Math.abs(m.heads[col.weight].w - 140) < 1.5, "width 140: " + m.heads[col.weight].w);
      t.assert(Math.abs(m.heads[col.node].w - 120) < 1.5, "width 120: " + m.heads[col.node].w);
      t.eq(cell(0, "blocks").text, "Zero", "value mapping text");
      t.eq(cell(3, "nb").text, "n/a", "no_value of the defaults");
      t.eq(cell(0, "Doubled").text, "1.0 X", "regex override: display_name, unit, decimals");
      t.assert(cell(0, "Doubled").color !== "", "color_text sets a text colour");
      t.eq(cell(0, "Doubled").color, cell(9, "Doubled").color, "fixed colour: same for all cells");
      t.eq(cell(0, "node").link.href, "https://example.org/v/v0", "link in the node column");
      // thresholds as a background colour: 5 green, 15 orange, 25 red, 0 mapped to blue
      const bgs = [5, 15, 25, 0].map((r) => rgb(cell(r, "blocks").bg));
      const [g, o, rd, bl] = bgs;
      t.assert(g[1] > g[0] && g[1] > g[2], "5 is green: " + g);
      t.assert(o[0] > o[2] && o[1] > o[2] && o[0] >= o[1], "15 is orange: " + o);
      t.assert(rd[0] > rd[1] && rd[0] > rd[2], "25 is red: " + rd);
      t.assert(bl[2] > bl[0], "0 is blue (mapping colour): " + bl);
      // the filter button exists for the filterable column only
      t.eq(await p.count('[data-testid^="filter-1-"]'), 1, "one filter button");
      // reduce.fields as /^Total$/ shows the field Total only; "Rows" is an exact plain match
      t.eq(await p.count('[data-testid^="stat-item-2"]'), 1, "stat 2 has one item");
      t.eq((await p.text('[data-testid="stat-item-2"] .statTile__value')).trim(), "450", "stat 2 shows Total");
      t.eq((await p.text('[data-testid="stat-item-3"] .statTile__value')).trim(), "30", "stat 3 shows Rows (plain text = exact)");
    },
  },

  {
    id: "B05",
    name: "table sort in three states, column filter, virtual rows with 10,000 rows",
    async run(t) {
      const d = doc("rows-test", "Rows", [
        section(null, [
          panel(1, "table", "Rows", ["SELECT number AS n, concat('k', toString(number % 5)) AS k, toString(number) AS s", "FROM numbers(10000)", "ORDER BY cityHash64(number)"], {
            grid: { x: 0, y: 0, w: 24, h: 12 },
            fields: { defaults: {}, overrides: [{ match: { by: "name", value: "k" }, set: { filterable: true } }] },
          }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("rows-test"));
      await waitPanelsReady(p, [1]);
      const T = '[data-testid="table-1"]';
      const rowsInDom = () => p.count(`${T} tbody tr`);
      const first = () => p.eval(`(() => { const r = [...document.querySelector('${T} tbody').children].find((x) => x.children.length > 1); return r.children[0].textContent; })()`);
      t.eq(await p.attr(`${T} table`, "aria-rowcount"), "10001", "rowcount");
      const n = await rowsInDom();
      t.assert(n > 5 && n < 200, `virtual rows: ${n} row elements for 10000 rows`);
      // scroll to the end: still bounded, and the last rows are there
      await p.eval(`document.querySelector('${T}').scrollTop = 1e9`);
      await p.waitFor(`(() => { const rows = document.querySelectorAll('${T} tbody tr'); return rows.length > 0 && rows.length < 200; })()`);
      await sleep(200);
      t.assert((await rowsInDom()) < 200, "bounded after scroll");
      await p.eval(`document.querySelector('${T}').scrollTop = 0`);
      await sleep(200);
      // sort: none -> asc -> desc -> none
      const initial = await first();
      const header = `${T} th[data-field="n"]`;
      const sortState = () => p.attr(header, "aria-sort");
      t.eq(await sortState(), "none", "initial aria-sort");
      await p.click(`${header} .dataTable__sort`);
      await p.waitFor(`document.querySelector('${header}').getAttribute("aria-sort") === "ascending"`);
      await sleep(150);
      t.eq(await first(), "0", "asc: first row is 0");
      await p.click(`${header} .dataTable__sort`);
      await p.waitFor(`document.querySelector('${header}').getAttribute("aria-sort") === "descending"`);
      await sleep(150);
      t.eq(await first(), "9999", "desc: first row is 9999");
      await p.click(`${header} .dataTable__sort`);
      await p.waitFor(`document.querySelector('${header}').getAttribute("aria-sort") === "none"`);
      await sleep(150);
      t.eq(await first(), initial, "no sort: the original order returns");
      // column filter: keep k0 only
      await p.click(p.tid("filter-1-1"));
      await p.waitSel(".filter-popup");
      await p.clickText(".filter-popup button", "Clear");
      await p.clickText(".filter-popup label", "k0");
      await p.clickText(".filter-popup button", "Apply");
      await p.waitGone(".filter-popup");
      await sleep(200);
      const ks = await p.eval(`[...document.querySelectorAll('${T} tbody tr')].filter((r) => r.children.length > 1).map((r) => r.children[1].textContent)`);
      t.assert(ks.length > 0 && ks.every((k) => k === "k0"), "only k0 rows: " + [...new Set(ks)]);
      t.eq(await p.attr(p.tid("filter-1-1"), "aria-pressed"), "true", "the filter button is marked");
      // the filter is page state: not in the URL
      t.assert(!/filter/i.test(await p.eval("location.href")), "filter not in the URL");
      // header cells have scope col and aria-sort (9.26)
      t.eq(await p.eval(`[...document.querySelectorAll('${T} th')].every((h) => h.scope === "col" && h.hasAttribute("aria-sort"))`), true, "th scope and aria-sort");
    },
  },

  {
    id: "B06",
    name: "tables show UInt64 maximum and Decimal(38, 9) values as exact text",
    async run(t) {
      const d = doc("exact-test", "Exact", [
        section(null, [
          panel(1, "table", "Exact", [
            "SELECT toUInt64('18446744073709551615') AS u, toInt64('-9223372036854775808') AS i,",
            "  toDecimal128('99999999999999999999999999999.999999999', 9) AS d, total AS agg",
            "FROM analytics.types_extra WHERE k = 1",
          ], {
            grid: { x: 0, y: 0, w: 24, h: 6 },
            // 9.15: the exact path rounds the exact text to d decimals, and d comes from the double.
            // The automatic d of 1e29 is 0, so the decimal column asks for 9 decimals.
            fields: { defaults: {}, overrides: [{ match: { by: "name", value: "d" }, set: { decimals: 9 } }] },
          }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("exact-test"));
      await waitPanelsReady(p, [1]);
      const cells = await p.eval(`[...document.querySelectorAll('[data-testid="table-1"] tbody tr:first-child td')].map((c) => c.textContent)`);
      t.eq(cells[0], "18446744073709551615", "UInt64 maximum");
      t.eq(cells[1], "-9223372036854775808", "Int64 minimum");
      t.eq(cells[2], "99999999999999999999999999999.999999999", "Decimal(38, 9)");
      t.eq(cells[3], "18446744073709551615", "SimpleAggregateFunction(sum, UInt64)");
      // the API frame carries the exact text, not a JSON number (D42)
      const q = p.requests.find((r) => /panels\/1\/query$/.test(r.url));
      const res = await t.api.call("POST", "api/dashboards/exact-test/panels/1/query", {
        body: JSON.parse(q.body), headers: {},
      });
      t.eq(res.status, 200, "query status");
      t.assert(res.text.includes('"18446744073709551615"') && res.text.includes('"99999999999999999999999999999.999999999"'), "frame has exact text: " + res.text.slice(0, 400));
    },
  },

  {
    id: "B07",
    name: "time picker (relative, absolute, zoom out), URL, refresh choices, no refresh while hidden",
    async run(t) {
      const p = await t.open(t.dash("nodes"));
      await waitPanelsReady(p, [1, 2, 3]);
      const search = () => p.eval("location.search");
      const params = async () => {
        const all = Object.fromEntries(new URLSearchParams(await search()));
        return { from: all.from, to: all.to };
      };
      t.eq(await params(), { from: String(SEED_FROM), to: String(SEED_TO) }, "URL holds epoch milliseconds");
      const label = () => p.text(`${p.tid("time-picker-button")} .picker__text`);
      const fmt = (ms) => p.eval(`(() => { const d = new Date(${ms}); const q = (n) => String(n).padStart(2, "0"); return d.getFullYear() + "-" + q(d.getMonth() + 1) + "-" + q(d.getDate()) + " " + q(d.getHours()) + ":" + q(d.getMinutes()) + ":" + q(d.getSeconds()); })()`);
      t.eq(await label(), `${await fmt(SEED_FROM)} to ${await fmt(SEED_TO)}`, "absolute label");
      // relative: Last 5 minutes
      p.clearRequests();
      await p.click(p.tid("time-picker-button"));
      await p.clickText(".picker__quick-item", "Last 5 minutes");
      await p.waitFor(`location.search.includes("from=now-5m")`);
      await p.idle();
      t.eq(await params(), { from: "now-5m", to: "now" }, "relative URL");
      t.eq(await label(), "Last 5 minutes", "quick range label");
      const q = panelQueries(p);
      t.assert(q.length >= 2, "panels query again after the change");
      const body = JSON.parse(q[0].body);
      t.assert(Math.abs(body.to_ms - Date.now()) < 60000 && body.to_ms - body.from_ms === 300000, "request has absolute milliseconds of now-5m");
      // absolute range through the form
      await p.click(p.tid("time-picker-button"));
      await p.fill(p.tid("time-from"), "2026-11-09 09:00:00");
      await p.fill(p.tid("time-to"), "2026-11-09 09:10:00");
      await p.click(p.tid("time-apply"));
      await p.idle();
      const ms = await p.eval(`[new Date(2026, 10, 9, 9, 0, 0).getTime(), new Date(2026, 10, 9, 9, 10, 0).getTime()]`);
      t.eq(await params(), { from: String(ms[0]), to: String(ms[1]) }, "absolute URL");
      // a field with an error, and from not below to
      await p.click(p.tid("time-picker-button"));
      await p.fill(p.tid("time-from"), "not a time");
      await p.click(p.tid("time-apply"));
      await p.waitFor(`[...document.querySelectorAll(".picker__popup .field-error")].some((e) => !e.hidden && e.textContent.length > 0)`, { what: "field error" });
      await p.fill(p.tid("time-from"), "2026-11-09 10:00:00");
      await p.click(p.tid("time-apply"));
      await sleep(200);
      t.eq(await params(), { from: String(ms[0]), to: String(ms[1]) }, "an invalid range keeps the earlier range");
      await p.press("Escape");
      // zoom out: [from - d/2, to + d/2]
      await p.click(p.tid("zoom-out"));
      await p.idle();
      const d = ms[1] - ms[0];
      t.eq(await params(), { from: String(ms[0] - d / 2), to: String(ms[1] + d / 2) }, "zoom out");
      // refresh choices
      const opts = await p.eval(`[...document.querySelectorAll('[data-testid="refresh-select"] option')].map((o) => o.value)`);
      t.eq(opts, ["off", "10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"], "refresh choices");
      // the themed menu of the split button offers the same choices; a choice sets the value and the URL
      await p.click(p.tid("refresh-interval"));
      t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitemradio"]')].map((e) => e.textContent)`), ["Off", "10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"], "menu choices");
      t.eq(await p.eval(`document.querySelector('.menu [aria-checked="true"]').textContent`), await p.eval(`document.querySelector('[data-testid="refresh-select"]').selectedOptions[0].textContent`), "the current value is checked");
      await p.clickText('.menu [role="menuitemradio"]', "30s");
      await p.waitFor(`location.search.includes("refresh=30s")`);
      t.eq(await p.eval(`document.querySelector('[data-testid="refresh-select"]').value`), "30s", "the value follows the menu");
      await p.select(p.tid("refresh-select"), "off");
      await p.waitFor(`!location.search.includes("refresh=30s")`);
      // a value that is not offered gives Off and a notice
      const p2 = await t.open(t.dash("nodes", "refresh=7s"));
      t.eq(await p2.eval(`document.querySelector('[data-testid="refresh-select"]').value`), "off", "refresh off");
      t.assert((await p2.eval(`[...document.querySelectorAll('[data-testid="notice"]')].map((e) => e.textContent).join(" ")`)).includes("The refresh value 7s is not offered"), "notice");
      // refresh 10s: a tick queries again; hidden: no tick; visible again: one refresh at once
      await p.select(p.tid("refresh-select"), "10s");
      await p.waitFor(`location.search.includes("refresh=10s")`);
      await p.idle();
      p.clearRequests();
      await p.waitFor(`true`);
      const t0 = Date.now();
      while (panelQueries(p).length === 0 && Date.now() - t0 < 16000) await sleep(200);
      t.assert(panelQueries(p).length >= 2, "a refresh tick ran the panel queries");
      await p.idle();
      await p.setHidden(true);
      p.clearRequests();
      await sleep(12500);
      t.eq(panelQueries(p).length, 0, "no query while the tab is hidden");
      await p.setHidden(false);
      await p.waitFor(`true`);
      const t1 = Date.now();
      while (panelQueries(p).length === 0 && Date.now() - t1 < 3000) await sleep(100);
      t.assert(panelQueries(p).length >= 2, "one refresh at once when visible again");
      await p.select(p.tid("refresh-select"), "off");
    },
  },

  {
    id: "B08",
    name: "variables: multi, All, search, text on Enter, custom, dependent refresh, time-change refresh, URL",
    async run(t) {
      const qv = (name, sql, o = {}) => ({
        kind: "query", name, label: null, hide: "none", source: "prod", sql, multi: true, include_all: true, all_value: null,
        refresh: "load", sort: "alpha_asc", regex: null, default: ["$__all"], ...o,
      });
      const d = doc("vars-test", "Variables", [
        section(null, [
          panel(1, "table", "Rows", [
            "SELECT node, blocks FROM analytics.seed",
            "WHERE node IN (${node}) AND ('${account}' = '' OR node = '${account}') AND ('${env}' = 'prod')",
            "ORDER BY event_time",
          ], { grid: { x: 0, y: 0, w: 24, h: 10 } }),
        ]),
      ], {
        variables: [
          qv("node", ["SELECT DISTINCT node", "FROM analytics.seed", "ORDER BY node"], { refresh: "time_change" }),
          qv("blk", ["SELECT DISTINCT toString(blocks) AS b", "FROM analytics.seed", "WHERE node IN (${node})"], { multi: false, include_all: false, sort: "num_asc", default: ["0"] }),
          qv("solo", ["SELECT DISTINCT node", "FROM analytics.seed"], { multi: false, include_all: false, default: ["v0"] }),
          { kind: "text", name: "account", label: "Account", hide: "none", value_type: "text", default: "" },
          { kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "Production", value: "prod" }, { text: "Preproduction", value: "preprod" }], multi: false, include_all: false, all_value: null, default: ["prod"] },
        ],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("vars-test"));
      await waitPanelsReady(p, [1]);
      const rows = () => p.count('[data-testid="table-1"] tbody tr');
      const params = async () => new URLSearchParams(await p.eval("location.search"));
      const btn = (n) => p.text(`${p.tid(`variable-${n}-button`)} .select__value`);
      t.eq(await btn("node"), "All", "default All");
      t.eq(await rows(), 30, "all rows");
      // "All" without all_value: the request holds the options that the bar shows (9.10)
      const first = JSON.parse(panelQueries(p)[0].body);
      t.eq(first.variables.node, { all: true, values: ["v0", "v1", "v2"] }, "All request");
      t.eq(first.variables.blk.values, ["0"], "single select request");
      t.eq(first.variables.solo.values, ["v0"], "variable of the document that the SQL does not name is still sent");
      // open: search keeps matching texts
      await p.click(p.tid("variable-node-button"));
      await p.fill(p.tid("variable-node-search"), "v1");
      const hidden = await p.eval(`[...document.querySelectorAll('.listbox__item')].filter((e) => e.hidden).map((e) => e.textContent)`);
      t.eq(hidden, ["All", "v0", "v2"], "search hides the other options");
      await p.fill(p.tid("variable-node-search"), "");
      // Escape closes with no change
      await p.click('.listbox__item[data-value="v1"]');
      await p.press("Escape");
      await p.waitGone(".select__popup");
      t.eq(await btn("node"), "All", "Escape keeps the selection");
      // select v1 and v2, then close by a click outside: applies
      p.clearRequests();
      await p.click(p.tid("variable-node-button"));
      await p.click('.listbox__item[data-value="v1"]');
      await p.click('.listbox__item[data-value="v2"]');
      t.eq(panelQueries(p).length, 0, "a multi change does not apply before the popup closes");
      await p.click(".page-title, #app-main", {});
      await p.idle();
      t.eq(await btn("node"), "v1 + v2", "selected texts joined by +");
      t.eq((await params()).getAll("var-node"), ["v1", "v2"], "var-node repeated in the URL");
      await p.waitFor(`document.querySelectorAll('[data-testid="table-1"] tbody tr').length === 20`, { what: "20 rows" });
      t.eq(panelQueries(p).length, 1, "one panel query for the change");
      t.assert(optionQueries(p).some((r) => /variables\/blk\/options/.test(r.url)), "the dependent variable blk reloads its options");
      t.assert(!optionQueries(p).some((r) => /variables\/node\/options/.test(r.url)), "the changed variable does not reload itself");
      // All again
      await p.click(p.tid("variable-node-button"));
      await p.click('.listbox__item[data-value="$__all"]');
      await p.click("#app-main");
      await p.idle();
      t.eq((await params()).getAll("var-node"), [], "var-node leaves the address when All is the default");
      // text variable: applies on Enter
      p.clearRequests();
      await p.fill(p.tid("variable-account-input"), "v0");
      t.eq(panelQueries(p).length, 0, "no query before Enter");
      await p.press("Enter");
      await p.idle();
      t.eq((await params()).get("var-account"), "v0", "var-account");
      t.eq(await rows(), 10, "rows of v0");
      // custom list: Preproduction gives no row
      await p.click(p.tid("variable-env-button"));
      await p.click('.listbox__item[data-value="preprod"]');
      await p.idle();
      t.eq((await params()).get("var-env"), "preprod", "var-env");
      await p.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "empty"`, { what: "empty state" });
      // refresh on a time change: `node` reloads its options, `blk` (refresh load) does not
      p.clearRequests();
      await p.click(p.tid("time-picker-button"));
      await p.clickText(".picker__quick-item", "Last 5 minutes");
      await p.waitFor(`location.search.includes("from=now-5m")`);
      await p.idle();
      const names = optionQueries(p).map((r) => /variables\/([^/]+)\/options/.exec(r.url)[1]);
      t.assert(names.includes("node"), "node reloads at a time change: " + names);
      // 9.10: a variable below a time_change variable reloads with it (its parent options are new).
      t.assert(names.includes("blk"), "blk, below node, reloads with it: " + names);
      t.assert(!names.includes("solo"), "solo (refresh load, no parent) does not reload at a time change: " + names);
      // load from a URL with repeated values and from $__all
      const p2 = await t.open(t.dash("vars-test", "var-node=v0&var-node=v2&var-account=&var-env=prod"));
      await waitPanelsReady(p2, [1]);
      t.eq(await p2.text(`${p2.tid("variable-node-button")} .select__value`), "v0 + v2", "repeated values at load");
      t.eq(await p2.count('[data-testid="table-1"] tbody tr'), 20, "rows for v0 and v2");
      // the URL value wins over the default; an unknown value falls back to the default
      const p3 = await t.open(t.dash("vars-test", "var-node=nope"));
      await waitPanelsReady(p3, [1]);
      t.eq(await p3.text(`${p3.tid("variable-node-button")} .select__value`), "All", "unknown value gives the default");
    },
  },

  {
    id: "B09",
    name: "lazy panels, at most 6 queries open at once, a refresh cancels the older queries",
    async run(t) {
      const slow = ["SELECT sleep(1) AS s, 1 AS x"];
      const sliders = [];
      for (let i = 0; i < 12; i++) sliders.push(panel(i + 1, "table", `Slow ${i + 1}`, slow, { grid: { x: (i % 4) * 6, y: Math.floor(i / 4) * 4, w: 6, h: 4 } }));
      const d = doc("lazy-test", "Lazy", [
        section(null, [...sliders, panel(13, "table", "Spacer", SQL.bars, { grid: { x: 0, y: 12, w: 24, h: 70 } })]),
        section("Collapsed", [panel(14, "table", "Hidden A", SQL.bars, { grid: { x: 0, y: 0, w: 12, h: 4 } }), panel(15, "table", "Hidden B", SQL.bars, { grid: { x: 12, y: 0, w: 12, h: 4 } })], true),
        section("Far", [panel(16, "table", "Far away", SQL.bars, { grid: { x: 0, y: 0, w: 24, h: 4 } })]),
      ]);
      await ensure(t, d);
      const p = await t.page();
      // track the number of panel queries in flight
      let inflight = 0;
      let peak = 0;
      const live = new Set();
      const orig = p._event.bind(p);
      p._event = (method, params) => {
        orig(method, params);
        if (method === "Network.requestWillBeSent" && params.request.method === "POST" && /\/panels\/\d+\/query$/.test(params.request.url)) {
          live.add(params.requestId);
          inflight = live.size;
          peak = Math.max(peak, inflight);
        }
        if ((method === "Network.loadingFinished" || method === "Network.loadingFailed") && live.has(params.requestId)) {
          live.delete(params.requestId);
          inflight = live.size;
        }
      };
      await p.goto(t.dash("lazy-test"));
      await p.waitFor(`document.querySelector('[data-testid="panel-1"]')`);
      // let the first wave start, then refresh now: the older requests are aborted
      await p.waitFor(`true`);
      const t0 = Date.now();
      while (inflight < 6 && Date.now() - t0 < 5000) await sleep(20);
      t.eq(inflight, 6, "six queries open at the same time");
      await sleep(150);
      const peakBefore = peak;
      t.eq(peakBefore, 6, "never more than 6 queries open at once (first wave)");
      await p.click(p.tid("refresh-now"));
      await sleep(300);
      const aborted = p.requests.filter((r) => /\/panels\/\d+\/query$/.test(r.url) && r.failed);
      t.assert(aborted.length >= 6 && aborted.every((r) => /ERR_ABORTED/.test(r.failed)), `older queries aborted: ${aborted.map((r) => r.failed)}`);
      await p.idle({ timeout: 40000 });
      // After the refresh, each visible panel ran again; collapsed and far panels sent no query.
      const ran = new Set(panelQueries(p).map(panelIdOf));
      t.assert(!ran.has(14) && !ran.has(15), "collapsed section: no query");
      t.assert(!ran.has(16), "panel far outside the view: no query");
      for (let i = 1; i <= 13; i++) t.assert(ran.has(i), `panel ${i} queried`);
      const st = await panelStates(p);
      t.eq([st[14], st[16]], ["idle", "idle"], "states of the panels that did not query");
      // scroll near the far panel: it queries
      await p.eval(`document.querySelector('[data-testid="panel-16"]').scrollIntoView()`);
      await p.waitFor(`document.querySelector('[data-testid="panel-16"]').dataset.state === "ready"`, { what: "far panel ready" });
      // expand the collapsed section: its panels query
      await p.eval(`document.querySelector('[data-testid="section-1"]').scrollIntoView()`);
      await p.click(`${p.tid("section-1")} button[aria-expanded="false"]`);
      await p.waitFor(`document.querySelector('[data-testid="panel-14"]').dataset.state === "ready" && document.querySelector('[data-testid="panel-15"]').dataset.state === "ready"`, { what: "collapsed panels ready" });
    },
  },

  {
    id: "B11",
    name: "filterByValue, groupingToMatrix and long-to-wide give the expected output, with display names of 9.13",
    async run(t) {
      const d = doc("transform-test", "Transforms", [
        section(null, [
          panel(1, "timeseries", "Long to wide", ["SELECT event_time AS t, node, weight", "FROM analytics.seed", "ORDER BY t"], { grid: { x: 0, y: 0, w: 24, h: 8 } }),
          panel(2, "table", "Filter", SQL.table, {
            grid: { x: 0, y: 8, w: 12, h: 8 },
            transforms: [{
              kind: "filter_by_value", mode: "include", match: "all",
              filters: [
                { field: "blocks", matcher: { kind: "greater_or_equal", value: 10 } },
                { field: "blocks", matcher: { kind: "lower", value: 20 } },
                { field: "node", matcher: { kind: "regex", pattern: "^v[01]$" } },
              ],
            }],
          }),
          panel(3, "table", "Matrix", ["SELECT node, toString(blocks % 2) AS parity, blocks AS v", "FROM analytics.seed", "WHERE blocks < 5", "ORDER BY event_time"], {
            grid: { x: 12, y: 8, w: 12, h: 8 },
            transforms: [{ kind: "grouping_to_matrix", column_field: "parity", row_field: "node", value_field: "v", empty_value: "empty" }],
          }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("transform-test"));
      await waitPanelsReady(p, [1, 2, 3]);
      // long-to-wide: one time field and one series for each label set, named by 9.13
      const w = await viewData(p, 1);
      t.eq(w.headers, ["t", "weight v0", "weight v1", "weight v2"], "long-to-wide display names");
      t.eq(w.rows.length, 30, "30 distinct times");
      w.rows.forEach((row, n) => {
        const vals = row.slice(1);
        vals.forEach((v, k) => {
          if (k === n % 3) t.assert(Number(v) === n + 0.5, `row ${n} series ${k}: ${v}`);
          else t.eq(v, "", `row ${n} series ${k} empty`);
        });
      });
      // filterByValue: blocks in [10, 20) of the nodes v0 and v1
      const f = await viewData(p, 2);
      t.eq(f.rows.map((r) => r[2]), ["10", "12", "13", "15", "16", "18", "19"], "filter_by_value rows");
      // groupingToMatrix
      const m = await viewData(p, 3);
      t.eq(m.headers, ["node\\parity", "0", "1"], "matrix headers");
      t.eq(m.rows, [["v0", "0", "3"], ["v1", "4", "1"], ["v2", "2", ""]], "matrix rows");
      void SEED_FROM;
    },
  },
];
