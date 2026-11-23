#!/usr/bin/env node
// Capture tool for the expansion vectors (16.13, case I15; 15.14a).
//
// What it does:
//   1. It writes one synthetic dashboard for each case of vector-cases.mjs into the provisioning folder
//      of the pinned Grafana.
//   2. It waits until Grafana has loaded each dashboard.
//   3. It opens each dashboard in headless Chrome. The Grafana front end interpolates the variables
//      and computes the interval. The ClickHouse plugin expands the macros in its backend.
//   4. It reads the request `POST /api/ds/query` of the panel and its answer. The text
//      `executedQueryString` of the answer is the expected SQL.
//   5. It writes one vector file for each case into tests/golden/expansion/.
//
// With --check, it writes no vector. It compares the capture with the committed files, and it
// exits with 1 on the first difference.
//
// The Grafana query of each dashboard reads only the synthetic table analytics.events.
// Use:  node tools/parity/vectors.mjs [--url URL] [--dashboards-dir DIR] [--out DIR] [--check]
//                                     [--only REGEX] [--concurrency N]

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Browser, sleep } from "../../tests/browser/lib/cdp.mjs";
import { GRAFANA_VERSION, PLUGIN, cases } from "./vector-cases.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const DATASOURCE = { type: "grafana-clickhouse-datasource", uid: "clickhouse" };
const PLUGIN_VERSION = PLUGIN.split(" ")[1];

function parseArgs(argv) {
  const o = {
    url: "http://127.0.0.1:3010",
    dashboardsDir: join(ROOT, "target/local/grafana/dashboards/vectors"),
    out: join(ROOT, "tests/golden/expansion"),
    differences: join(ROOT, "tools/parity/vector-differences.json"),
    check: false,
    only: null,
    concurrency: 4,
  };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) fail(`${a} needs a value`);
      return argv[++i];
    };
    if (a === "--url") o.url = next().replace(/\/$/, "");
    else if (a === "--dashboards-dir") o.dashboardsDir = resolve(next());
    else if (a === "--out") o.out = resolve(next());
    else if (a === "--differences") o.differences = resolve(next());
    else if (a === "--check") o.check = true;
    else if (a === "--only") o.only = new RegExp(next());
    else if (a === "--concurrency") o.concurrency = Number(next());
    else fail(`unknown argument ${a}`);
  }
  if (!Number.isInteger(o.concurrency) || o.concurrency < 1 || o.concurrency > 16) fail("--concurrency must be 1 to 16");
  return o;
}

function fail(msg) {
  console.error(`vectors: ${msg}`);
  process.exit(2);
}

/** Convert a Grafana duration such as `1m` to milliseconds. */
export function durationMs(text) {
  const m = /^(\d+)(ms|s|m|h|d|w)$/.exec(text);
  if (!m) throw new Error(`bad duration ${text}`);
  const unit = { ms: 1, s: 1000, m: 60000, h: 3600000, d: 86400000, w: 604800000 }[m[2]];
  return Number(m[1]) * unit;
}

/** A ClickHouse string literal for the synthetic options of a query variable. */
function chLiteral(s) {
  if (s.includes("$")) throw new Error(`option with $ is not allowed: ${s}`);
  return `'${s.replaceAll("\\", "\\\\").replaceAll("'", "\\'")}'`;
}

function current(def, sel) {
  const multi = def.multi;
  if (sel.all) return { text: multi ? ["All"] : "All", value: multi ? ["$__all"] : "$__all" };
  if (multi) return { text: sel.values, value: sel.values };
  if (sel.values.length === 0) return { text: "", value: "" };
  return { text: sel.values[0], value: sel.values[0] };
}

function variable(def, sel) {
  if (!sel) throw new Error(`no selection for variable ${def.name}`);
  const base = { name: def.name, label: def.name, hide: 0, skipUrlSync: false };
  if (def.kind === "text") {
    const value = sel.values[0] ?? "";
    return {
      ...base,
      type: "textbox",
      query: value,
      current: { text: value, value },
      options: [{ selected: true, text: value, value }],
    };
  }
  const cur = current(def, sel);
  const common = { ...base, multi: def.multi, includeAll: def.include_all, current: cur };
  if (def.all_value !== null && def.all_value !== undefined) common.allValue = def.all_value;
  if (def.kind === "custom") {
    for (const o of def.options) if (o.includes(",")) throw new Error(`comma in custom option ${o}`);
    const picked = new Set(sel.all ? [] : sel.values);
    return {
      ...common,
      type: "custom",
      query: def.options.join(","),
      options: def.options.map((o) => ({ selected: picked.has(o), text: o, value: o })),
    };
  }
  const rawSql = def.options.length
    ? `SELECT arrayJoin([${def.options.map(chLiteral).join(", ")}]) AS v`
    : "SELECT 'x' AS v WHERE 0";
  return {
    ...common,
    type: "query",
    datasource: DATASOURCE,
    definition: rawSql,
    query: { refId: "variable", editorType: "sql", queryType: "table", format: 1, pluginVersion: PLUGIN_VERSION, rawSql },
    refresh: 1,
    sort: 0,
    regex: "",
    options: [],
  };
}

