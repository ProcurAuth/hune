#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
// Capture tool for the SQL parity of a dashboard (15.14a).
//
// What it does for one pair (Grafana uid, Hune uid) and one fixed time range:
//   1. It opens the dashboard in Grafana and in Hune in headless Chrome. Both get the same URL state: the absolute
//      `from` and `to`, and the `var-` values. The window is 1920 by 1080 pixels.
//   2. It scrolls each page and opens each collapsed row, because both tools query a panel only when it is in view.
//      It waits until no request is open.
//   3. It records each `POST /api/ds/query` of Grafana with its answer (the header `X-Panel-Id` names the panel),
//      and each panel or variable query of Hune with its answer (6.20, 6.21).
//   4. The frame of Hune holds no SQL. The tool gets the SQL that Hune executed in one of two ways, with the
//      `query_id` of the answer (8.10):
//      --hune-sql query_log   it reads the text from `system.query_log` of ClickHouse. The account of
//                              --ch-user needs SELECT on this table.
//      --hune-sql log-hash    it reads the field `sql_sha256` of the log lines `query.run` of Hune (14), from the
//                              file --hune-log. It has no text of Hune: it compares the hash of the SQL of
//                              Grafana, and the hash of this SQL with the interval of Hune (cause `interval`).
//   5. It normalises the SQL texts (15.14a), compares them with the interval, `maxDataPoints`, the range and the
//      data, and gives each difference one cause.
//   6. It writes `report.json` and `report.md`, and the raw requests and answers, into the output directory.
//
// The output directory holds confidential data of a real run (15.18). The tool refuses a directory inside a git
// checkout and creates it with mode 0700. It has no default path. The Chrome profile lives in this directory
// and goes at the end of the run.
//
// The tool sends only read requests: page loads, the queries of the two tools, and one SELECT on the ClickHouse
// system table `system.query_log`. It never saves a dashboard.
//
// Use:
//   node tools/parity/capture.mjs --out DIR
//       --grafana-url URL --grafana-uid UID --hune-url URL --hune-uid UID
//       (--from TIME --to TIME | --range R1|R2|R3)
//       --hune-sql query_log --ch-url URL --ch-user USER --ch-password-file FILE
//       --hune-sql log-hash --hune-log FILE
//       [--var NAME=VALUE ...] [--panel ID ...] [--no-expand] [--grafana-undock-menu] [--cookie-file FILE]
//       [--timeout-s N] [--print-config]
//
// Exit codes: 0 all queries equal; 1 at least one difference or one panel not captured; 2 usage error or refusal;
// 3 the capture failed (browser, network, ClickHouse).

import { chmodSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { Browser, sleep } from "../../tests/browser/lib/cdp.mjs";
import {
  comparePanelQuery,
  insideGitCheckout,
  pairByRef,
  namedRange,
  pairVariable,
  parseTime,
  parseVar,
  renderMarkdown,
  stateQuery,
  tableOfGrafana,
  tableOfHune,
  totals,
} from "./capture-lib.mjs";

const TOOL = "tools/parity/capture.mjs";
const WINDOW = { width: 1920, height: 1080 };

function fail(code, msg) {
  console.error(`capture: ${msg}`);
  process.exit(code);
}

// --- Arguments ----------------------------------------------------------------------------------------------------------

const KEYS = {
  "--out": "out",
  "--grafana-url": "grafanaUrl",
  "--grafana-uid": "grafanaUid",
  "--hune-url": "huneUrl",
  "--hune-uid": "huneUid",
  "--from": "from",
  "--to": "to",
  "--range": "range",
  "--hune-sql": "huneSql",
  "--hune-log": "huneLog",
  "--ch-url": "chUrl",
  "--ch-user": "chUser",
  "--ch-password-file": "chPasswordFile",
  "--cookie-file": "cookieFile",
  "--timeout-s": "timeoutS",
};

export function parseArgs(argv) {
  const o = { vars: [], panels: [], expand: true, undockMenu: false, printConfig: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) fail(2, `${a} needs a value`);
      return argv[++i];
    };
    if (a === "--var") {
      const v = parseVar(next());
      if (!v) fail(2, "--var needs the form NAME=VALUE");
      o.vars.push(v);
    } else if (a === "--panel") {
      const n = Number(next());
      if (!Number.isInteger(n) || n < 0) fail(2, "--panel needs a whole number");
      o.panels.push(n);
    } else if (a === "--no-expand") o.expand = false;
    else if (a === "--grafana-undock-menu") o.undockMenu = true;
    else if (a === "--print-config") o.printConfig = true;
    else if (KEYS[a]) {
      if (o[KEYS[a]] !== undefined) fail(2, `${a} is given two times`);
      o[KEYS[a]] = next();
    } else fail(2, `unknown argument ${a}`);
  }
  const need = (k) => {
    if (o[k] === undefined) fail(2, `the option --${k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())} is necessary`);
  };
  for (const k of ["out", "grafanaUrl", "grafanaUid", "huneUrl", "huneUid", "huneSql"]) need(k);
  if (o.huneSql === "query_log") {
    for (const k of ["chUrl", "chUser", "chPasswordFile"]) need(k);
    if (o.huneLog !== undefined) fail(2, "--hune-log needs --hune-sql log-hash");
  } else if (o.huneSql === "log-hash") {
    need("huneLog");
    for (const k of ["chUrl", "chUser", "chPasswordFile"]) {
      if (o[k] !== undefined) fail(2, `--${k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())} needs --hune-sql query_log`);
    }
  } else fail(2, `--hune-sql is query_log or log-hash, not "${o.huneSql}"`);
  if (o.range !== undefined) {
    if (o.from !== undefined || o.to !== undefined) fail(2, "--range excludes --from and --to");
    const r = namedRange(o.range);
    if (!r) fail(2, `--range is R1, R2 or R3, not "${o.range}"`);
    o.fromMs = r.from_ms;
    o.toMs = r.to_ms;
  } else {
    if (o.from === undefined || o.to === undefined) fail(2, "give --range, or --from and --to");
    o.fromMs = parseTime(o.from);
    o.toMs = parseTime(o.to);
    if (o.fromMs === null) fail(2, `--from is milliseconds or an ISO time that ends with Z, not "${o.from}"`);
    if (o.toMs === null) fail(2, `--to is milliseconds or an ISO time that ends with Z, not "${o.to}"`);
  }
  if (o.fromMs >= o.toMs) fail(2, "--from must be before --to");
  if (o.toMs > Date.now()) fail(2, "--to is in the future: the range must be fixed and in the past (15.14a)");
  o.timeoutS = o.timeoutS === undefined ? 120 : Number(o.timeoutS);
  if (!Number.isInteger(o.timeoutS) || o.timeoutS < 10 || o.timeoutS > 3600) fail(2, "--timeout-s is a whole number from 10 to 3600");
  for (const k of ["grafanaUrl", "huneUrl", "chUrl"]) {
    if (o[k] === undefined) continue;
    if (!/^https?:\/\/[^\s]+$/.test(o[k])) fail(2, `--${k} needs an http or https URL`);
    o[k] = o[k].replace(/\/+$/, "");
  }
  o.out = resolve(o.out);
  if (insideGitCheckout(o.out)) fail(2, `the output directory ${o.out} is inside a git checkout (15.18)`);
  return o;
}

function readSecretFile(path, what) {
  let text;
  try {
    text = readFileSync(path, "utf8");
  } catch (e) {
    fail(2, `${what}: cannot read ${path}: ${e.code ?? e.message}`);
  }
  const value = text.replace(/\n$/, "");
  if (value === "") fail(2, `${what}: ${path} is empty`);
  return value;
}

// --- Files --------------------------------------------------------------------------------------------------------------

function prepareOut(dir) {
  if (existsSync(dir)) {
    const mode = statSync(dir).mode & 0o777;
    if (mode & 0o077) fail(2, `the output directory ${dir} has mode ${mode.toString(8)}; it must be 0700`);
  } else {
    const parent = resolve(dir, "..");
    if (!existsSync(parent)) fail(2, `the parent of the output directory does not exist: ${parent}`);
    mkdirSync(dir, { mode: 0o700 });
    chmodSync(dir, 0o700);
  }
}

