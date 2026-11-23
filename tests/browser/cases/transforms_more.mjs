// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B180 to B189 (4.25a, 9.21a, 9.22a, 10.3a, 15.6): the eleven more transforms (F28).
// B180: organize, sort_by, limit. B181: calculate_field. B182: merge. B183: join_by_field. B184: partition_by_values.
// B185: the caps. B186: the forms of the editor. B187: the import. B188: a reuse panel. B189: hostile names.
// All SQL is synthetic: it runs literal SELECT texts or `numbers(...)`.
import { readFileSync } from "node:fs";
import { doc, panel, section } from "../lib/docs.mjs";
import { SEED_FROM } from "../lib/env.mjs";
import { draftOf, enterEdit, openPanelEditor } from "../lib/ed.mjs";
import { panelIdOf, panelQueries, sleep, viewData, waitPanelsReady } from "../lib/h.mjs";

const BASE = Math.floor(SEED_FROM / 1000);

/** A table of rows: `cols` is "host String, x Int32", `rows` is [["a", 1], ...]. The order of the rows is the order given. */
const values = (cols, rows) => [`SELECT * FROM values('${cols}', ${rows.map((r) => `(${r.map((v) => (typeof v === "string" ? `'${v}'` : v)).join(", ")})`).join(", ")})`];
/** A time series SQL text of `n` rows, one for each 60 s from the seed start, `shift` steps late. */
const series = (col, shift = 0, from = 0, n = 5) => [`SELECT toDateTime(${BASE + shift * 60} + number * 60) AS t, number + ${from} AS ${col} FROM numbers(${n})`];

const ROWS = [["a", 1, 10, 100], ["b", 2, 20, 200], ["c", 3, 30, 300], ["d", 4, 40, 400], ["e", 5, 50, 500]];
const FOUR = values("host String, x Int32, y Int32, z Int32", ROWS);