/** The uid is a hash of the id: a uid has 40 characters at most. */
function uidOf(id) {
  return "vec-" + createHash("sha256").update(id).digest("hex").slice(0, 20);
}

function dashboardOf(c) {
  const d = {
    uid: uidOf(c.id),
    title: `vector ${c.id}`,
    tags: ["vectors"],
    schemaVersion: 41,
    version: 1,
    editable: false,
    graphTooltip: 0,
    refresh: "",
    timezone: "utc",
    time: { from: new Date(c.from_ms).toISOString(), to: new Date(c.to_ms).toISOString() },
    templating: { list: c.variables.map((def) => variable(def, c.values[def.name])) },
    panels: [
      {
        id: 1,
        type: "table",
        title: c.id,
        gridPos: { x: 0, y: 0, w: 24, h: 8 },
        datasource: DATASOURCE,
        maxDataPoints: c.max_data_points,
        ...(c.min_interval ? { interval: c.min_interval } : {}),
        targets: [
          {
            refId: "A",
            datasource: DATASOURCE,
            editorType: "sql",
            format: 1,
            queryType: "table",
            pluginVersion: PLUGIN_VERSION,
            rawSql: c.template,
          },
        ],
      },
    ],
  };
  // A hash of the content lets the tool see that Grafana loaded this exact version.
  d.description = createHash("sha256").update(JSON.stringify(d)).digest("hex").slice(0, 16);
  return d;
}

async function getJson(url) {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.json();
}

async function checkVersions(base) {
  const health = await getJson(`${base}/api/health`);
  if (health.version !== GRAFANA_VERSION) fail(`Grafana ${health.version} runs, but the vectors need ${GRAFANA_VERSION}`);
  const p = await getJson(`${base}/api/plugins/grafana-clickhouse-datasource/settings`);
  if (p.info.version !== PLUGIN_VERSION) fail(`plugin ${p.info.version} runs, but the vectors need ${PLUGIN_VERSION}`);
}

function writeDashboards(dir, selected) {
  mkdirSync(dir, { recursive: true });
  const keep = new Set(cases.map((c) => `${c.id}.json`));
  for (const f of readdirSync(dir)) {
    if (f.endsWith(".json") && !keep.has(f)) rmSync(join(dir, f)); // a file of an old case
  }
  const expected = new Map();
  for (const c of selected) {
    const d = dashboardOf(c);
    const file = join(dir, `${c.id}.json`);
    const text = JSON.stringify(d, null, 1) + "\n";
    if (!existsSync(file) || readFileSync(file, "utf8") !== text) writeFileSync(file, text);
    expected.set(d.uid, d.description);
  }
  return expected;
}

async function waitProvisioned(base, expected) {
  const end = Date.now() + 180000;
  let missing = [...expected.keys()];
  while (missing.length) {
    const still = [];
    for (const uid of missing) {
      try {
        const r = await getJson(`${base}/api/dashboards/uid/${uid}`);
        if (r.dashboard.description !== expected.get(uid)) still.push(uid);
      } catch {
        still.push(uid);
      }
    }
    missing = still;
    if (!missing.length) break;
    if (Date.now() > end) fail(`Grafana did not load ${missing.length} dashboards (first uid ${missing[0]}). Does it mount the dashboards folder?`);
    await sleep(2000);
  }
}

/** Open one dashboard and return the panel query and its answer. */
async function capture(page, base, c) {
  page.requests.length = 0;
  page.byId.clear();
  const uid = uidOf(c.id);
  await page.send("Page.navigate", { url: `${base}/d/${uid}/x?orgId=1&kiosk` });
  const panelRequests = () =>
    page.requests.filter((r) => r.url.includes("/api/ds/query") && (r.headers["x-panel-id"] || r.headers["X-Panel-Id"]));
  const end = Date.now() + 40000;
  let lastCount = -1;
  let settled = 0;
  for (;;) {
    const rs = panelRequests();
    const done = rs.length > 0 && rs.every((r) => r.status !== null || r.failed);
    if (done && rs.length === lastCount) {
      if (++settled >= 8) break; // 8 reads of 100 ms without a new request
    } else {
      settled = 0;
    }
    lastCount = rs.length;
    if (Date.now() > end) throw new Error(`${c.id}: no panel query in 40 s`);
    await sleep(100);
  }
  const r = panelRequests().at(-1);
  let body = null;
  for (let i = 0; i < 20 && body === null; i++) {
    try {
      body = (await page.send("Network.getResponseBody", { requestId: r.id })).body;
    } catch {
      await sleep(100);
    }
  }
  if (body === null) throw new Error(`${c.id}: no answer body`);
  const sent = JSON.parse(r.body);
  const answer = JSON.parse(body);
  const q = sent.queries[0];
  const res = answer.results?.A;
  const meta = res?.frames?.[0]?.schema?.meta;
  return {
    sent_sql: q.rawSql,
    interval_ms: q.intervalMs,
    max_data_points: q.maxDataPoints,
    from_ms: Number(sent.from),
    to_ms: Number(sent.to),
    executed: meta?.executedQueryString ?? null,
    error: res?.error ?? answer.message ?? null,
    queries: sent.queries.length,
  };
}

