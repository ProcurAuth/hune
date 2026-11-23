// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B44 and B45 (16.10): the PostgreSQL source in the page.
// B44 a table with int8, numeric and timestamptz columns, a time series with `$__timeGroupAlias`, a refused function in
// the editor, and the kind of each source in the source picker. B45 the scanner of 9.10 reads a variable after `#`.
//
// The cases need a disposable PostgreSQL 18 (the local image `postgres:18`, see `startPostgres` in lib/env.mjs). The module
// starts it at load, but only when this run selects B44 or B45 (the argument `--only`). It removes the container at exit.
// All data is synthetic (tests/fixtures/postgres-source/init.sql).
import { doc, panel, section } from "../lib/docs.mjs";
import { DOCKER, PG_FROM, PG_ID, PG_TO, sh, startPostgres } from "../lib/env.mjs";
import { enterEdit, openPanelEditor } from "../lib/ed.mjs";
import { panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const CASES = ["B44", "B45"];

function selected() {
  const i = process.argv.indexOf("--only");
  if (i < 0) return true;
  return String(process.argv[i + 1] || "")
    .split(",")
    .some((id) => CASES.includes(id));
}

let pgStart = null;
if (selected() && process.argv.includes("--cases-dir") === false) {
  const name = `hune-test-browser-pg-${process.pid}`;
  process.on("exit", () => {
    try {
      sh(DOCKER, ["rm", "-f", "-v", name]);
    } catch {
      // ignore
    }
  });
  // A failure is kept and thrown by the case: an unhandled rejection would stop the runner.
  pgStart = startPostgres().then(
    (pg) => ({ pg }),
    (error) => ({ error }),
  );
}

async function postgres() {
  if (!pgStart) throw new Error("PostgreSQL was not started for this run");
  const r = await pgStart;
  if (r.error) throw new Error("PostgreSQL: " + r.error.message);
  // The container of a run starts when the module loads, long before the case. Make sure it still runs: a source
  // on a removed container shows 503 source_unavailable with no other sign.
  try {
    r.pg.query("SELECT 1");
  } catch (e) {
    throw new Error(`the PostgreSQL container ${r.pg.name} is gone before the case: ${e.message}`);
  }
  return r.pg;
}

const range = `from=${PG_FROM}&to=${PG_TO}`;

export const cases = [
  {
    id: "B44",
    name: "postgres source: exact texts of int8, numeric and timestamptz, a time series, a refused function, the kind in the source picker",
    async run(t) {
      const pg = await postgres();
      const server = await t.ownServer("pg", { postgres: pg });
      const url = (path) => server.url + path;
      const d = doc("pg-source", "PostgreSQL source", [
        section(null, [
          panel(
            1,
            "table",
            "Types",
            ["SELECT c_int8, c_numeric_38, c_timestamptz", "FROM metrics.types_all", "WHERE id IN (1, 2)", "ORDER BY id"],
            // The page rounds a `number_text` with no `decimals` to the automatic decimals (9.15). The override keeps the
            // scale of numeric(38,9) visible, so the case checks the exact text that the source gives.
            { source: PG_ID, grid: { x: 0, y: 0, w: 12, h: 8 }, fields: { defaults: {}, overrides: [{ match: { by: "name", value: "c_numeric_38" }, set: { decimals: 9 } }] } },
          ),
          panel(
            2,
            "timeseries",
            "Load",
            [
              "SELECT $__timeGroupAlias(created_at, $__interval), host, avg(load) AS load",
              "FROM metrics.node_load",
              "WHERE $__timeFilter(created_at)",
              "GROUP BY 1, 2",
              "ORDER BY 1",
            ],
            { source: PG_ID, grid: { x: 12, y: 0, w: 12, h: 8 } },
          ),
        ]),
      ]);
      const made = await fetch(url("api/dashboards"), {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ folder: "transit", message: "", document: d }),
      });
      t.assert(made.status === 201, `create pg-source: ${made.status} ${(await made.text()).slice(0, 400)}`);

      // The source list holds the kind of each source (6.22).
      const list = await (await fetch(url("api/sources"))).json();
      const kinds = Object.fromEntries(list.items.map((s) => [s.id, s.kind]));
      t.eq(kinds[PG_ID], "postgres", "kind of the postgres source: " + JSON.stringify(list));
      t.eq(kinds.prod, "clickhouse", "kind of the ClickHouse source");

      const p = await t.page();
      await p.goto(url(`d/pg-source?${range}`));
      await p.idle();
      await waitPanelsReady(p, [1, 2]);

      // Table: exact texts (7.16a, 9.15).
      const cells = await p.eval(`(() => {
        const t = document.querySelector('[data-testid="table-1"] table');
        const heads = [...t.querySelectorAll("th")].map((x) => x.textContent.trim());
        const rows = [...t.querySelectorAll("tbody tr")].map((r) => [...r.children].map((c) => c.textContent.trim()));
        return { heads, rows };
      })()`);
      t.eq(cells.heads.slice(0, 3), ["c_int8", "c_numeric_38", "c_timestamptz"], "table headers");
      t.eq(cells.rows.length, 2, "two rows");
      t.eq(cells.rows[0][0], "4503599627370496", "int8 above 2^52 is exact");
      t.eq(cells.rows[1][0], "9223372036854775807", "int8 maximum is exact");
      t.eq(cells.rows[0][1], "12.500000000", "numeric(38,9) keeps its scale");
      t.eq(cells.rows[1][1], "99999999999999999999999999999.999999999", "numeric(38,9) is exact");
      t.assert(/2026/.test(cells.rows[0][2]), "timestamptz shows a date of 2026: " + cells.rows[0][2]);

      // Time series: the macro `$__timeGroupAlias` with `$__interval` gives three series (7.16b).
      const ts = await p.eval(`(() => {
        const c = document.querySelector('[data-testid="panel-2"] .chartCore');
        return c ? { type: c.dataset.type, series: c.dataset.seriesDrawn, points: Number(c.dataset.pointsDrawn) } : null;
      })()`);
      t.assert(ts !== null, "the time series draws a chart");
      t.eq([ts.type, ts.series], ["line", "3"], "three series, one for each host");
      t.assert(ts.points > 3, "points drawn: " + ts.points);
      const q2 = panelQueries(p).filter((r) => /\/panels\/2\/query$/.test(r.url));
      t.eq(q2.map((r) => r.status), [200], "the query of panel 2 answers 200");

      // The editor: the source picker shows the kind, and a refused function shows sql_invalid with its line.
      await enterEdit(p);
      await openPanelEditor(p, 2);
      const options = await p.eval(`(() => {
        for (const l of document.querySelectorAll('[data-testid="panel-editor"] label')) {
          if (l.textContent.trim() === "Source" && l.htmlFor) {
            return [...document.getElementById(l.htmlFor).options].map((o) => ({ value: o.value, text: o.textContent }));
          }
        }
        return [];
      })()`);
      const text = Object.fromEntries(options.map((o) => [o.value, o.text]));
      t.assert(/PostgreSQL/.test(text[PG_ID] || ""), "the picker shows the kind of the postgres source: " + JSON.stringify(options));
      t.assert(/ClickHouse/.test(text.prod || ""), "the picker shows the kind of the ClickHouse source: " + JSON.stringify(options));

      t.allow(p, /Failed to load resource/);
      await p.fill(p.tid("sql-input"), "SELECT 1\n, pg_sleep(1)");
      await sleep(450);
      p.clearRequests();
      await p.click(p.tid("run-button"));
      await p.waitFor(`!document.querySelector('[data-testid="panel-errors"]').hidden`, { what: "error box" });
      const box = await p.text(p.tid("panel-errors"));
      t.assert(/line 2, column \d+/.test(box), "the error shows its line: " + box);
      const pv = p.requests.filter((r) => r.method === "POST" && /\/api\/sources\/[^/]+\/preview$/.test(r.url));
      t.eq(pv.length, 1, "one preview request");
      t.assert(pv[0].url.endsWith(`/api/sources/${PG_ID}/preview`), "the preview goes to the postgres source");
      t.eq(pv[0].status, 422, "a refused function gives 422");
    },
  },

  {
    id: "B45",
    name: "postgres source: the scanner finds a variable after `#`, and a change of the variable runs the panel again",
    async run(t) {
      const pg = await postgres();
      const server = await t.ownServer("pg45", { postgres: pg });
      // `#` is the operator XOR of PostgreSQL. The lexer of ClickHouse reads `# ` as a comment (9.10).
      const d = doc("pg-hash", "PostgreSQL hash operator", [
        section(null, [panel(1, "table", "Xor", ["SELECT 6 # ${n}::int AS xor_value"], { source: PG_ID, grid: { x: 0, y: 0, w: 24, h: 6 } })]),
      ], {
        variables: [{ kind: "text", name: "n", label: "N", hide: "none", value_type: "text", default: "3" }],
      });
      const r = await fetch(server.url + "api/dashboards", {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ folder: "transit", message: "", document: d }),
      });
      t.assert(r.status === 201, `create pg-hash: ${r.status} ${(await r.text()).slice(0, 400)}`);
      const p = await t.page();
      await p.goto(server.url + `d/pg-hash?${range}`);
      await p.idle();
      await waitPanelsReady(p, [1]);
      const value = () => p.eval(`(document.querySelector('[data-testid="table-1"] tbody tr td') || {textContent: ""}).textContent.trim()`);
      t.eq(await value(), "5", "6 # 3");
      const first = JSON.parse(panelQueries(p)[0].body);
      t.eq(first.variables.n.values, ["3"], "the request holds the variable");
      // A change of the variable runs the panel again: the scanner found the name after `#`.
      p.clearRequests();
      await p.fill(p.tid("variable-n-input"), "1");
      t.eq(panelQueries(p).length, 0, "no query before Enter");
      await p.press("Enter");
      await p.waitFor(`(document.querySelector('[data-testid="table-1"] tbody tr td') || {textContent: ""}).textContent.trim() === "7"`, { what: "6 # 1 = 7" });
      t.eq(panelQueries(p).length, 1, "one panel query for the change");
      t.eq(await value(), "7", "6 # 1");
    },
  },
];
