// Browser case B217 (4.5b, 7.37, 9.19d, 10.3b, 10.3c; decisions ID-K142 and ID-K333): the annotations from a PostgreSQL
// source and a MongoDB source. A dashboard has a time series on the ClickHouse source of the run and one annotation on each
// other kind. The page sends one annotation request for each annotation, and the test hook `data-annotations` of the panel
// holds the marks of both with the colour of each annotation. The hover card shows the texts of each kind. The tab
// "Annotations" of the settings offers the sources of the three kinds, and the SQL area of each annotation has the dialect
// of its source: the MongoDB text formats as JSON, the PostgreSQL text formats as SQL.
//
// The case needs a disposable PostgreSQL 18 and a disposable MongoDB 8.0 (`startPostgres` and `startMongo` of lib/env.mjs,
// with the tag `an217`, so that B44, B45, B47 and B48 keep their own containers). The module starts them at load, but only
// when this run selects B217 (the argument `--only`). It removes the containers at exit. A build with no MongoDB driver
// refuses a `mongodb` block (8b.2): the case is then a `skip`. All data is synthetic (tests/fixtures/postgres-source and
// tests/fixtures/mongo-source).
import { NAMED_LIGHT, parseCss } from "../../../web/js/colors.js";
import { doc, panel, section } from "../lib/docs.mjs";
import { DOCKER, MG_ID, PG_FROM, PG_ID, pgContainerName, probeMongoDriver, sh, startMongo, startPostgres } from "../lib/env.mjs";
import { enterEdit, moreItem } from "../lib/ed.mjs";
import { sleep } from "../lib/h.mjs";

const CASES = ["B217"];
const NO_DRIVER = "no MongoDB driver in this build";
const TAG = "an217";

function selected() {
  const i = process.argv.indexOf("--only");
  if (i < 0) return true;
  return String(process.argv[i + 1] || "")
    .split(",")
    .some((id) => CASES.includes(id));
}

let probe = null;
let start = null;
if (selected() && process.argv.includes("--cases-dir") === false) {
  probe = probeMongoDriver();
  if (probe.driver) {
    const names = [pgContainerName(TAG), `hune-test-browser-mongo-${process.pid}-${TAG}`];
    process.on("exit", () => {
      for (const name of names) {
        try {
          sh(DOCKER, ["rm", "-f", "-v", name]);
        } catch {
          // ignore
        }
      }
    });
    // A failure is kept and thrown by the case: an unhandled rejection would stop the runner.
    start = Promise.all([startPostgres(TAG), startMongo(TAG)]).then(
      ([pg, mg]) => ({ pg, mg }),
      (error) => ({ error }),
    );
  }
}

async function servers() {
  if (probe && probe.error) throw new Error("MongoDB probe: " + probe.error);
  if (!start) throw new Error("PostgreSQL and MongoDB were not started for this run");
  const r = await start;
  if (r.error) throw new Error("PostgreSQL or MongoDB: " + r.error.message);
  return r;
}

const MINUTE = 60000;
// The range: 00:00 to 00:30 on 2026-01-01, in UTC. The data of both fixtures start at 00:00.
const FROM = PG_FROM;
const TO = PG_FROM + 30 * MINUTE;
const SERIES = "SELECT toDateTime('2026-01-01 00:00:00','UTC') + number*60 AS time, number AS v FROM numbers(31)";
// node_load of PostgreSQL: one row each minute for each host. OFFSET 10 gives the rows of 00:10 and 00:11 of node-a.
const PG_SQL = [
  "SELECT $__time(created_at), host AS title, 'load ' || load::text AS text, ARRAY[env, host] AS tags",
  "FROM node_load WHERE $__timeFilter(created_at) AND host = 'node-a'",
  "ORDER BY created_at LIMIT 2 OFFSET 10",
];
// node_events of MongoDB: one document each 30 s, the level `error` for i % 5 = 4. The first two are i = 4 (00:02:00) and
// i = 9 (00:04:30), each a region of 60 s.
const MG_SQL = [
  '{"collection":"node_events","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom","$lt":"$__timeTo"},"level":"error"}},{"$sort":{"ts":1}},{"$limit":2},{"$project":{"_id":0,"time":"$ts","timeEnd":{"$add":["$ts",60000]},"title":"$node","text":"$message","tags":1}}]}',
];