function writePrivate(path, text) {
  const tmp = `${path}.tmp`;
  writeFileSync(tmp, text, { mode: 0o600 });
  renameSync(tmp, path);
}

// --- Browser ------------------------------------------------------------------------------------------------------------

const lower = (headers) => Object.fromEntries(Object.entries(headers ?? {}).map(([k, v]) => [k.toLowerCase(), v]));

/** Each request that the side records. `pick(request)` gives an info object or null. */
function recorder(page, pick) {
  const seen = new Map();
  const sync = () => {
    for (const r of page.requests) {
      if (seen.has(r.id)) continue;
      const info = pick(r);
      if (info) seen.set(r.id, { r, info, answer: null });
    }
  };
  const pending = () => {
    sync();
    return [...seen.values()].filter((x) => x.r.status === null && !x.r.failed).length;
  };
  const fetchBodies = async () => {
    sync();
    for (const x of seen.values()) {
      if (x.answer !== null || x.r.status === null) continue;
      for (let i = 0; i < 20 && x.answer === null; i++) {
        try {
          x.answer = (await page.send("Network.getResponseBody", { requestId: x.r.id })).body;
        } catch {
          await sleep(100);
        }
      }
    }
  };
  return { seen, pending, fetchBodies, sync };
}

/** Wait until no recorded request is open and none was new for `quietMs`. */
async function settle(rec, deadline, quietMs = 400) {
  let last = -1;
  let since = Date.now();
  for (;;) {
    const n = rec.pending();
    const total = rec.seen.size;
    if (n !== 0 || total !== last) {
      last = total;
      since = Date.now();
    } else if (Date.now() - since >= quietMs) return;
    if (Date.now() > deadline) throw new Error("timeout while it waited for the queries to end");
    await sleep(50);
  }
}

// The scroll container is the element with the largest scroll range. It can be the document or an inner block.
const SCROLL = (step) => `(() => {
  const list = [document.scrollingElement, ...document.querySelectorAll('*')].filter((e) => e &&
    e.scrollHeight > e.clientHeight + 50 && (e === document.scrollingElement || ['auto', 'scroll'].includes(getComputedStyle(e).overflowY)));
  list.sort((a, b) => (b.scrollHeight - b.clientHeight) - (a.scrollHeight - a.clientHeight));
  const e = list[0];
  if (!e) return { top: 0, max: 0, end: true };
  const max = e.scrollHeight - e.clientHeight;
  e.scrollTop = ${step} === 0 ? 0 : Math.min(max, e.scrollTop + ${step});
  return { top: e.scrollTop, max, end: e.scrollTop >= max - 1 };
})()`;

const COLLAPSED = {
  grafana: `[...document.querySelectorAll('button[aria-label="Expand row"]')]`,
  hune: `[...document.querySelectorAll('button.section__toggle[aria-expanded="false"]')]`,
};

const WIDTH = {
  grafana: (id) => `(() => { const e = document.querySelector('[data-viz-panel-key="panel-${id}"]'); return e ? Math.round(e.getBoundingClientRect().width) : null; })()`,
  hune: (id) => `(() => { const e = document.querySelector('[data-testid="panel-${id}"]'); return e ? Math.round(e.getBoundingClientRect().width) : null; })()`,
};

/** Open the dashboard, scroll, expand the rows, and return the settled recorder. */
async function openAndScroll(page, rec, url, kind, o, wanted) {
  const deadline = Date.now() + o.timeoutS * 1000;
  await page.send("Page.navigate", { url });
  await sleep(1500);
  await settle(rec, deadline, 800);
  const widths = new Map();
  const measure = async () => {
    rec.sync();
    for (const x of rec.seen.values()) {
      const id = x.info.panel;
      if (id === null || widths.has(id)) continue;
      const w = await page.eval(WIDTH[kind](id)).catch(() => null);
      if (w !== null) widths.set(id, w);
    }
  };
  const sweep = async () => {
    await page.eval(SCROLL(0));
    await sleep(150);
    for (let n = 0; n < 400; n++) {
      await settle(rec, deadline, 300);
      await measure();
      const s = await page.eval(SCROLL(500));
      await sleep(200);
      if (s.end) break;
    }
    await settle(rec, deadline, 400);
    await measure();
  };
  await sweep();
  if (o.expand) {
    for (let round = 0; round < 10; round++) {
      const count = await page.eval(`${COLLAPSED[kind]}.length`);
      if (!count) break;
      await page.eval(`(() => { const b = ${COLLAPSED[kind]}[0]; b.scrollIntoView({block: "center"}); b.click(); })()`);
      await sleep(300);
      await settle(rec, deadline, 400);
      await sweep();
    }
  }
  // A panel that is wanted but not seen: scroll to its element once.
  for (const id of wanted) {
    const have = [...rec.seen.values()].some((x) => x.info.panel === id);
    if (have) continue;
    await page.eval(`(() => { const e = document.querySelector('[data-viz-panel-key="panel-${id}"], [data-testid="panel-${id}"]'); if (e) e.scrollIntoView({block: "center"}); })()`);
    await sleep(300);
    await settle(rec, deadline, 500);
    await measure();
  }
  await rec.fetchBodies();
  return widths;
}

