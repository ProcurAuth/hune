// Tests of the parity capture tool (15.14a): the normaliser, the comparison, the causes and the refusals of the command.
// No browser, no Grafana and no ClickHouse run here: the functions of tools/parity/capture-lib.mjs are pure.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  compareData,
  comparePanelQuery,
  compareSql,
  compareSqlByHash,
  explainedByInterval,
  insideGitCheckout,
  intervalSeconds,
  namedRange,
  normaliseSql,
  pairVariable,
  parseTime,
  parseVar,
  renderMarkdown,
  sha256,
  stateQuery,
  substituteInterval,
  totals,
} from "../../tools/parity/capture-lib.mjs";
import { checkQueryLog, logHashes, queryLog } from "../../tools/parity/capture.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

test("normaliser: blanks outside literals and comments become one space", () => {
  assert.equal(normaliseSql("  SELECT  a,\n\tb\r\nFROM   t  "), "SELECT a, b FROM t");
  assert.equal(normaliseSql("SELECT a"), "SELECT a");
  assert.equal(normaliseSql(""), "");
  assert.equal(normaliseSql(" \n "), "");
});

test("normaliser: a literal, a quoted name and a comment stay as they are", () => {
  assert.equal(normaliseSql("x  =  'a   b'  AND  \"c   d\""), "x = 'a   b' AND \"c   d\"");
  assert.equal(normaliseSql("a  /* x   y */  b"), "a /* x   y */ b");
  // A line comment ends before the line break (7.5), so the break is a blank outside the comment.
  assert.equal(normaliseSql("a  -- x   y\n  b"), "a -- x   y b");
  assert.equal(normaliseSql("a  # x   y\n  b"), "a # x   y b");
  assert.equal(normaliseSql("a #x   y"), "a #x y");
  assert.equal(normaliseSql("a  /* x /* y */   z */  b"), "a /* x /* y */   z */ b");
  assert.equal(normaliseSql("'it''s   ok'  +  1"), "'it''s   ok' + 1");
  assert.equal(normaliseSql("'a\\'  b'  c"), "'a\\'  b' c");
  assert.equal(normaliseSql("$$a   b$$   c"), "$$a   b$$ c");
});

test("normaliser: it is idempotent and equal texts stay equal", () => {
  const t = "WITH  x AS (SELECT  1)  -- c  d\nSELECT *  FROM  x";
  assert.equal(normaliseSql(normaliseSql(t)), normaliseSql(t));
  assert.equal(normaliseSql("SELECT 1\n"), normaliseSql("SELECT   1"));
});

test("interval seconds follow 7.4", () => {
  assert.equal(intervalSeconds(60000), 60);
  assert.equal(intervalSeconds(500), 1);
  assert.equal(intervalSeconds(1999), 1);
});

test("compare: blank differences only are equal", () => {
  const r = compareSql({ grafana: "SELECT  1\nFROM t", hune: "SELECT 1 FROM t" });
  assert.deepEqual(r, { status: "equal", cause: null, detail: null });
});

test("compare: a difference of the interval gets the cause interval", () => {
  const g = "SELECT toStartOfInterval(t, INTERVAL 30 second) FROM x WHERE n = 30000";
  const v = "SELECT toStartOfInterval(t, INTERVAL 20 second) FROM x WHERE n = 20000";
  assert.equal(explainedByInterval(g, v, 30000, 20000), true);
  const r = compareSql({ grafana: g, hune: v, grafanaIntervalMs: 30000, huneIntervalMs: 20000 });
  assert.equal(r.status, "different");
  assert.equal(r.cause, "interval");
  assert.equal(r.detail.explained, true);
});

test("compare: an interval difference with another difference is other", () => {
  const g = "SELECT INTERVAL 30 second, a FROM x";
  const v = "SELECT INTERVAL 20 second, b FROM x";
  assert.equal(explainedByInterval(g, v, 30000, 20000), false);
  const r = compareSql({ grafana: g, hune: v, grafanaIntervalMs: 30000, huneIntervalMs: 20000 });
  assert.equal(r.cause, "other");
});