const plot = (id) => `document.querySelector('[data-testid="panel-${id}"] .panel__plot')`;
const hook = (p, id) => p.eval(`JSON.parse(${plot(id)}.dataset.annotations)`);
const inkOf = (c, scheme) => {
  const v = parseCss(scheme === "light" && NAMED_LIGHT[c] ? NAMED_LIGHT[c] : c);
  return `rgb(${v.r} ${v.g} ${v.b} / ${v.a})`;
};
/** The client position of a point (x, y) of the plot of a panel. */
const plotPoint = (p, id, x, y) => p.eval(`(() => { const r = document.querySelector('[data-testid="panel-${id}"] .chartCore__plot').getBoundingClientRect(); return { x: r.left + ${x}, y: r.top + ${y} }; })()`);
const moveTo = async (p, id, x, y) => {
  const c = await plotPoint(p, id, x, y);
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: c.x - 1, y: c.y });
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: c.x, y: c.y });
  await sleep(40);
};
/** The card of a panel: the visibility, the head, the body and the texts of the chips. */
const cardState = (p, id) => p.eval(`(() => {
  const c = document.querySelector('[data-testid="annot-${id}-card"]');
  return { hidden: c.hidden || getComputedStyle(c).display === "none", head: c.children[0].textContent, body: c.children[1].textContent, tags: [...c.children[2].children].map((e) => e.textContent) };
})()`);
/** The state of the SQL area of the open form, after a click on "Format". */
const formatArea = async (p) => {
  await p.click('[data-testid="settings-annotations"] [data-testid="sql-format"]');
  await sleep(200);
  return p.eval(`(() => { const r = document.querySelector('[data-testid="settings-annotations"]'); return { value: r.querySelector('[data-testid="sql-input"]').value, status: r.querySelector('[data-testid="sql-status"]').textContent }; })()`);
};