// --- The Grafana side ---------------------------------------------------------------------------------------------------

function grafanaPick(uid) {
  return (r) => {
    if (r.method !== "POST" || !/\/api\/ds\/query(\?|$)/.test(r.url)) return null;
    const h = lower(r.headers);
    if (h["x-dashboard-uid"] !== uid) return null;
    return { panel: h["x-panel-id"] === undefined || h["x-panel-id"] === "" ? null : Number(h["x-panel-id"]) };
  };
}

/** Panels and variables from the Grafana recorder. The last answer of a panel wins. */
function readGrafana(rec) {
  const panels = new Map();
  const variableRequests = [];
  for (const x of [...rec.seen.values()].sort((a, b) => a.r.t - b.r.t)) {
    let sent;
    let answer = null;
    try {
      sent = JSON.parse(x.r.body ?? "null");
      if (x.answer !== null) answer = JSON.parse(x.answer);
    } catch {
      continue;
    }
    if (!sent?.queries) continue;
    const queries = sent.queries.map((q) => {
      const res = answer?.results?.[q.refId];
      const frames = res?.frames ?? [];
      return {
        ref_id: q.refId,
        raw_sql: q.rawSql ?? null,
        sql: frames[0]?.schema?.meta?.executedQueryString ?? null,
        interval_ms: q.intervalMs ?? null,
        max_data_points: q.maxDataPoints ?? null,
        format: q.format ?? null,
        from_ms: Number(sent.from),
        to_ms: Number(sent.to),
        error: res?.error ?? (x.r.status !== 200 ? answer?.message ?? `HTTP ${x.r.status}` : null),
        frames,
        table: tableOfGrafana(frames),
      };
    });
    if (x.info.panel === null) variableRequests.push({ queries, count: 1 });
    else {
      const prev = panels.get(x.info.panel);
      panels.set(x.info.panel, { id: x.info.panel, queries, requests: (prev?.requests ?? 0) + 1 });
    }
  }
  return { panels, variableRequests };
}

// --- The Hune side -----------------------------------------------------------------------------------------------------

function viguiePick(uid) {
  const re = new RegExp(`/api/dashboards/${uid.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/(?:panels/(\\d+)/query|variables/([^/]+)/options)$`);
  return (r) => {
    if (r.method !== "POST") return null;
    const m = re.exec(new URL(r.url).pathname);
    if (!m) return null;
    return m[1] !== undefined ? { panel: Number(m[1]), variable: null } : { panel: null, variable: decodeURIComponent(m[2]) };
  };
}