test("compare: a number that is not the interval is not explained", () => {
  assert.equal(explainedByInterval("a 7 b", "a 8 b", 30000, 20000), false);
  assert.equal(explainedByInterval("a 30 b", "a 20 b", 30000, 20000), true);
  assert.equal(explainedByInterval("a 30 b", "a 30 b 1", 30000, 20000), false);
});

test("compare: the escape of a value with a quote gets the cause escape", () => {
  const r = compareSql({ grafana: "WHERE a IN ('o'k')", hune: "WHERE a IN ('o\\'k')", values: ["o'k"] });
  assert.equal(r.cause, "escape");
});

test("compare: a variable text that differs without a quote gets the cause format", () => {
  const r = compareSql({ grafana: "WHERE a IN ('x','y')", hune: "WHERE a IN ('x', 'y')", values: ["x", "y"] });
  assert.equal(r.cause, "format");
});

test("compare: any other difference gets the cause other", () => {
  const r = compareSql({ grafana: "SELECT 1", hune: "SELECT 2" });
  assert.equal(r.status, "different");
  assert.equal(r.cause, "other");
  assert.ok(r.detail.grafana.includes("1") && r.detail.hune.includes("2"));
  assert.equal(compareSql({ grafana: "SELECT 1", hune: null }).cause, "other");
  assert.equal(compareSql({ grafana: null, hune: "SELECT 1" }).cause, "other");
});