export const cases = [
  {
    id: "B217",
    name: "annotations from a PostgreSQL source and a MongoDB source: one request each, the marks of both in the hook with their colours, the card texts, the sources of the three kinds in the tab Annotations, the dialect of each SQL area (9.19d, 10.3c)",
    skip: probe && probe.driver === false ? NO_DRIVER : undefined,
    async run(t) {
      const { pg, mg } = await servers();
      const server = await t.ownServer("an217", { postgres: pg, mongodb: mg });
      const d = {
        ...doc("an217", "Annotations of two kinds", [section(null, [panel(1, "timeseries", "Series", SERIES, { grid: { x: 0, y: 0, w: 24, h: 9 } })])], {
          time: { from: String(FROM), to: String(TO), timezone: "utc", refresh: null },
        }),
        annotations: [
          { name: "Load", source: PG_ID, sql: PG_SQL, color: "red" },
          { name: "Errors", source: MG_ID, sql: MG_SQL, color: "blue" },
        ],
      };
      const made = await fetch(server.url + "api/dashboards", {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ folder: "transit", message: "", document: d }),
      });
      t.assert(made.status === 201, `create an217: ${made.status} ${(await made.text()).slice(0, 400)}`);

      const p = await t.page({ width: 1280, height: 900 });
      await p.emulateColorScheme("dark");
      await p.goto(server.url + `d/an217?from=${FROM}&to=${TO}&timezone=utc`);
      await p.idle();
      await p.waitFor(`(() => { const e = ${plot(1)}; return !!e && !!e.dataset.annotations && JSON.parse(e.dataset.annotations).n === 4; })()`, { what: "4 marks on panel 1" });
      await p.idle();

      // One request for each annotation, with the index in the path and no SQL in the body (6.21a).
      const reqs = p.reqs("/annotations/", "POST");
      t.eq(reqs.map((r) => Number(/\/annotations\/(\d+)\/query$/.exec(r.url)[1])).sort(), [0, 1], "one annotation request for each kind");
      t.assert(reqs.every((r) => !/SELECT|aggregate/.test(r.body || "")), "the body of an annotation request holds no query text");

      // The marks of both kinds in the order of the time: the two MongoDB regions, then the two PostgreSQL points.
      const h = await hook(p, 1);
      const [L, , W, H] = h.plot;
      const x = (minutes) => L + (minutes * MINUTE * W) / (TO - FROM);
      t.eq([h.n, h.drawn, h.cut], [4, 4, false], "4 marks apply and show, no cut");
      const want = [[2, 3, 1, "blue", 1], [4.5, 5.5, 1, "blue", 1], [10, 10, 0, "red", 0], [11, 11, 0, "red", 0]];
      h.marks.forEach((m, i) => {
        const [a, b, kind, color, index] = want[i];
        t.assert(Math.abs(m[0] - x(a)) <= 1 && Math.abs(m[1] - x(b)) <= 1, `mark ${i} at ${m[0]}, ${m[1]}; expected ${x(a)}, ${x(b)}`);
        t.eq([m[2], m[3], m[4]], [kind, inkOf(color, "dark"), index], `mark ${i}: the kind, the colour and the annotation`);
      });

      // The card: the texts of a PostgreSQL row (a float8 as text, a text[] of tags) and of a MongoDB document.
      const bottom = h.plot[1] + H;
      await moveTo(p, 1, x(10), bottom - 5);
      let c = await cardState(p, 1);
      t.eq([c.hidden, c.head, c.body, c.tags], [false, "2026-01-01 00:10:00", "node-a\nload 10", ["prod", "node-a"]], "the card of a PostgreSQL mark");
      await moveTo(p, 1, x(2.5), bottom - 5);
      c = await cardState(p, 1);
      t.eq([c.hidden, c.head, c.body, c.tags], [false, "2026-01-01 00:02:00 - 2026-01-01 00:03:00", "node-04\nsynthetic event 4", ["t1", "t4"]], "the card of a MongoDB region");

      // The tab "Annotations": the select offers the sources of the three kinds (4.5b, 10.3b).
      await enterEdit(p);
      await moreItem(p, "dash-settings");
      await p.click(p.tid("settings-tab-annotations"));
      await p.waitSel(p.tid("annotation-list"));
      await p.click(p.tid("annotation-1-open"));
      await p.waitSel(p.tid("annotation-form-name"));
      const sources = await p.eval(`[...document.querySelectorAll('[data-testid="annotation-form-source"] option')].map((o) => o.value)`);
      t.eq([...sources].sort(), ["mongo", "pg", "prod", "zdown"], "the select offers the ClickHouse, PostgreSQL and MongoDB sources");
      t.eq(await p.eval(`document.querySelector('[data-testid="annotation-form-source"]').value`), MG_ID, "the form shows the MongoDB source");
      // The SQL area of the MongoDB annotation reads its text as JSON (10.3c): the format writes `": "` and keeps the tokens.
      const json = await formatArea(p);
      t.eq(json.status, "Formatted.", "the MongoDB text formats");
      t.assert(json.value.includes('"aggregate": [') && json.value.replace(/\s+/g, "") === MG_SQL[0].replace(/\s+/g, ""), `the JSON format keeps the tokens: ${json.value.slice(0, 200)}`);
      // The SQL area of the PostgreSQL annotation reads its text as SQL.
      await p.click(p.tid("annotation-back"));
      await p.click(p.tid("annotation-0-open"));
      await p.waitSel(p.tid("annotation-form-name"));
      t.eq(await p.eval(`document.querySelector('[data-testid="annotation-form-source"]').value`), PG_ID, "the form shows the PostgreSQL source");
      const sql = await formatArea(p);
      t.eq(sql.status, "Formatted.", "the PostgreSQL text formats");
      t.assert(/\nFROM node_load\b/.test(sql.value) && sql.value.includes("load::text"), `the SQL format puts FROM on its own line and keeps the cast: ${sql.value.slice(0, 300)}`);
    },
  },
];