function readHune(rec) {
  const panels = new Map();
  const variables = new Map();
  for (const x of [...rec.seen.values()].sort((a, b) => a.r.t - b.r.t)) {
    let sent;
    let answer = null;
    try {
      sent = JSON.parse(x.r.body ?? "null");
      if (x.answer !== null) answer = JSON.parse(x.answer);
    } catch {
      continue;
    }
    const item = {
      query_id: answer?.query_id ?? null,
      interval_ms: answer?.interval_ms ?? null,
      max_data_points: sent?.max_data_points ?? null,
      from_ms: sent?.from_ms ?? null,
      to_ms: sent?.to_ms ?? null,
      error: x.r.status === 200 ? null : answer?.code ?? `HTTP ${x.r.status}`,
      table: x.r.status === 200 ? tableOfHune(answer) : null,
      frame: answer,
      sql: null,
    };
    if (x.info.panel !== null) {
      // A panel with several queries has one request for each visible query, with the member `ref` (6.20). A
      // request with no `ref` is the first query, with the ref `A`. The last answer of a ref wins.
      const before = panels.get(x.info.panel);
      const refs = before?.refs ?? new Map();
      const explicit = typeof sent?.ref === "string";
      refs.set(explicit ? sent.ref : "A", item);
      panels.set(x.info.panel, { id: x.info.panel, ...item, refs, explicit_ref: Boolean(before?.explicit_ref) || explicit });
    } else variables.set(x.info.variable, { name: x.info.variable, ...item });
  }
  return { panels, variables };
}

// --- ClickHouse: the SQL that Hune executed -----------------------------------------------------------------------------

