// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B218 and B219 (F36; 9.5, 9.10a): the ad hoc filters of an `adhoc` variable. A server of the case has
// `clickhouse "prod".adhoc_columns`. The panel with `$__adhocFilters` runs again after each change of the filters, and the panel with no
// macro does not. The address holds the form of Grafana 12.4.2 (`var-filters=key|op|value`, measure of 2026-11-22).
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { doc, panel, section } from "../lib/docs.mjs";
import { api as apiOf } from "../lib/env.mjs";
import { panelIdOf, panelQueries, waitPanelsReady } from "../lib/h.mjs";

const UID = "adhoc-b218";
const COLUMNS = 'adhoc_columns      = ["node", "blocks", "active", "weight"]';

const filtersVar = (keys = ["node", "blocks"]) => ({ kind: "adhoc", name: "filters", label: "Filters", hide: "none", source: "prod", keys });
const mk = (keys) =>
  doc(UID, "Ad hoc filters", [
    section(null, [
      panel(1, "table", "Filtered", ["SELECT count() AS n", "FROM analytics.seed", "WHERE $__adhocFilters"], { grid: { x: 0, y: 0, w: 12, h: 6 } }),
      panel(2, "table", "All rows", ["SELECT count() AS n FROM analytics.seed"], { grid: { x: 12, y: 0, w: 12, h: 6 } }),
    ]),
  ], { variables: [filtersVar(keys)] });

/** The text of the first cell of a table panel. */
const cell = (p, id) => p.eval(`(document.querySelector('[data-testid="table-${id}"] tbody td') || {}).textContent ?? null`);
const chips = (p) => p.eval(`[...document.querySelectorAll('[data-testid="variable-filters-filter"]')].map((c) => c.textContent)`);
const filtersOf = (r) => JSON.parse(r.body).variables.filters;

async function server(t) {
  const s = await t.ownServer("adhoc", { chLines: [COLUMNS] });
  const r = await apiOf(s).create("transit", mk());
  t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
  return s;
}

async function open(t, s, query = "") {
  const p = await t.page();
  await p.goto(`${s.url}d/${UID}?${t.range}${query ? "&" + query : ""}`);
  await p.idle();
  await waitPanelsReady(p, [1, 2]);
  return p;
}

/** Adds one filter with the form of the bar and waits for the new answer of panel 1. */
async function addFilter(p, key, op, value) {
  const before = panelQueries(p).length;
  await p.click(p.tid("variable-filters-add"));
  await p.waitSel(p.tid("variable-filters-key"));
  await p.select(p.tid("variable-filters-key"), key);
  await p.select(p.tid("variable-filters-op"), op);
  await p.fill(p.tid("variable-filters-value"), value);
  await p.press("Enter");
  await p.waitFor(`document.querySelectorAll('[data-testid="variable-filters-filter"]').length > 0`, { what: "a chip" });
  await p.idle();
  await waitPanelsReady(p, [1, 2]);
  return panelQueries(p).slice(before);
}