function vectorOf(c, cap) {
  if (cap.from_ms !== c.from_ms || cap.to_ms !== c.to_ms) throw new Error(`${c.id}: Grafana changed the range to ${cap.from_ms}..${cap.to_ms}`);
  if (cap.max_data_points !== c.max_data_points) throw new Error(`${c.id}: Grafana sent max_data_points ${cap.max_data_points}`);
  const v = {
    id: c.id,
    tags: c.tags,
    grafana: GRAFANA_VERSION,
    plugin: PLUGIN,
    template: c.template,
    variables: c.variables,
    values: c.values,
    from_ms: c.from_ms,
    to_ms: c.to_ms,
    max_data_points: c.max_data_points,
    min_interval: c.min_interval,
    min_interval_ms: c.min_interval ? durationMs(c.min_interval) : null,
    interval_ms: cap.interval_ms,
  };
  if (cap.executed !== null) {
    v.expected = cap.executed;
  } else {
    v.expected = null;
    v.grafana_error = cap.error;
  }
  // The text that the front end sent to the plugin. It shows the variable step apart from the macros.
  v.sent_sql = cap.sent_sql;
  return v;
}

async function main() {
  const o = parseArgs(process.argv);
  const selected = cases.filter((c) => !o.only || o.only.test(c.id));
  if (!selected.length) fail("no case matches --only");
  await checkVersions(o.url);
  const differences = existsSync(o.differences) ? JSON.parse(readFileSync(o.differences, "utf8")) : {};
  for (const id of Object.keys(differences)) {
    // The keys that start with "pg-" belong to the PostgreSQL cases of pg_vectors.py.
    if (id.startsWith("pg-")) continue;
    if (!cases.some((c) => c.id === id)) fail(`${o.differences}: unknown case ${id}`);
  }

  const expected = writeDashboards(o.dashboardsDir, selected);
  console.log(`${selected.length} dashboards written; waiting for Grafana to load them`);
  await waitProvisioned(o.url, expected);

  // One Chrome process for each worker: a background tab does not run its queries.
  const results = new Map();
  const queue = [...selected];
  const worker = async (n) => {
    const browser = await Browser.launch({
      profileDir: join(ROOT, `target/local/vectors-chrome-${n}`),
      width: 1920,
      height: 1080,
    });
    try {
      const page = await (await browser.newContext()).newPage({ width: 1920, height: 1080 });
      for (let c = queue.shift(); c; c = queue.shift()) {
        let cap;
        for (let attempt = 1; ; attempt++) {
          try {
            cap = await capture(page, o.url, c);
            break;
          } catch (e) {
            if (attempt >= 3) throw e;
            console.error(`retry ${c.id}: ${e.message}`);
          }
        }
        results.set(c.id, vectorOf(c, cap));
      }
    } finally {
      await browser.close();
    }
  };
  await Promise.all(Array.from({ length: o.concurrency }, (_, n) => worker(n)));

  let changed = 0;
  mkdirSync(o.out, { recursive: true });
  for (const c of selected) {
    const v = results.get(c.id);
    const file = join(o.out, `${c.id}.json`);
    const withDiff = differences[c.id] ? { ...v, difference: differences[c.id] } : v;
    const text = JSON.stringify(withDiff, null, 2) + "\n";
    if (o.check) {
      if (!existsSync(file)) {
        console.error(`missing vector ${file}`);
        changed++;
        continue;
      }
      const old = JSON.parse(readFileSync(file, "utf8"));
      delete old.difference;
      if (JSON.stringify(old) !== JSON.stringify(v)) {
        console.error(`vector ${c.id} differs from the capture`);
        console.error(`  committed: ${JSON.stringify(old.expected)}`);
        console.error(`  captured:  ${JSON.stringify(v.expected)}`);
        changed++;
      }
    } else if (!existsSync(file) || readFileSync(file, "utf8") !== text) {
      writeFileSync(file, text);
      changed++;
    }
  }
  if (!o.check && !o.only) {
    const keep = new Set(cases.map((c) => `${c.id}.json`));
    for (const f of readdirSync(o.out)) {
      if (f.endsWith(".json") && !keep.has(f)) rmSync(join(o.out, f)); // a vector of an old case
    }
  }
  const tags = new Map();
  for (const c of selected) for (const t of c.tags) tags.set(t, (tags.get(t) ?? 0) + 1);
  console.log([...tags].sort().map(([t, n]) => `${t}: ${n}`).join("\n"));
  console.log(`${selected.length} vectors, ${changed} ${o.check ? "different" : "written"}`);
  if (o.check && changed) process.exit(1);
}

main().catch((e) => {
  console.error(`vectors: ${e.stack || e.message}`);
  process.exit(1);
});