/** Fail at the start, not after the capture, when the account cannot read the query log. */
export async function checkQueryLog(o, password) {
  const res = await fetch(`${o.chUrl}/`, {
    method: "POST",
    headers: { "X-ClickHouse-User": o.chUser, "X-ClickHouse-Key": password },
    body: "SELECT count() FROM system.query_log WHERE event_date = today() FORMAT TabSeparated",
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`ClickHouse refuses the read of system.query_log for the user ${o.chUser} (HTTP ${res.status}): ${text.slice(0, 200).replace(/\s+/g, " ")}`);
}

export async function queryLog(o, password, ids, deadline) {
  const found = new Map();
  const list = ids.map((i) => `'${i.replace(/[^0-9a-f-]/gi, "")}'`).join(",");
  const sql =
    `SELECT query_id, argMax(query, event_time_microseconds) AS query FROM system.query_log ` +
    `WHERE event_date >= today() - 1 AND query_id IN (${list}) AND type != 'QueryStart' GROUP BY query_id FORMAT JSONEachRow`;
  while (found.size < ids.length) {
    const res = await fetch(`${o.chUrl}/`, {
      method: "POST",
      headers: { "X-ClickHouse-User": o.chUser, "X-ClickHouse-Key": password },
      body: sql,
    });
    const text = await res.text();
    if (!res.ok) throw new Error(`ClickHouse answered HTTP ${res.status} to the query_log read: ${text.slice(0, 200).replace(/\s+/g, " ")}`);
    for (const line of text.split("\n")) {
      if (!line) continue;
      const row = JSON.parse(line);
      found.set(row.query_id, row.query);
    }
    if (found.size >= ids.length) break;
    if (Date.now() > deadline) break;
    await sleep(2000); // the log flushes about every 7.5 s
  }
  return found;
}

/** The `sql_sha256` of each `query.run` line of the Hune log, by `query_id`. It waits for the lines that are not yet written. */
export async function logHashes(file, ids, deadline) {
  const found = new Map();
  for (;;) {
    for (const line of readFileSync(file, "utf8").split("\n")) {
      if (!line.includes('"query.run"')) continue;
      let row;
      try {
        row = JSON.parse(line);
      } catch {
        continue;
      }
      if (row.event === "query.run" && row.query_id && row.sql_sha256) found.set(row.query_id, row.sql_sha256);
    }
    if (ids.every((i) => found.has(i)) || Date.now() > deadline) return found;
    await sleep(1000);
  }
}

// --- The dashboards of both tools ---------------------------------------------------------------------------------------

async function pageJson(page, url) {
  const r = await page.eval(`fetch(${JSON.stringify(url)}, { credentials: "same-origin" }).then(async (r) => ({ ok: r.ok, status: r.status, text: await r.text() }))`);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return JSON.parse(r.text);
}

function flattenGrafana(panels, out = []) {
  for (const p of panels ?? []) {
    if (p.type === "row") flattenGrafana(p.panels, out);
    else out.push(p);
  }
  return out;
}

function grafanaFacts(dash) {
  const panels = new Map();
  for (const p of flattenGrafana(dash.panels)) {
    const notes = [];
    for (const ov of p.fieldConfig?.overrides ?? []) {
      if ((ov.properties ?? []).some((x) => x.id === "custom.hideFrom" && x.value?.viz === true)) {
        notes.push("the override hideSeriesFrom of Grafana has no counterpart in Hune; it changes the display only, not the SQL or the data");
        break;
      }
    }
    panels.set(p.id, { id: p.id, title: p.title ?? null, type: p.type, queries: (p.targets ?? []).filter((t) => !t.hide).length, notes });
  }
  const variables = (dash.templating?.list ?? [])
    .filter((v) => v.type === "query")
    .map((v) => ({ name: v.name, sql: typeof v.query === "object" ? v.query?.rawSql ?? v.definition ?? "" : v.definition ?? v.query ?? "" }));
  return { panels, variables };
}

function huneFacts(doc) {
  const panels = new Map();
  for (const s of doc.sections ?? []) {
    for (const p of s.panels ?? []) panels.set(p.id, { id: p.id, title: p.title ?? null, type: p.type, hasQuery: !!p.data?.query });
  }
  return panels;
}

// --- Main ---------------------------------------------------------------------------------------------------------------

function effective(o) {
  return {
    out: o.out,
    grafana: { url: o.grafanaUrl, uid: o.grafanaUid },
    hune: { url: o.huneUrl, uid: o.huneUid },
    from_ms: o.fromMs,
    to_ms: o.toMs,
    vars: o.vars.map(([n, v]) => [n, v]),
    panels: o.panels,
    expand_collapsed: o.expand,
    grafana_undock_menu: o.undockMenu,
    hune_sql: o.huneSql,
    hune_log: o.huneLog ?? null,
    clickhouse: o.huneSql === "query_log" ? { url: o.chUrl, user: o.chUser, password: "file:" + o.chPasswordFile + " (hidden)" } : null,
    cookie_file: o.cookieFile ?? null,
    timeout_s: o.timeoutS,
    window: WINDOW,
  };
}

async function loadCookies(page, file) {
  const mode = statSync(file).mode & 0o777;
  if (mode & 0o077) fail(2, `the cookie file ${file} has mode ${mode.toString(8)}; it must be 0600`);
  const cookies = JSON.parse(readFileSync(file, "utf8"));
  await page.send("Network.setCookies", { cookies });
}

async function main() {
  const o = parseArgs(process.argv.slice(2));
  if (o.printConfig) {
    console.log(JSON.stringify(effective(o), null, 2));
    return 0;
  }
  let password = null;
  if (o.huneSql === "query_log") {
    password = readSecretFile(o.chPasswordFile, "--ch-password-file");
    try {
      await checkQueryLog(o, password);
    } catch (e) {
      console.error(`capture: ${e.message}`);
      return 3;
    }
  } else if (!existsSync(o.huneLog)) fail(2, `--hune-log: ${o.huneLog} does not exist`);
  prepareOut(o.out);
  mkdirSync(join(o.out, "raw"), { recursive: true, mode: 0o700 });
  const profile = join(o.out, "chrome-profile");
  const query = stateQuery(o.fromMs, o.toMs, o.vars);
  const gUrl = `${o.grafanaUrl}/d/${encodeURIComponent(o.grafanaUid)}/x?orgId=1&${stateQuery(o.fromMs, o.toMs, o.vars, true)}`;
  const vUrl = `${o.huneUrl}/d/${encodeURIComponent(o.huneUid)}?${query}`;
  const deadline = Date.now() + o.timeoutS * 1000;

  let browser;
  let gRead;
  let vRead;
  let gFacts;
  let vFacts;
  let gWidths;
  let vWidths;
  const versions = {};
  try {
    browser = await Browser.launch({ profileDir: profile, ...WINDOW });
    const ctx = await browser.newContext();
    for (const side of ["grafana", "hune"]) {
      const page = await ctx.newPage(WINDOW);
      await page.send("Emulation.setTimezoneOverride", { timezoneId: "UTC" });
      if (o.cookieFile) await loadCookies(page, o.cookieFile);
      const base = side === "grafana" ? o.grafanaUrl : o.huneUrl;
      await page.send("Page.navigate", { url: base + (side === "grafana" ? "/login" : "/") });
      await sleep(500);
      // The docked menu of Grafana 12.4 takes 250 px of the width. The option shows the width without it (7.4).
      if (side === "grafana" && o.undockMenu) await page.eval(`localStorage.setItem("grafana.navigation.docked", "false")`);
      const uid = side === "grafana" ? o.grafanaUid : o.huneUid;
      if (side === "grafana") {
        const dash = await pageJson(page, `${o.grafanaUrl}/api/dashboards/uid/${encodeURIComponent(uid)}`);
        gFacts = grafanaFacts(dash.dashboard);
        versions.grafana = (await pageJson(page, `${o.grafanaUrl}/api/health`).catch(() => ({}))).version ?? null;
        versions.clickhouse_plugin = (await pageJson(page, `${o.grafanaUrl}/api/plugins/grafana-clickhouse-datasource/settings`).catch(() => ({}))).info?.version ?? null;
      } else {
        const dash = await pageJson(page, `${o.huneUrl}/api/dashboards/${encodeURIComponent(uid)}`);
        vFacts = huneFacts(dash.document);
        versions.hune_dashboard_version = dash.version ?? null;
      }
      const wanted = [...(side === "grafana" ? gFacts.panels : vFacts).values()].filter((p) => (side === "grafana" ? p.queries > 0 : p.hasQuery)).map((p) => p.id);
      const rec = recorder(page, side === "grafana" ? grafanaPick(uid) : viguiePick(uid));
      const widths = await openAndScroll(page, rec, side === "grafana" ? gUrl : vUrl, side, o, wanted);
      if (side === "grafana") {
        gRead = readGrafana(rec);
        gWidths = widths;
      } else {
        vRead = readHune(rec);
        vWidths = widths;
      }
      console.log(`${side}: ${rec.seen.size} requests recorded`);
    }
  } catch (e) {
    console.error(`capture: the browser capture failed: ${e.message}`);
    return 3;
  } finally {
    if (browser) await browser.close();
    rmSync(profile, { recursive: true, force: true });
    if (o.cookieFile && resolve(o.cookieFile).startsWith(o.out + "/")) rmSync(o.cookieFile, { force: true });
  }

  // The SQL that Hune executed.
  const huneQueries = [...[...vRead.panels.values()].flatMap((p) => [...p.refs.values()]), ...vRead.variables.values()];
  const ids = [...new Set(huneQueries.map((x) => x.query_id).filter(Boolean))];
  let log;
  try {
    log = o.huneSql === "query_log" ? await queryLog(o, password, ids, Math.max(deadline, Date.now() + 40000)) : await logHashes(o.huneLog, ids, Date.now() + 20000);
  } catch (e) {
    console.error(`capture: ${e.message}`);
    return 3;
  }
  for (const x of [...vRead.panels.values(), ...huneQueries]) {
    if (o.huneSql === "query_log") x.sql = log.get(x.query_id) ?? null;
    else x.sql_hash = log.get(x.query_id) ?? null;
  }

  // Compare panel by panel.
  const values = o.vars.map(([, v]) => v);
  const ids2 = [...new Set([...gRead.panels.keys(), ...vRead.panels.keys()])].sort((a, b) => a - b);
  const records = [];
  const missing = [];
  for (const id of ids2) {
    if (o.panels.length && !o.panels.includes(id)) continue;
    const g = gRead.panels.get(id);
    const v = vRead.panels.get(id);
    const gf = gFacts.panels.get(id);
    const meta = { id, title: gf?.title ?? vFacts.get(id)?.title ?? null, type: gf?.type ?? vFacts.get(id)?.type ?? null, notes: [...(gf?.notes ?? [])] };
    if (!g || !v) {
      missing.push({ id, title: meta.title, reason: !g ? "Grafana sent no query for it" : "Hune sent no query for it" });
      continue;
    }
    meta.panel_width_px = { grafana: gWidths.get(id) ?? null, hune: vWidths.get(id) ?? null };
    // Hune sends one request for each visible query, with its ref (4.15a, 6.20). Grafana sends one request for
    // the panel. The queries pair by ref, and a record has the panel and the ref (15.14c, 19.20f).
    if (g.queries.length > 1) meta.notes.push("Grafana sent one request for the panel, and Hune sends one for each query (19.20f)");
    g.queries.forEach((gq, index) => {
      const vq = pairByRef(v, gq.ref_id, index);
      const rec = comparePanelQuery(gq, vq, values);
      const errors = [gq.error && `Grafana: ${gq.error}`, vq?.error && `Hune: ${vq.error}`].filter(Boolean);
      records.push({ ...meta, ref_id: gq.ref_id, errors, ...rec, notes: [...meta.notes, ...(vq ? [] : ["Hune has no query for this refId"])] });
    });
  }
  for (const p of gFacts.panels.values()) {
    if (p.queries > 0 && !gRead.panels.has(p.id) && !missing.some((m) => m.id === p.id) && (!o.panels.length || o.panels.includes(p.id))) {
      missing.push({ id: p.id, title: p.title, reason: "Grafana sent no query for it (the panel is not in view or has no query)" });
    }
  }

  // Variables.
  const variables = [];
  for (const gv of gRead.variableRequests) {
    for (const q of gv.queries) {
      const pair = pairVariable(gFacts.variables, q.raw_sql ?? "");
      const entry = { ref_id: q.ref_id, name: pair.name, matches: pair.matches };
      if (pair.name === null) {
        entry.sql = { status: "different", cause: "other", detail: { reason: `the request pairs with ${pair.matches} variables` } };
      } else {
        const vv = vRead.variables.get(pair.name);
        entry.sql = vv
          ? comparePanelQuery({ sql: q.sql, interval_ms: null, max_data_points: null, from_ms: 0, to_ms: 0 }, { sql: vv.sql, sql_hash: vv.sql_hash, interval_ms: null, max_data_points: null, from_ms: 0, to_ms: 0 }, values).sql
          : { status: "different", cause: "other", detail: { reason: "Hune sent no options query for this variable" } };
      }
      variables.push(entry);
    }
  }

  const report = {
    tool: TOOL,
    created_at: new Date().toISOString(),
    run: {
      grafana_uid: o.grafanaUid,
      hune_uid: o.huneUid,
      from_ms: o.fromMs,
      to_ms: o.toMs,
      vars: o.vars.map(([n, v]) => `${n}=${v}`).join(" ") || "none",
      window: `${WINDOW.width}x${WINDOW.height}`,
      grafana_menu: o.undockMenu ? "undocked" : "docked (browser default)",
      grafana_version: versions.grafana,
      clickhouse_plugin_version: versions.clickhouse_plugin,
      hune_dashboard_version: versions.hune_dashboard_version,
      hune_sql_source: o.huneSql === "query_log" ? "system.query_log (text)" : "log lines query.run (sha256 only)",
    },
    totals: totals(records),
    panels: records,
    variables,
    missing,
  };
  writePrivate(join(o.out, "report.json"), JSON.stringify(report, null, 2) + "\n");
  writePrivate(join(o.out, "report.md"), renderMarkdown(report));
  writePrivate(
    join(o.out, "raw", "grafana.json"),
    JSON.stringify({ panels: [...gRead.panels.values()].map((p) => ({ ...p, queries: p.queries.map(({ table, ...q }) => q) })), variables: gRead.variableRequests.map((v) => v.queries.map(({ table, ...q }) => q)) }, null, 1) + "\n",
  );
  writePrivate(
    join(o.out, "raw", "hune.json"),
    JSON.stringify({ panels: [...vRead.panels.values()].map(({ table, ...p }) => p), variables: [...vRead.variables.values()].map(({ table, ...p }) => p) }, null, 1) + "\n",
  );

  const t = report.totals;
  console.log(`${t.queries} queries compared: ${t.equal} equal, ${t.different} different, ${missing.length} not captured`);
  console.log(`SQL: ${t.sql.equal} equal, ${t.sql.different} different`);
  console.log(`causes: ${Object.entries(t.causes).map(([c, n]) => `${c}=${n}`).join(" ")}`);
  console.log(`report: ${join(o.out, "report.md")}`);
  const varDiff = variables.some((x) => x.sql.status !== "equal");
  return t.different || missing.length || varDiff ? 1 : 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().then(
    (code) => process.exit(code),
    (e) => {
      console.error(`capture: ${e.stack || e.message}`);
      process.exit(3);
    },
  );
}