export const cases = [
  {
    id: "B218",
    name: "ad hoc filters: the bar adds and removes filters, the panel with $__adhocFilters runs again with the closed request member, the address holds the form of Grafana and a Grafana link opens with its filters (9.10a)",
    async run(t) {
      const s = await server(t);
      const p = await open(t, s);
      t.eq(await cell(p, 1), "30", "no filter: 1=1");
      t.eq(await cell(p, 2), "30", "the panel without the macro");
      t.eq(await chips(p), [], "no chip");
      t.eq(await p.attr(p.tid("variable-filters-add"), "aria-label"), "Add filter to Filters", "the name of the add button");
      // The form offers the permitted keys of 6.14 and the two operators only.
      await p.click(p.tid("variable-filters-add"));
      t.eq(await p.eval(`[...document.querySelector('[data-testid="variable-filters-key"]').options].map((o) => o.value)`), ["node", "blocks"], "the keys");
      t.eq(await p.eval(`[...document.querySelector('[data-testid="variable-filters-op"]').options].map((o) => o.value)`), ["=", "!="], "the operators");
      await p.press("Escape");
      await p.waitFor(`document.querySelector('.adhoc__form').hidden`, { what: "the form hides" });
      t.eq(await p.eval("document.activeElement.dataset.testid"), "variable-filters-add", "Escape gives the focus back");

      let sent = await addFilter(p, "node", "=", "v1");
      t.eq(sent.map(panelIdOf), [1], "only panel 1 runs again");
      t.eq(filtersOf(sent[0]), { filters: [{ key: "node", op: "=", value: "v1" }] }, "the request member");
      t.eq(await cell(p, 1), "10", "node = v1 keeps 10 rows");
      t.eq(await chips(p), ["node = v1"], "one chip");
      t.assert((await p.eval("location.search")).includes("var-filters=node%7C%3D%7Cv1"), "the address holds the filter");

      sent = await addFilter(p, "blocks", "!=", "4");
      t.eq(filtersOf(sent[0]).filters.length, 2, "two filters");
      t.eq(await cell(p, 1), "9", "and blocks != 4");
      const search = await p.eval("location.search");
      t.eq(new URLSearchParams(search).getAll("var-filters"), ["node|=|v1", "blocks|!=|4"], "two items in the address, in order");

      // Remove the first filter: the chip goes and the panel runs again.
      const before = panelQueries(p).length;
      await p.click('[data-testid="variable-filters-filter"] button[aria-label="Remove filter with key node"]');
      await p.waitFor(`document.querySelectorAll('[data-testid="variable-filters-filter"]').length === 1`, { what: "one chip left" });
      await p.idle();
      await waitPanelsReady(p, [1, 2]);
      t.eq(panelQueries(p).slice(before).map(panelIdOf), [1], "panel 1 runs again");
      t.eq(await cell(p, 1), "29", "blocks != 4 keeps 29 rows");
      t.eq(await p.eval("document.activeElement.dataset.testid"), "variable-filters-add", "the focus goes to the add button");

      // A link of Grafana 12.4.2: percent-encoded items, `!=`, a label after `,`, and the escape of `|`.
      const g = await open(t, s, "var-filters=node%7C%21%3D%7Cv0&var-filters=blocks%7C%3D%7C5,five");
      t.eq(await chips(g), ["node != v0", "blocks = 5"], "the filters of the link");
      t.eq(await cell(g, 1), "1", "node != v0 and blocks = 5 keep one row");
      t.eq(filtersOf(panelQueries(g).find((r) => panelIdOf(r) === 1)).filters, [{ key: "node", op: "!=", value: "v0" }, { key: "blocks", op: "=", value: "5" }], "the request");
      const e = await open(t, s, "var-filters=node%7C%3D%7Cv__gfp__1");
      t.eq(await chips(e), ["node = v|1"], "__gfp__ is a |");
      t.eq(await cell(e, 1), "0", "no row has v|1");
      // A value with a quote and a backslash stays a value.
      const q = await open(t, s, `var-filters=${encodeURIComponent("node|=|v1' OR 1=1 --\\")}`);
      t.eq(await cell(q, 1), "0", "a hostile value matches no row");
    },
  },
  {
    id: "B219",
    name: "ad hoc filters: an item of the address with a key that is not permitted, another operator, a control character or a place after the 16th is dropped with a notice; the form refuses a long value; a key that the configuration drops is not offered (9.10a)",
    async run(t) {
      const s = await server(t);
      const items = ["node|=|v1", "zone|=|x", "node|>|v0", "node|=|a\nb", "weight|=|1", "blocks|=", ...Array.from({ length: 16 }, (_, i) => `blocks|!=|${100 + i}`)];
      const p = await open(t, s, items.map((i) => `var-filters=${encodeURIComponent(i)}`).join("&"));
      const notices = await p.eval(`[...document.querySelectorAll('[data-testid="notice"]')].map((n) => [n.dataset.notice || null, n.textContent])`);
      const notice = notices.find((n) => n[0] === "variable");
      t.assert(notice && notice[1].includes("The page does not use these filters of Filters: \"zone|=|x\", \"node|>|v0\", \"node|=|a") && notice[1].includes("and 3 more."), `the notice: ${JSON.stringify(notices)}`);
      const sent = filtersOf(panelQueries(p).find((r) => panelIdOf(r) === 1)).filters;
      t.eq(sent.length, 16, "16 filters at most go to the server");
      t.eq(sent[0], { key: "node", op: "=", value: "v1" }, "the first valid item");
      t.eq((await chips(p)).length, 16, "16 chips");
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-filters-add"]').disabled`), true, "no 17th filter");
      // The address drops what the page does not use.
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-filters").length === 16`, { what: "the address with 16 items" });
      t.assert(!(await p.eval("location.search")).includes("zone"), "no dropped item in the address");

      // The form: a value above 1,024 bytes shows the error and sends nothing.
      const f = await open(t, s);
      const before = panelQueries(f).length;
      await f.click(f.tid("variable-filters-add"));
      await f.select(f.tid("variable-filters-key"), "blocks");
      await f.fill(f.tid("variable-filters-value"), "é".repeat(513));
      await f.click(f.tid("variable-filters-apply"));
      await f.waitFor(`document.querySelector('.adhoc__form [role="alert"]') && !document.querySelector('.adhoc__form [role="alert"]').hidden`, { what: "the error" });
      t.eq(await f.text(".adhoc__form [role=\"alert\"]"), "A value has 1024 bytes at most.", "the error text");
      t.eq(await f.attr(f.tid("variable-filters-value"), "aria-invalid"), "true", "the field is invalid");
      t.eq(panelQueries(f).length, before, "no request");
      await f.press("Escape");

      // A server whose list has `node` only: 6.14 offers it alone, and an item of `blocks` in the address is dropped.
      // The document goes in the volume before the start: the load of a document runs no check of 7.18 (7.18).
      const prepare = (volume) => writeFileSync(join(volume, "transit", `${UID}.json`), JSON.stringify(mk(), null, 2) + "\n");
      const narrow = await t.ownServer("narrow", { chLines: ['adhoc_columns      = ["node"]'], prepare });
      const read = await apiOf(narrow).dashboard(UID);
      t.eq(read.json.adhoc, { filters: ["node"] }, "the permitted keys of 6.14");
      const n = await open(t, narrow, "var-filters=blocks%7C%3D%7C5&var-filters=node%7C%3D%7Cv2");
      t.eq(await chips(n), ["node = v2"], "the blocks item is dropped");
      t.eq(await cell(n, 1), "10", "node = v2");
      await n.click(n.tid("variable-filters-add"));
      t.eq(await n.eval(`[...document.querySelector('[data-testid="variable-filters-key"]').options].map((o) => o.value)`), ["node"], "one key offered");
      await n.press("Escape");
    },
  },
];