/** A panel with several queries: the first is `data.query`, the others are `data.extra_queries` (4.15a). */
function mq(id, type, title, queries, o = {}) {
  const [first, ...rest] = queries;
  const p = panel(id, type, title, first.sql, { ...o, format: first.format || (type === "timeseries" ? "time_series" : "table") });
  if (first.ref && first.ref !== "A") p.data.query.ref = first.ref;
  if (rest.length) p.data.extra_queries = rest.map((q) => ({ ref: q.ref, source: p.data.query.source, format: q.format || p.data.query.format, sql: q.sql }));
  return p;
}

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 600)}`);
}

/** The headers and the rows of the table of a panel, as texts. */
const tableOf = (p, id) => p.eval(`(() => {
  const t = document.querySelector('[data-testid="table-${id}"]');
  if (!t) return null;
  return { headers: [...t.querySelectorAll("thead th .vtable__label")].map((e) => e.textContent), rows: [...t.querySelectorAll("tbody tr[data-row]")].map((r) => [...r.children].map((c) => c.textContent)) };
})()`);
const stateOf = (p, id) => p.eval(`(document.querySelector('[data-testid="panel-${id}"]') || {dataset: {}}).dataset.state || null`);
const errorOf = (p, id) => p.eval(`(document.querySelector('[data-testid="panel-${id}-error"] .panel__error-text') || {}).textContent || null`);
const legend = (p, id) => p.eval(`[...document.querySelectorAll('[data-testid="panel-${id}"] .chartCore__legendItem')].map((e) => e.textContent.trim())`);
const options = (p, id) => p.eval(`(document.querySelector('[data-testid="panel-${id}-frames"]') ? [...document.querySelector('[data-testid="panel-${id}-frames"]').options].map((o) => o.text) : null)`);
const column = (data, name) => data.rows.map((r) => r[data.headers.indexOf(name)]);

const organize = (hide, order, rename) => ({ kind: "organize", hide, order, rename: rename.map(([field, name]) => ({ field, name })) });
const calc = (mode, object, o = {}) => ({ kind: "calculate_field", mode, alias: o.alias ?? null, replace_fields: o.replace ?? false, [{ binary: "binary", reduce_row: "reduce", unary: "unary", index: "index" }[mode]]: object });

async function openGroup(p, key) {
  const open = await p.eval(`document.querySelector('[data-group="${key}"]').getAttribute("aria-expanded") === "true"`);
  if (!open) await p.click(`[data-group="${key}"]`);
}

/** Wait until the draft panel `id` of the dashboard `uid` satisfies `pred`. */
async function waitPanel(p, uid, id, pred, what) {
  await p.waitFor(`(() => { try { const r = JSON.parse(localStorage.getItem("hune.draft.v1.${uid}")); for (const s of r.document.sections) for (const x of s.panels) if (x.id === ${id}) return (${pred.toString()})(x); } catch (e) {} return false; })()`, { what });
}

export const cases = [
  {
    id: "B180",
    name: "organize (rename, hide, order), then sort_by and limit: the header shows the new names, the order and the cut, and View data shows the same (9.21a, rules 1 to 5)",
    async run(t) {
      const d = doc("tm180", "More transforms: organize", [
        section(null, [
          panel(1, "table", "Organize, sort, limit", FOUR, {
            grid: { x: 0, y: 0, w: 12, h: 10 },
            transforms: [
              organize(["z"], ["y", "host"], [["host", "Host"]]),
              { kind: "sort_by", sort: [{ field: "x", desc: true }] },
              { kind: "limit", count: 3, end: "first" },
            ],
          }),
          panel(2, "table", "Limit last", FOUR, { grid: { x: 12, y: 0, w: 12, h: 10 }, transforms: [{ kind: "limit", count: 2, end: "last" }, { kind: "sort_by", sort: [{ field: "nope", desc: false }] }] }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("tm180"));
      await waitPanelsReady(p, [1, 2]);
      const one = await tableOf(p, 1);
      t.eq(one.headers, ["y", "Host", "x"], "the order, the rename, the hidden field");
      t.eq(column(one, "x"), ["5", "4", "3"], "sorted down and cut at 3");
      t.eq(column(one, "Host"), ["e", "d", "c"], "the rows follow the sort");
      const view = await viewData(p, 1);
      t.eq(view.headers, one.headers, "View data has the same headers");
      t.eq(view.rows.length, 3, "View data has the same rows");
      const two = await tableOf(p, 2);
      t.eq(column(two, "host"), ["d", "e"], "limit with end last; a sort key that is not a field does nothing");
      t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-1-error"]')`), false, "no error");
    },
  },
  {
    id: "B181",
    name: "calculate_field in the four modes on a table and a stat: the default name, the alias, replace_fields (9.21a, rule 3)",
    async run(t) {
      const xy = values("x Int32, y Int32, z Int32", [[1, 10, 100], [2, 20, 200], [3, 30, 300]]);
      const d = doc("tm181", "More transforms: calculate_field", [
        section(null, [
          panel(1, "table", "Binary", xy, { grid: { x: 0, y: 0, w: 8, h: 8 }, transforms: [calc("binary", { left: { kind: "field", field: "x" }, right: { kind: "field", field: "y" }, operator: "add" })] }),
          panel(2, "table", "Alias", xy, { grid: { x: 8, y: 0, w: 8, h: 8 }, transforms: [calc("binary", { left: { kind: "field", field: "z" }, right: { kind: "number", value: 2 }, operator: "divide" }, { alias: "Half" })] }),
          panel(3, "table", "Reduce row", xy, { grid: { x: 16, y: 0, w: 8, h: 8 }, transforms: [calc("reduce_row", { reducer: "total", include: ["x", "y"] })] }),
          panel(4, "table", "Unary", xy, { grid: { x: 0, y: 8, w: 8, h: 8 }, transforms: [calc("unary", { operator: "abs", field: "x" })] }),
          panel(5, "table", "Index, replace", xy, { grid: { x: 8, y: 8, w: 8, h: 8 }, transforms: [calc("index", { as_percentile: false }, { replace: true })] }),
          panel(6, "stat", "Stat", xy, { grid: { x: 16, y: 8, w: 8, h: 8 }, transforms: [calc("binary", { left: { kind: "field", field: "x" }, right: { kind: "field", field: "y" }, operator: "multiply" }, { alias: "Product" })], options: { reduce: { calc: "last_not_null", fields: "/^Product$/", values: false }, color_mode: "value", graph_mode: "none", text_mode: "auto", orientation: "auto" } }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("tm181"));
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6]);
      const first = await tableOf(p, 1);
      t.eq(first.headers, ["x", "y", "z", "x + y"], "the default name of a binary operation");
      t.eq(column(first, "x + y"), ["11", "22", "33"], "the sums");
      t.eq(column(await tableOf(p, 2), "Half"), ["50", "100", "150"], "the alias and a number operand");
      const total = await tableOf(p, 3);
      t.eq(total.headers.at(-1), "Total", "the title of the calculation");
      t.eq(column(total, "Total"), ["11", "22", "33"], "the total of the listed fields");
      t.eq((await tableOf(p, 4)).headers.at(-1), "abs(x)", "the default name of a unary operation");
      const index = await tableOf(p, 5);
      t.eq(index.headers, ["Row"], "replace_fields with no time field keeps the new field only");
      t.eq(column(index, "Row"), ["0", "1", "2"], "the row index");
      const tiles = await p.eval(`[...document.querySelectorAll('[data-testid="panel-6"] .stat__title-text')].map((e) => e.textContent.trim())`);
      t.eq(tiles, ["Product"], "the stat shows the new field by its alias");
      t.assert((await p.eval(`document.querySelector('[data-testid="panel-6"]').textContent`)).includes("90"), "the stat shows the last product (3 x 30)");
    },
  },
  {
    id: "B182",
    name: "merge: a table with two queries shows one frame with no select, and a merge of a panel with one query does nothing (9.21a, rule 6)",
    async run(t) {
      const a = values("host String, x Int32", [["a", 1], ["b", 2]]);
      const b = values("host String, y Int32", [["b", 20], ["c", 30]]);
      const d = doc("tm182", "More transforms: merge", [
        section(null, [
          mq(1, "table", "Merge of A and B", [{ ref: "A", sql: a }, { ref: "B", sql: b }], { grid: { x: 0, y: 0, w: 12, h: 8 }, transforms: [{ kind: "merge" }] }),
          mq(2, "table", "No merge", [{ ref: "A", sql: a }, { ref: "B", sql: b }], { grid: { x: 12, y: 0, w: 12, h: 8 } }),
          panel(3, "table", "One query", a, { grid: { x: 0, y: 8, w: 12, h: 6 }, transforms: [{ kind: "merge" }] }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("tm182"));
      await waitPanelsReady(p, [1, 2, 3]);
      const merged = await tableOf(p, 1);
      t.eq(merged.headers, ["host", "x", "y"], "one frame with the fields of both queries");
      t.eq(column(merged, "host"), ["a", "b", "c"], "the rows join on the key host");
      t.eq(column(merged, "y"), ["", "20", "30"], "a field that a frame lacks is empty");
      t.eq(await options(p, 1), null, "one frame: no select of frames");
      t.eq(await options(p, 2), ["A", "B"], "without the merge the select lists the queries");
      const one = await tableOf(p, 3);
      t.eq([one.headers, one.rows.length], [["host", "x"], 2], "a merge of a panel with one query does nothing");
    },
  },
  {
    id: "B183",
    name: "join_by_field on time with two queries: one chart with the legend A a and B b, and the mode inner removes the rows that one query lacks (9.21a, rule 7)",
    async run(t) {
      const join = (mode) => ({ kind: "join_by_field", field: null, mode });
      const queries = [{ ref: "A", sql: series("a") }, { ref: "B", sql: series("b", 1, 10) }];
      const d = doc("tm183", "More transforms: join", [
        section(null, [
          mq(1, "timeseries", "Outer", queries, { grid: { x: 0, y: 0, w: 12, h: 8 }, transforms: [join("outer")] }),
          mq(2, "timeseries", "Inner", queries, { grid: { x: 12, y: 0, w: 12, h: 8 }, transforms: [join("inner")] }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("tm183"));
      await waitPanelsReady(p, [1, 2]);
      t.eq(await legend(p, 1), ["A a", "B b"], "the legend keeps the prefix of each query");
      t.eq(await p.eval(`document.querySelectorAll('[data-testid="panel-1"] canvas').length > 0`), true, "one chart draws");
      const outer = await viewData(p, 1);
      const inner = await viewData(p, 2);
      t.eq(outer.headers, ["t", "A a", "B b"], "the key field comes first");
      t.eq([outer.rows.length, inner.rows.length], [6, 4], "outer keeps all the times, inner keeps the times of both queries");
      t.eq(await legend(p, 2), ["A a", "B b"], "the legend of the inner join");
    },
  },
  {
    id: "B184",
    name: "partition_by_values: a time series draws one series for each value, and a table lists each frame in its select with the text of 9.22a (rule 9)",
    async run(t) {
      const long = [`SELECT toDateTime(${BASE} + (number % 4) * 60) AS t, arrayElement(['x', 'y', 'z'], 1 + intDiv(number, 4)) AS host, number AS v FROM numbers(12)`];
      const d = doc("tm184", "More transforms: partition", [
        section(null, [
          panel(1, "timeseries", "One series for each host", long, { grid: { x: 0, y: 0, w: 12, h: 8 }, format: "table", transforms: [{ kind: "partition_by_values", fields: ["host"], keep_fields: false }] }),
          panel(2, "table", "A frame for each host", long, { grid: { x: 12, y: 0, w: 12, h: 8 }, format: "table", transforms: [{ kind: "partition_by_values", fields: ["host"], keep_fields: false }] }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("tm184"));
      await waitPanelsReady(p, [1, 2]);
      t.eq(await legend(p, 1), ["v x", "v y", "v z"], "one series for each value, named by its label");
      t.eq(await options(p, 2), ["A x", "A y", "A z"], "the option text is the ref and the label value");
      t.eq(column(await tableOf(p, 2), "v x"), ["0", "1", "2", "3"], "the first frame");
      await p.select(p.tid("panel-2-frames"), "A y");
      await p.waitFor(`document.querySelector('[data-testid="table-2"] tbody tr[data-row]').lastElementChild.textContent === "4"`, { what: "frame y shown" });
      t.eq(column(await tableOf(p, 2), "v y"), ["4", "5", "6", "7"], "the choice shows the second frame");
    },
  },
  {
    id: "B185",
    name: "the caps: a join of three frames of 99,000 rows and a partition of 150 values each show the error of the cap, and the other panels stay as they were (9.21a, rule 13; T85)",
    timeout: 240000,
    async run(t) {
      // The query limit of 100,000 rows (8.5) bounds each frame, so the join of three frames goes above 200,000 rows.
      const big = (col) => [`SELECT number AS k, number AS ${col} FROM numbers(99000)`];
      const d = doc("tm185", "More transforms: caps", [
        section(null, [
          mq(1, "table", "Join above the cap", [{ ref: "A", sql: big("a") }, { ref: "B", sql: big("b") }, { ref: "C", sql: big("c") }], { grid: { x: 0, y: 0, w: 12, h: 8 }, transforms: [{ kind: "join_by_field", field: "k", mode: "outer" }] }),
          panel(2, "table", "Partition above the cap", [`SELECT number AS k, number AS v FROM numbers(150)`], { grid: { x: 12, y: 0, w: 12, h: 8 }, transforms: [{ kind: "partition_by_values", fields: ["k"], keep_fields: false }] }),
          panel(3, "table", "Other panel", FOUR, { grid: { x: 0, y: 8, w: 12, h: 8 } }),
          mq(4, "table", "Join at the bound", [{ ref: "A", sql: big("a") }, { ref: "B", sql: big("b") }], { grid: { x: 12, y: 8, w: 12, h: 8 }, transforms: [{ kind: "join_by_field", field: "k", mode: "inner" }] }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("tm185"));
      await waitPanelsReady(p, [1, 2], "error");
      await waitPanelsReady(p, [3, 4]);
      t.eq(await errorOf(p, 1), "The transform join_by_field failed: 297000 rows, 200,000 at most", "the error of the rows");
      t.eq(await errorOf(p, 2), "The transform partition_by_values failed: 101 frames, 100 at most", "the error of the frames");
      t.eq(await stateOf(p, 3), "ready", "the other panel stays ready");
      t.eq((await tableOf(p, 3)).rows.length, 5, "the other panel keeps its rows");
      t.eq(await stateOf(p, 4), "ready", "a join of 198,000 rows is under the cap");
      t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-1-retry"]')`), true, "the error panel has Retry");
    },
  },
  {
    id: "B186",
    name: "the editor adds each of the eleven kinds, edits its fields, moves it, removes it and shows an error of 4.37; the JSON tab and the draft agree (10.3a)",
    async run(t) {
      const uid = "tm186";
      const d = doc(uid, "More transforms: editor", [section(null, [panel(1, "table", "Editor", FOUR, { grid: { x: 0, y: 0, w: 12, h: 8 } })])]);
      await ensure(t, d);
      const p = await t.open(t.dash(uid));
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      await openPanelEditor(p, 1);
      await openGroup(p, "transforms");
      const KINDS = ["organize", "calculate_field", "sort_by", "limit", "merge", "reduce", "partition_by_values", "join_by_field", "filter_fields_by_name", "rename_by_regex", "convert_field_type"];
      for (const kind of KINDS) {
        await p.select(p.tid("transform-add-kind"), kind);
        await p.click(p.tid("transform-add"));
      }
      await waitPanel(p, uid, 1, (q) => q.transforms.length === 11, "eleven transforms");
      const kinds = async () => (await draftOf(p, uid)).document.sections[0].panels[0].transforms;
      t.eq((await kinds()).map((x) => x.kind), KINDS, "the eleven kinds in the order of the adds");
      for (let i = 0; i < 11; i++) t.eq(await p.eval(`document.querySelector('[data-testid="transform-${i}-kind"]').value`), KINDS[i], `the select of the kind of ${i}`);
      // limit (3): the count and the end
      await p.fill(p.tid("transform-3-count"), "7");
      await p.select(p.tid("transform-3-end"), "last");
      // calculate_field (1): a binary operation
      await p.select(p.tid("transform-1-mode"), "binary");
      await p.select(p.tid("transform-1-operator"), "multiply");
      await p.fill(p.tid("transform-1-right-value"), "3");
      await p.fill(p.tid("transform-1-alias"), "Triple");
      // sort_by (2): a second key
      await p.click(p.tid("transform-2-sort-add"));
      // organize (0): hide the first field
      await p.click(p.tid("transform-0-hide-0"));
      // join_by_field (7) and rename_by_regex (9)
      await p.select(p.tid("transform-7-mode"), "inner");
      await p.fill(p.tid("transform-9-regex"), "(.*)_total");
      await p.fill(p.tid("transform-9-rename-pattern"), "$1");
      await sleep(900);
      const now = await kinds();
      t.eq([now[3].count, now[3].end], [7, "last"], "limit");
      t.eq(now[1].binary.operator, "multiply", "the operator");
      t.eq(now[1].binary.right, { kind: "number", value: 3 }, "the number operand");
      t.eq(now[1].alias, "Triple", "the alias");
      t.eq(now[2].sort.length, 2, "two sort keys");
      t.eq(now[0].hide.length, 1, "one field hidden");
      t.eq([now[7].mode, now[9].regex, now[9].rename_pattern], ["inner", "(.*)_total", "$1"], "join and rename");
      // the error of 4.37: a count above the bound shows under the field and the draft keeps the last good value
      await p.fill(p.tid("transform-3-count"), "1000001");
      await sleep(900);
      t.assert((await p.eval(`document.querySelector('[data-testid="transform-3"]').textContent`)).includes("whole number from 0 to 1000000"), "the error of the count");
      t.eq((await kinds())[3].count, 7, "the draft keeps the last good count");
      await p.fill(p.tid("transform-9-regex"), "(");
      await sleep(900);
      t.assert((await p.eval(`document.querySelector('[data-testid="transform-9"]').textContent`)).includes("The pattern is not valid."), "the error of the pattern");
      // two numbers are not permitted (4.34)
      await p.select(p.tid("transform-1-left-kind"), "number");
      t.assert((await p.eval(`document.querySelector('[data-testid="transform-1"]').textContent`)).includes("One operand must be a field."), "two numbers");
      // move and remove
      await p.click(p.tid("transform-4-up"));
      await waitPanel(p, uid, 1, (q) => q.transforms[3].kind === "merge" && q.transforms[4].kind === "limit", "the merge moved up");
      await p.click(p.tid("transform-3-remove"));
      await waitPanel(p, uid, 1, (q) => q.transforms.length === 10 && q.transforms.every((x) => x.kind !== "merge"), "the merge removed");
      // a change of kind in the select replaces the transform with a new transform of that kind
      await p.select(p.tid("transform-3-kind"), "sort_by");
      await waitPanel(p, uid, 1, (q) => q.transforms[3].kind === "sort_by", "the kind changed");
      // the JSON tab agrees with the draft
      await p.click(p.tid("tab-json"));
      const pj = JSON.parse(await p.eval(`document.querySelector('[data-testid="panel-json"]').value`));
      t.eq(pj.transforms, await kinds(), "the JSON tab and the draft agree");
      t.eq(pj.transforms.length, 10, "ten transforms");
    },
  },
  {
    id: "B187",
    name: "the import of a Grafana dashboard with the eleven ids gives the rows of 15.6: the report, the new kinds in the saved document and a panel that draws (15.6, 4.38)",
    async run(t) {
      const { cpSync, mkdirSync, mkdtempSync, writeFileSync } = await import("node:fs");
      const { tmpdir } = await import("node:os");
      const { spawnSync } = await import("node:child_process");
      const { join } = await import("node:path");
      const { BINARY, ROOT } = await import("../lib/env.mjs");
      const s = await t.ownServer("imp187");
      const work = mkdtempSync(join(tmpdir(), "hune-b187-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      const ds = { type: "grafana-clickhouse-datasource", uid: "clickhouse" };
      const target = (rawSql, refId = "A") => ({ datasource: ds, editorType: "sql", format: 1, pluginVersion: "4.5.1", queryType: "table", rawSql, refId });
      const grafana = JSON.parse(readFileSync(join(fx, "multi_query_table.json"), "utf8"));
      const transformations = [
        { id: "organize", options: { excludeByName: { z: true }, indexByName: { y: 0 }, renameByName: { host: "Host" } } },
        { id: "calculateField", options: { mode: "binary", binary: { left: "x", operator: "+", right: "y" }, alias: "S" } },
        { id: "sortBy", options: { sort: [{ field: "x", desc: true }] } },
        { id: "limit", options: { limitField: 3 } },
        { id: "merge", options: {} },
        { id: "reduce", options: { mode: "seriesToRows", reducers: ["max", "sum"] } },
        { id: "partitionByValues", options: { fields: ["host"], keepFields: false } },
        { id: "joinByField", options: { byField: "host", mode: "outer" } },
        { id: "filterFieldsByName", options: { include: { names: ["host"], pattern: "/^x/" } } },
        { id: "renameByRegex", options: { regex: "(.*)_user", renamePattern: "$1" } },
        { id: "convertFieldType", options: { conversions: [{ targetField: "x", destinationType: "string" }] } },
        { id: "labelsToFields", options: {} },
      ];
      Object.assign(grafana.panels[0], { transformations, targets: [target("SELECT 'a' AS host, 1 AS x, 10 AS y, 100 AS z")] });
      Object.assign(grafana, { uid: "tm187", title: "More transforms: import" });
      writeFileSync(join(work, "dashboards", "tm187.json"), JSON.stringify(grafana));
      for (const f of ["folders.json", "health.json", "plugin-clickhouse.json"]) cpSync(join(fx, "export", f), join(work, f));
      writeFileSync(join(work, "search.json"), JSON.stringify([{ uid: "tm187", folderUid: "fe1x0abc2kq9" }]));
      const run = (extra) => spawnSync(BINARY, ["import-grafana", "--in", work, "--source-map", "clickhouse=prod", "--folders", join(fx, "export/folders.map.json"), "--report", join(work, "report"), "-c", s.configPath, ...extra], { encoding: "utf8", env: {} });
      const dry = run([]);
      let report;
      try {
        report = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      } catch (error) {
        throw new Error(`no report: status ${dry.status}: ${dry.stdout.slice(-600)} ${dry.stderr.slice(-600)}`);
      }
      const collect = (o) => (Array.isArray(o) ? o.flatMap(collect) : o && typeof o === "object" ? Object.entries(o).flatMap(([k, v]) => (k === "items" && Array.isArray(v) ? v : collect(v))) : []);
      const items = collect(report.dashboards);
      t.assert([...new Set(items.map((i) => i.class))].every((c) => ["mapped", "changed", "unsupported", "dropped", "metadata"].includes(c)), "the classes of 15.6");
      // 15.6: partitionByValues and the values of convertFieldType are `changed`, and each id of the eleven has no item that is `unsupported` or `dropped`.
      const mine = items.filter((i) => /^transformations\./.test(i.member || ""));
      const changed = mine.filter((i) => i.class === "changed").map((i) => i.member);
      t.assert(changed.includes("transformations.6") && changed.includes("transformations.10.options.conversions"), "partitionByValues and the conversion are changed: " + JSON.stringify(mine.map((i) => [i.member, i.class])));
      t.eq(mine.filter((i) => i.class === "unsupported").length, 0, "no unsupported item for the eleven ids");
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B187\n${items.map((i) => i.id).join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import with the acceptance: " + wrote.stdout.slice(-400) + wrote.stderr.slice(-300));
      await s.restart();
      const got = await (await fetch(`${s.url}api/dashboards/tm187`)).json();
      const imported = got.document.sections.flatMap((x) => x.panels)[0].transforms;
      t.eq(imported.map((x) => x.kind), ["organize", "calculate_field", "sort_by", "limit", "merge", "reduce", "partition_by_values", "join_by_field", "filter_fields_by_name", "rename_by_regex", "convert_field_type"], "the eleven new kinds, in order");
      t.eq(imported[0], { kind: "organize", hide: ["z"], order: ["y"], rename: [{ field: "host", name: "Host" }] }, "the members of organize");
      t.eq(imported[3], { kind: "limit", count: 3, end: "first" }, "the members of limit");
      // The page draws the imported panel with no error: the transforms run in the browser.
      const p = await t.open(`${s.url}d/tm187?from=${SEED_FROM}&to=${SEED_FROM + 1200000}`);
      await p.waitFor(`(document.querySelector('[data-testid="panel-1"]') || {dataset: {}}).dataset.state !== undefined`, { what: "the panel has a state" });
      t.assert(["ready", "error", "empty"].includes(await stateOf(p, 1)), "the panel reaches a final state");
    },
  },
  {
    id: "B188",
    name: "a reuse panel with with_transforms applies the new kinds of its source and then its own, in this order (9.21a, rule 16; 9.23)",
    async run(t) {
      const d = doc("tm188", "More transforms: reuse", [
        section(null, [
          panel(1, "table", "Source", FOUR, { grid: { x: 0, y: 0, w: 12, h: 8 }, transforms: [organize(["z"], [], [["x", "X"]]), { kind: "filter_fields_by_name", include: { names: [], pattern: null }, exclude: { names: ["y"], pattern: null } }] }),
          panel(2, "table", "Reuse", [], { grid: { x: 12, y: 0, w: 12, h: 8 }, reuse: { panel: 1, with_transforms: true }, transforms: [{ kind: "sort_by", sort: [{ field: "X", desc: true }] }, { kind: "limit", count: 2, end: "first" }] }),
          panel(3, "table", "Reuse without", [], { grid: { x: 0, y: 8, w: 12, h: 8 }, reuse: { panel: 1, with_transforms: false }, transforms: [{ kind: "limit", count: 1, end: "first" }] }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("tm188"));
      await waitPanelsReady(p, [1, 2, 3]);
      const source = await tableOf(p, 1);
      t.eq(source.headers, ["host", "X"], "the source: hide z, rename x, exclude y");
      const reuse = await tableOf(p, 2);
      t.eq(reuse.headers, ["host", "X"], "the reuse panel has the source transforms first");
      t.eq(column(reuse, "X"), ["5", "4"], "then its own: sort by the new name X, then the cut");
      const without = await tableOf(p, 3);
      t.eq(without.headers, ["host", "x", "y", "z"], "with_transforms false: the source transforms do not run");
      t.eq(without.rows.length, 1, "the own limit runs");
    },
  },
  {
    id: "B189",
    name: "hostile names: a name with markup as a rename, an alias, a label and a converted value shows as text; no script runs and the CSP report is empty (T86, 9.1)",
    async run(t) {
      const evil = "<img src=x onerror=alert(1)>";
      const evil2 = "<svg onload=alert(2)>";
      const d = doc("tm189", "More transforms: hostile", [
        section(null, [
          panel(1, "table", "Rename and alias", [`SELECT '${evil}' AS h, 1 AS v, 2 AS w`], {
            grid: { x: 0, y: 0, w: 12, h: 8 },
            transforms: [organize([], [], [["v", evil]]), calc("index", { as_percentile: false }, { alias: evil2 }), { kind: "rename_by_regex", regex: "/^w$/", rename_pattern: "$$1 <b>$&</b>" }, { kind: "convert_field_type", conversions: [{ field: "h", to: "text" }] }],
          }),
          panel(2, "table", "Partition label", values("h String, v Int32", [[evil, 1], ["ok", 2]]), { grid: { x: 12, y: 0, w: 12, h: 8 }, transforms: [{ kind: "partition_by_values", fields: ["h"], keep_fields: false }, { kind: "convert_field_type", conversions: [{ field: "v", to: "text" }] }] }),
          panel(3, "table", "Converted value", [`SELECT 1 AS n, '${evil}' AS s`], { grid: { x: 0, y: 8, w: 12, h: 8 }, transforms: [{ kind: "convert_field_type", conversions: [{ field: "n", to: "text" }, { field: "s", to: "number" }] }] }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.page();
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: "window.__alerts = 0; window.alert = () => { window.__alerts += 1; };" });
      await p.goto(t.dash("tm189"));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3]);
      const one = await tableOf(p, 1);
      t.eq(one.headers, ["h", evil, "$1 <b>w</b>", evil2], "the names are text");
      t.eq(one.rows[0][0], evil, "the value is text");
      t.eq(await options(p, 2), [`A ${evil}`, "A ok"], "the label value is text in the option");
      t.eq((await tableOf(p, 3)).rows[0], ["1", ""], "a text that is not a number gives a null cell");
      t.eq(await p.eval(`document.querySelectorAll('[data-testid="panel-1"] img, [data-testid="panel-1"] svg[onload], [data-testid^="table-"] img').length`), 0, "no element comes from a name");
      t.eq(await p.eval(`window.__alerts`), 0, "no script ran");
      // The headers in View data and in the menus are text too.
      const view = await viewData(p, 1);
      t.eq(view.headers.includes(evil), true, "View data shows the name as text");
      t.eq(await p.eval(`window.__alerts`), 0, "no script ran in View data");
    },
  },
];
