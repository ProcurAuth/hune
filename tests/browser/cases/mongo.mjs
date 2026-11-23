// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B47 and B48 (16.10): the MongoDB source in the page.
// B47 a table with exact texts (Int64, Decimal128, Date), a time series with `$dateTrunc` and `$__interval_s`, the kind of
// each source in the source picker, and a refused stage (`$out`) in the editor. B48 an empty result, a nested document,
// a time series with no Date column, and the scanner of 9.10 on the string "${n:number}".
//
// This build of Hune may have no MongoDB driver: a server with a `mongodb` block then does not start (8b.2). The module
// asks the binary at load (`hune check-config`, see `probeMongoDriver` in lib/env.mjs). When the binary refuses the block
// for this reason, both cases are reported as `skip` with the reason "no MongoDB driver in this build". Any other failure
// of the probe fails the case. When the build has the driver, the module starts a disposable MongoDB 8 (the light data of
// tests/fixtures/mongo-source/init.js, see `startMongo` in lib/env.mjs) and removes it at exit.
// The module does all this only when this run selects B47 or B48 (the argument `--only`). All data is synthetic.
import { doc, panel, section } from "../lib/docs.mjs";
import { DOCKER, MG_FROM, MG_ID, MG_TO, probeMongoDriver, sh, startMongo } from "../lib/env.mjs";
import { enterEdit, openPanelEditor } from "../lib/ed.mjs";
import { panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const CASES = ["B47", "B48"];
const NO_DRIVER = "no MongoDB driver in this build";

function selected() {
  const i = process.argv.indexOf("--only");
  if (i < 0) return true;
  return String(process.argv[i + 1] || "")
    .split(",")
    .some((id) => CASES.includes(id));
}

let probe = null;
let mgStart = null;
if (selected() && process.argv.includes("--cases-dir") === false) {
  probe = probeMongoDriver();
  if (probe.driver) {
    const name = `hune-test-browser-mongo-${process.pid}`;
    process.on("exit", () => {
      try {
        sh(DOCKER, ["rm", "-f", "-v", name]);
      } catch {
        // ignore
      }
    });
    // A failure is kept and thrown by the case: an unhandled rejection would stop the runner.
    mgStart = startMongo().then(
      (mg) => ({ mg }),
      (error) => ({ error }),
    );
  }
}

async function mongo() {
  if (probe && probe.error) throw new Error("MongoDB probe: " + probe.error);
  if (!mgStart) throw new Error("MongoDB was not started for this run");
  const r = await mgStart;
  if (r.error) throw new Error("MongoDB: " + r.error.message);
  return r.mg;
}

const range = `from=${MG_FROM}&to=${MG_TO}`;

/** The text of a query as lines of the panel (4.15): one JSON text, cut at line feeds. */
const lines = (text) => text.split("\n");

async function create(server, d) {
  const made = await fetch(server.url + "api/dashboards", {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
    body: JSON.stringify({ folder: "transit", message: "", document: d }),
  });
  if (made.status !== 201) throw new Error(`create ${d.uid}: ${made.status} ${(await made.text()).slice(0, 400)}`);
}

/** The texts of the cells of a table panel, as `{ heads, rows }`. */
const tableOf = (p, id) =>
  p.eval(`(() => {
    const t = document.querySelector('[data-testid="table-${id}"] table');
    const heads = [...t.querySelectorAll("th")].map((x) => x.textContent.trim());
    const rows = [...t.querySelectorAll("tbody tr")].map((r) => [...r.children].map((c) => c.textContent.trim()));
    return { heads, rows };
  })()`);

const TYPES_QUERY = `{"collection":"types",
 "aggregate":[
  {"$match":{"_id":1}},
  {"$project":{"_id":0,"above_2_52":{"$toLong":"4503599627370497"},"t_int64_max":1,"t_decimal":1,"t_date_ms":1}}
 ]}`;

const SERIES_QUERY = `{"collection":"node_load",
 "aggregate":[
  {"$match":{"ts":{"$gte":"$__timeFrom","$lte":"$__timeTo"},"node":{"$in":["node-00","node-01","node-02"]}}},
  {"$group":{"_id":{"t":{"$dateTrunc":{"date":"$ts","unit":"second","binSize":"$__interval_s"}},"host":"$node"},"cpu":{"$avg":"$cpu"}}},
  {"$project":{"_id":0,"time":"$_id.t","host":"$_id.host","cpu":1}},
  {"$sort":{"time":1}}
 ]}`;

// The refused stage `$out` is on line 3 of this text.
const OUT_QUERY = `{"collection":"node_load",
 "aggregate":[
  {"$out":"copy"}
 ]}`;

export const cases = [
  {
    id: "B47",
    name: "mongodb source: exact texts of Int64, Decimal128 and Date, a time series, the kind in the source picker, a refused $out",
    skip: probe && probe.driver === false ? NO_DRIVER : undefined,
    async run(t) {
      const mg = await mongo();
      const server = await t.ownServer("mg47", { mongodb: mg });
      const url = (path) => server.url + path;
      const d = doc("mongo-source", "MongoDB source", [
        section(null, [
          panel(1, "table", "Types", lines(TYPES_QUERY), {
            source: MG_ID,
            grid: { x: 0, y: 0, w: 12, h: 8 },
            // The page rounds a `number_text` with no `decimals` to the automatic decimals (9.15). The override keeps the
            // scale of the Decimal128 visible, so the case checks the exact text that the source gives.
            fields: { defaults: {}, overrides: [{ match: { by: "name", value: "t_decimal" }, set: { decimals: 4 } }] },
          }),
          panel(2, "timeseries", "Load", lines(SERIES_QUERY), { source: MG_ID, grid: { x: 12, y: 0, w: 12, h: 8 } }),
        ]),
      ]);
      await create(server, d);

      // The source list holds the kind of each source (6.22).
      const list = await (await fetch(url("api/sources"))).json();
      const kinds = Object.fromEntries(list.items.map((s) => [s.id, s.kind]));
      t.eq(kinds[MG_ID], "mongodb", "kind of the mongodb source: " + JSON.stringify(list));
      t.eq(kinds.prod, "clickhouse", "kind of the ClickHouse source");

      const p = await t.page();
      await p.goto(url(`d/mongo-source?${range}`));
      await p.idle();
      await waitPanelsReady(p, [1, 2]);

      // Table: exact texts (7.16c, 9.15).
      const cells = await tableOf(p, 1);
      t.eq(cells.rows.length, 1, "one row");
      const cell = (name) => cells.rows[0][cells.heads.indexOf(name)];
      t.assert(cells.heads.includes("above_2_52"), "header above_2_52: " + JSON.stringify(cells.heads));
      t.eq(cell("above_2_52"), "4503599627370497", "an Int64 above 2^52 is exact");
      t.eq(cell("t_int64_max"), "9223372036854775807", "the Int64 maximum is exact");
      t.eq(cell("t_decimal"), "12345.6700", "a Decimal128 keeps its scale");
      t.assert(/2026/.test(cell("t_date_ms") || ""), "a Date of 2026: " + cell("t_date_ms"));

      // Time series: `$dateTrunc` with `$__interval_s` gives three series, one for each host (7.16d).
      const ts = await p.eval(`(() => {
        const c = document.querySelector('[data-testid="panel-2"] .chartCore');
        return c ? { type: c.dataset.type, series: c.dataset.seriesDrawn, points: Number(c.dataset.pointsDrawn) } : null;
      })()`);
      t.assert(ts !== null, "the time series draws a chart");
      t.eq([ts.type, ts.series], ["line", "3"], "three series, one for each host");
      t.assert(ts.points > 3, "points drawn: " + ts.points);
      const q2 = panelQueries(p).filter((r) => /\/panels\/2\/query$/.test(r.url));
      t.eq(q2.map((r) => r.status), [200], "the query of panel 2 answers 200");

      // The editor: the source picker shows "Label (MongoDB, <state>)".
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
      t.assert(/^Events \(MongoDB, [a-z]+\)$/.test(text[MG_ID] || ""), "the picker shows the kind of the mongodb source: " + JSON.stringify(options));
      t.assert(/ClickHouse/.test(text.prod || ""), "the picker shows the kind of the ClickHouse source: " + JSON.stringify(options));

      // A refused stage: `sql_invalid` with its line, from the preview.
      t.allow(p, /Failed to load resource/);
      await p.fill(p.tid("sql-input"), OUT_QUERY);
      await sleep(450);
      p.clearRequests();
      await p.click(p.tid("run-button"));
      await p.waitFor(`!document.querySelector('[data-testid="panel-errors"]').hidden`, { what: "error box" });
      const box = await p.text(p.tid("panel-errors"));
      t.assert(/line 3\b/.test(box), "the error names line 3: " + box);
      const pv = p.requests.filter((r) => r.method === "POST" && /\/api\/sources\/[^/]+\/preview$/.test(r.url));
      t.eq(pv.length, 1, "one preview request");
      t.assert(pv[0].url.endsWith(`/api/sources/${MG_ID}/preview`), "the preview goes to the mongodb source");
      t.eq(pv[0].status, 422, "a refused stage gives 422");
    },
  },

  {
    id: "B48",
    name: "mongodb source: an empty result, a nested document as JSON text, a time series with no Date column, the scanner on ${n:number}",
    skip: probe && probe.driver === false ? NO_DRIVER : undefined,
    async run(t) {
      const mg = await mongo();
      const server = await t.ownServer("mg48", { mongodb: mg });
      const d = doc(
        "mongo-edge",
        "MongoDB edge cases",
        [
          section(null, [
            panel(1, "table", "Empty", lines('{"collection":"node_load","aggregate":[{"$match":{"_id":-1}}]}'), { source: MG_ID, grid: { x: 0, y: 0, w: 12, h: 6 } }),
            panel(2, "table", "Nested", lines('{"collection":"node_events","find":{"filter":{"_id":0},"projection":{"_id":0,"attrs":1}}}'), {
              source: MG_ID,
              grid: { x: 12, y: 0, w: 12, h: 6 },
            }),
            // A text column and no Date column: the page shows the error of 9.22.
            panel(3, "timeseries", "No time", lines('{"collection":"node_events","aggregate":[{"$limit":3},{"$project":{"_id":0,"node":1,"code":1}}]}'), {
              source: MG_ID,
              format: "time_series",
              grid: { x: 0, y: 6, w: 12, h: 6 },
            }),
            panel(
              4,
              "table",
              "Variable",
              lines('{"collection":"node_load","aggregate":[{"$match":{"_id":"${n:number}"}},{"$project":{"_id":0,"node":1}}]}'),
              { source: MG_ID, grid: { x: 12, y: 6, w: 12, h: 6 } },
            ),
          ]),
        ],
        { variables: [{ kind: "text", name: "n", label: "N", hide: "none", value_type: "text", default: "3" }] },
      );
      await create(server, d);

      const p = await t.page();
      // The browser logs a failed fetch status as a console error: panel 3 gives 422.
      t.allow(p, /Failed to load resource/);
      await p.goto(server.url + `d/mongo-edge?${range}`);
      await p.idle();
      await waitPanelsReady(p, [1], "empty");
      await waitPanelsReady(p, [2, 4], "ready");
      await waitPanelsReady(p, [3], "error");

      // A query that finds no document: the state `empty`, and no column (7.16c).
      t.eq(await p.count('[data-testid="table-1"] th'), 0, "the empty frame has no column");

      // A nested document: one cell with its compact JSON text (7.16c).
      const nested = await tableOf(p, 2);
      t.eq(nested.heads, ["attrs"], "one column");
      t.assert(/^\{"shard":0(\.0)?,"retry":true\}$/.test(nested.rows[0][0]), "the nested document is a JSON text: " + nested.rows[0][0]);

      // A time series with no Date column: the error of 9.22.
      const e3 = await p.text(p.tid("panel-3-error"));
      t.assert(/A time series query needs a time column/.test(e3), "panel 3 error: " + e3);

      // The scanner finds `n` in the string "${n:number}" (9.10): the request holds the variable.
      const value = async () => (await tableOf(p, 4)).rows[0]?.[0];
      t.eq(await value(), "node-03", "_id 3");
      const first = JSON.parse(panelQueries(p).filter((r) => /\/panels\/4\/query$/.test(r.url))[0].body);
      t.eq(first.variables.n.values, ["3"], "the request holds the variable");
      // A new value runs one panel query: only the panel that names `n`.
      p.clearRequests();
      await p.fill(p.tid("variable-n-input"), "4");
      t.eq(panelQueries(p).length, 0, "no query before Enter");
      await p.press("Enter");
      await p.waitFor(`(document.querySelector('[data-testid="table-4"] tbody tr td') || {textContent: ""}).textContent.trim() === "node-04"`, { what: "_id 4" });
      const after = panelQueries(p);
      t.eq(after.length, 1, "one panel query for the change");
      t.assert(/\/panels\/4\/query$/.test(after[0].url), "the query is the one of panel 4: " + after[0].url);
      t.eq(await value(), "node-04", "_id 4");
    },
  },
];