test("compare by hash: equal bytes, the interval, and the rest", () => {
  const g = "SELECT INTERVAL 30 second, 30000";
  const v = "SELECT INTERVAL 20 second, 20000";
  assert.equal(sha256("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  assert.equal(substituteInterval(g, 30000, 20000), v);
  assert.equal(compareSqlByHash({ grafana: g, huneHash: sha256(g) }).status, "equal");
  const byInterval = compareSqlByHash({ grafana: g, huneHash: sha256(v), grafanaIntervalMs: 30000, huneIntervalMs: 20000 });
  assert.equal(byInterval.cause, "interval");
  assert.equal(byInterval.detail.explained, true);
  const other = compareSqlByHash({ grafana: g, huneHash: sha256("SELECT 1"), grafanaIntervalMs: 30000, huneIntervalMs: 20000 });
  assert.equal(other.cause, "other");
  assert.equal(compareSqlByHash({ grafana: g, huneHash: null }).cause, "other");
  // The hash covers the blanks: a text with other blanks differs, and the report says so.
  assert.equal(compareSqlByHash({ grafana: "SELECT  1", huneHash: sha256("SELECT 1") }).status, "different");
});

const table = (names, kinds, cols) => ({ columns: names.map((name, i) => ({ name, kind: kinds[i], values: cols[i] })) });

test("data: an exact table compares cell by cell, Grafana time in ms and Hune time in s", () => {
  const g = table(["t", "n", "s"], ["time", "number", "string"], [[1794290400000, 1794290460000], [1, 2.5], ["a", null]]);
  const v = table(["t", "n", "s"], ["time", "number_text", "text"], [[1794290400, 1794290460], ["1", "2.5"], ["a", null]]);
  assert.equal(compareData(g, v, "exact").status, "equal");
  const bad = table(["t", "n", "s"], ["time", "number_text", "text"], [[1794290400, 1794290460], ["1", "2.6"], ["a", null]]);
  const r = compareData(g, bad, "exact");
  assert.equal(r.status, "different");
  assert.deepEqual([r.detail.reason, r.detail.row, r.detail.column], ["cell", 1, "n"]);
});

test("data: columns, rows and the order of rows", () => {
  const a = table(["x"], ["text"], [["a", "b"]]);
  assert.equal(compareData(a, table(["y"], ["text"], [["a", "b"]]), "exact").detail.reason, "columns");
  assert.equal(compareData(a, table(["x"], ["text"], [["a"]]), "exact").detail.reason, "rows");
  const swapped = compareData(a, table(["x"], ["text"], [["b", "a"]]), "exact");
  assert.equal(swapped.status, "different");
  assert.equal(swapped.detail.reason, "row order");
});

test("data: the multiset mode ignores the order, the nulls and the shape", () => {
  const wide = table(["t", "a", "b"], ["time", "number", "number"], [[1000, 2000], [1, null], [null, 3]]);
  const long = table(["t", "m", "v"], ["time", "text", "number"], [[1, 2], ["a", "b"], [3, 1]]);
  assert.equal(compareData(wide, long, "multiset").status, "equal");
  const other = table(["t", "m", "v"], ["time", "text", "number"], [[1, 3], ["a", "b"], [3, 1]]);
  assert.equal(compareData(wide, other, "multiset").detail.reason, "times");
  const values = table(["t", "m", "v"], ["time", "text", "number"], [[1, 2], ["a", "b"], [3, 9]]);
  assert.equal(compareData(wide, values, "multiset").detail.reason, "values");
  assert.equal(compareData(null, long, "multiset").status, "not_compared");
});

const side = (extra = {}) => ({
  sql: "SELECT 1",
  interval_ms: 60000,
  max_data_points: 300,
  from_ms: 1,
  to_ms: 2,
  format: 1,
  table: table(["v"], ["number"], [[1]]),
  ...extra,
});

test("panel: equal sides give status equal", () => {
  const r = comparePanelQuery(side(), side(), []);
  assert.equal(r.status, "equal");
  assert.equal(r.sql.status, "equal");
});

test("panel: maxDataPoints and interval differences get the cause interval", () => {
  const r = comparePanelQuery(side({ max_data_points: 293 }), side({ max_data_points: 300 }), []);
  assert.equal(r.status, "different");
  assert.equal(r.max_data_points.cause, "interval");
  assert.equal(r.sql.status, "equal");
  const i = comparePanelQuery(side({ interval_ms: 30000 }), side({ interval_ms: 20000 }), []);
  assert.equal(i.interval.cause, "interval");
});

test("panel: a different range or an error is a difference with the cause other", () => {
  assert.equal(comparePanelQuery(side(), side({ to_ms: 3 }), []).range.cause, "other");
  const e = comparePanelQuery(side({ error: "boom" }), side(), []);
  assert.equal(e.data.cause, "other");
  assert.equal(e.data.detail.reason, "error");
});

test("panel: data of equal SQL that differs has the cause other, not interval", () => {
  const r = comparePanelQuery(side({ max_data_points: 1 }), side({ table: table(["v"], ["number"], [[2]]) }), []);
  assert.equal(r.data.status, "different");
  assert.equal(r.data.cause, "other");
});

test("totals and report", () => {
  const records = [
    comparePanelQuery(side(), side(), []),
    comparePanelQuery(side({ sql: "SELECT INTERVAL 30 second", interval_ms: 30000 }), side({ sql: "SELECT INTERVAL 20 second", interval_ms: 20000 }), []),
    comparePanelQuery(side(), side({ sql: "SELECT 2" }), []),
  ].map((r, i) => ({ id: i + 1, title: `P${i}`, type: "table", ref_id: "A", notes: [], errors: [], ...r }));
  const t = totals(records);
  assert.equal(t.queries, 3);
  assert.equal(t.equal, 1);
  assert.equal(t.different, 2);
  assert.deepEqual(t.sql, { equal: 1, different: 2 });
  assert.equal(t.sql_causes.interval, 1);
  assert.equal(t.sql_causes.other, 1);
  const md = renderMarkdown({ tool: "t", created_at: "now", run: { a: 1 }, totals: t, panels: records, variables: [], missing: [{ id: 9, title: "M", reason: "none" }] });
  assert.ok(md.includes("Panels not captured"));
  assert.ok(md.includes("Differences of the SQL"));
});

test("variable pairing: by the text, or by the text with a placeholder replaced", () => {
  const vars = [
    { name: "a", sql: "SELECT x FROM t" },
    { name: "b", sql: "SELECT y FROM t WHERE r IN ($region) AND ${env:singlequote} = 1" },
  ];
  assert.deepEqual(pairVariable(vars, "SELECT  x\nFROM t"), { name: "a", matches: 1 });
  assert.deepEqual(pairVariable(vars, "SELECT y FROM t WHERE r IN ('FRA','AMS') AND 'prod' = 1"), { name: "b", matches: 1 });
  assert.deepEqual(pairVariable(vars, "SELECT z FROM t"), { name: null, matches: 0 });
  const two = [{ name: "p", sql: "SELECT $a" }, { name: "q", sql: "SELECT $b" }];
  assert.deepEqual(pairVariable(two, "SELECT 1"), { name: null, matches: 2 });
});

test("ranges, times, variables and the URL state", () => {
  const now = Date.UTC(2026, 10, 11, 5, 30);
  const D = Date.UTC(2026, 10, 11);
  assert.deepEqual(namedRange("R1", now), { from_ms: D - 3600000, to_ms: D });
  assert.deepEqual(namedRange("R2", now), { from_ms: D - 86400000, to_ms: D });
  assert.deepEqual(namedRange("R3", now), { from_ms: D - 7 * 86400000, to_ms: D });
  assert.equal(namedRange("R4", now), null);
  assert.equal(parseTime("1794290400000"), 1794290400000);
  assert.equal(parseTime("2026-11-10T06:00:00Z"), Date.UTC(2026, 10, 10, 6));
  assert.equal(parseTime("yesterday"), null);
  assert.equal(parseTime("2026-11-10T06:00:00+01:00"), null);
  assert.deepEqual(parseVar("env=prod"), ["env", "prod"]);
  assert.deepEqual(parseVar("q=a=b"), ["q", "a=b"]);
  assert.equal(parseVar("novalue"), null);
  assert.equal(stateQuery(1, 2, [["env", "a b"], ["env", "c"]]), "from=1&to=2&var-env=a%20b&var-env=c");
});

test("git checkout detection", () => {
  assert.equal(insideGitCheckout(join(root, "target", "does", "not", "exist")), true);
  const dir = mkdtempSync(join(tmpdir(), "capture-test-"));
  try {
    assert.equal(insideGitCheckout(join(dir, "out")), false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

function run(...args) {
  return spawnSync(process.execPath, [join(root, "tools/parity/capture.mjs"), ...args], { encoding: "utf8", timeout: 20000 });
}

test("command: it refuses wrong arguments with exit code 2 before it opens a browser", () => {
  const base = ["--grafana-url", "http://127.0.0.1:1", "--grafana-uid", "g", "--hune-url", "http://127.0.0.1:2", "--hune-uid", "v"];
  const outside = mkdtempSync(join(tmpdir(), "capture-test-"));
  try {
    const out = join(outside, "out");
    const good = [...base, "--out", out, "--from", "1000000000000", "--to", "1000000003600", "--hune-sql", "log-hash", "--hune-log", "/nonexistent"];
    assert.equal(run().status, 2);
    assert.equal(run("--bogus").status, 2);
    assert.match(run("--bogus").stderr, /unknown argument/);
    // Inside a git checkout (15.18).
    const inside = run(...base, "--out", join(root, "target", "capture-test-refused"), "--from", "1000000000000", "--to", "1000000003600", "--hune-sql", "log-hash", "--hune-log", "/x");
    assert.equal(inside.status, 2);
    assert.match(inside.stderr, /inside a git checkout/);
    // A range in the future, a range that ends before it starts, a bad time.
    assert.match(run(...base, "--out", out, "--from", "1000000000000", "--to", "99999999999999", "--hune-sql", "log-hash", "--hune-log", "/x").stderr, /future/);
    assert.match(run(...base, "--out", out, "--from", "1000000003600", "--to", "1000000000000", "--hune-sql", "log-hash", "--hune-log", "/x").stderr, /before/);
    assert.match(run(...base, "--out", out, "--from", "soon", "--to", "1000000000000", "--hune-sql", "log-hash", "--hune-log", "/x").stderr, /--from/);
    // The two SQL sources have their own options.
    assert.match(run(...base, "--out", out, "--range", "R2", "--hune-sql", "query_log").stderr, /necessary/);
    assert.match(run(...base, "--out", out, "--range", "R2", "--hune-sql", "log-hash").stderr, /hune-log/);
    assert.match(run(...base, "--out", out, "--range", "R2", "--hune-sql", "log-hash", "--hune-log", "/x", "--ch-user", "u").stderr, /query_log/);
    assert.match(run(...base, "--out", out, "--range", "R2", "--hune-sql", "text", "--hune-log", "/x").stderr, /query_log or log-hash/);
    assert.match(run(...base, "--out", out, "--range", "R5", "--hune-sql", "log-hash", "--hune-log", "/x").stderr, /R1, R2 or R3/);
    // A duplicate option and a variable without a value.
    assert.match(run(...good, "--out", out).stderr, /two times/);
    assert.match(run(...good, "--var", "novalue").stderr, /NAME=VALUE/);
    // The effective configuration (no browser, no file written).
    const cfg = run(...good, "--print-config", "--var", "env=prod");
    assert.equal(cfg.status, 0);
    const parsed = JSON.parse(cfg.stdout);
    assert.equal(parsed.hune_sql, "log-hash");
    assert.deepEqual(parsed.vars, [["env", "prod"]]);
    assert.equal(parsed.window.width, 1920);
  } finally {
    rmSync(outside, { recursive: true, force: true });
  }
});

// A local HTTP server plays ClickHouse: it records the requests and answers with JSONEachRow.
async function fakeClickHouse(handler) {
  const seen = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      seen.push({ headers: req.headers, url: req.url, body });
      const [code, text] = handler(body);
      res.writeHead(code);
      res.end(text);
    });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  return { seen, url: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((r) => server.close(r)) };
}

test("query log: it sends the account in headers and reads the text of each query id", async () => {
  const id1 = "01a1c84d-ac83-776d-bf87-6376cad74588";
  const id2 = "01a1c84e-e0c4-765e-83b0-7fd814478f2d";
  const ch = await fakeClickHouse(() => [200, `${JSON.stringify({ query_id: id1, query: "SELECT 1" })}\n${JSON.stringify({ query_id: id2, query: "SELECT 2" })}\n`]);
  try {
    const o = { chUrl: ch.url, chUser: "reader" };
    const found = await queryLog(o, "s3cret", [id1, id2], Date.now() + 5000);
    assert.equal(found.get(id1), "SELECT 1");
    assert.equal(found.get(id2), "SELECT 2");
    const req = ch.seen[0];
    assert.equal(req.headers["x-clickhouse-user"], "reader");
    assert.equal(req.headers["x-clickhouse-key"], "s3cret");
    assert.ok(!req.url.includes("s3cret"), "the password is not in the URL");
    assert.match(req.body, /FROM system\.query_log/);
    assert.ok(req.body.includes(`'${id1}'`) && req.body.includes(`'${id2}'`));
    assert.match(req.body, /^SELECT /);
  } finally {
    await ch.close();
  }
});

test("query log: a refusal names the user and the status, and not the password", async () => {
  const ch = await fakeClickHouse(() => [403, "Code: 497. Not enough privileges"]);
  try {
    await assert.rejects(checkQueryLog({ chUrl: ch.url, chUser: "reader" }, "s3cret"), (e) => /reader/.test(e.message) && /403/.test(e.message) && !/s3cret/.test(e.message));
  } finally {
    await ch.close();
  }
});

test("query log: ids that the log does not hold are missing from the result after the deadline", async () => {
  const ch = await fakeClickHouse(() => [200, ""]);
  try {
    const found = await queryLog({ chUrl: ch.url, chUser: "u" }, "p", ["01a1c84d-ac83-776d-bf87-6376cad74588"], Date.now() - 1);
    assert.equal(found.size, 0);
  } finally {
    await ch.close();
  }
});

test("Hune log: it reads sql_sha256 of the lines query.run", async () => {
  const dir = mkdtempSync(join(tmpdir(), "capture-test-"));
  try {
    const file = join(dir, "server.log");
    const line = (o) => JSON.stringify(o);
    writeFileSync(
      file,
      [
        line({ event: "query.run", query_id: "a", sql_sha256: "h1" }),
        line({ event: "http.request", query_id: "b", sql_sha256: "h2" }),
        "not json but query.run",
        line({ event: "query.run", query_id: "c", sql_sha256: "h3" }),
      ].join("\n") + "\n",
    );
    const found = await logHashes(file, ["a", "c"], Date.now() + 1000);
    assert.equal(found.get("a"), "h1");
    assert.equal(found.get("c"), "h3");
    assert.equal(found.has("b"), false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
