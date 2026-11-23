#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
// Capture tool for the visual parity of a dashboard (15.14c): the canvas calls, the display names and the boxes.
//
// What it does for one Grafana dashboard file and the same dashboard in Hune:
//   1. It puts the Grafana file in the folder `hune-parity` of Grafana under the uid `hpar-<uid>`. It makes the folder when it is
//      not there. At the end it deletes the dashboard, and the folder when the tool made it. Nothing else of Grafana changes.
//   2. It opens both dashboards in headless Chrome with the same absolute time range, in UTC, and it scrolls both pages so that
//      each panel draws. The window of Hune is as wide as it must be for its first panel to have the width of the first panel of
//      Grafana: then the canvases have the same size and the marks compare in the same pixels.
//   3. It records the canvas calls of each panel (the hook `canvas-hook.js`), the box and the title of each panel and each copy of
//      a repeat, the display names of the series and the state of the panel.
//   4. It compares both sides with the tolerances of 15.14c (visual-lib.mjs) and it gives each difference a class: a difference
//      that a section of 19.20 lists (visual-rules.mjs), or a fault.
//   5. It writes `report.json` and `report.md` into the output directory.
//
// The tool sends only read requests to Hune. It writes only the one dashboard (and the folder) in the disposable Grafana of
// the parity harness. The data of the fixtures are synthetic.
//
// Use:
//   node tools/parity/visual.mjs --out DIR --fixture FILE --grafana-url URL --hune-url URL
//       [--hune-uid UID] [--from TIME --to TIME] [--grafana-undock-menu] [--timeout-s N] [--replay DIR]
//       [--grafana-datasource-map FROM=TO] [--grafana-quote-placeholders] [--print-config]
//   --fixture      the Grafana dashboard file (the output of to_grafana.py or a file of tests/fixtures/grafana)
//   --hune-uid     the uid in Hune (default: the uid of the file with each underscore changed to a hyphen)
//   --from, --to   milliseconds or ISO times (default: the time of the file)
//   --grafana-datasource-map FROM=TO
//                  save the dashboard in Grafana with the data source uid TO where the file has FROM (a source that the disposable
//                  Grafana does not have)
//   --grafana-quote-placeholders
//                  write each `${name}` of a SQL text as `${name:singlequote}` in Grafana. Hune quotes a value in a code context, and
//                  Grafana gives the raw text
//   --replay       compare the captures that an earlier run saved in DIR (capture.json), with no browser and no Grafana write
//
// Exit codes: 0 no fault; 1 at least one fault or one panel not captured; 2 usage error; 3 the capture failed.

import { chmodSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Browser, sleep } from "../../tests/browser/lib/cdp.mjs";
import { classifyDifference, offsetCuts } from "./visual-rules.mjs";
import {
  colourToleranceOf,
  compareBox,
  grafanaCopyQueries,
  huneCopyQueries,
  compareCopy,
  compareMarks,
  compareNames,
  compareTexts,
  pairCopies,
  parseGrafanaKey,
  parseHuneTestid,
  renderReport,
  linePlotOf,
  splitTexts,
  xScaleOf,
} from "./visual-lib.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const TOOL = "tools/parity/visual.mjs";
const FOLDER_TITLE = "hune-parity";
const HOOK = readFileSync(join(HERE, "canvas-hook.js"), "utf8");

function fail(code, msg) {
  console.error(`visual: ${msg}`);
  process.exit(code);
}

// --- Arguments ---------------------------------------------------------------------------------------------------------

const KEYS = {
  "--out": "out",
  "--fixture": "fixture",
  "--grafana-url": "grafanaUrl",
  "--hune-url": "huneUrl",
  "--hune-uid": "huneUid",
  "--from": "from",
  "--to": "to",
  "--timeout-s": "timeoutS",
  "--replay": "replay",
  "--grafana-datasource-map": "dsMap",
};

function timeOf(text) {
  if (/^\d{10,13}$/.test(text)) return Number(text);
  const t = Date.parse(text);
  return Number.isNaN(t) ? null : t;
}

export function parseArgs(argv) {
  const o = { undockMenu: false, printConfig: false, quotePlaceholders: false };
  // --replay DIR compares the captures that an earlier run saved in DIR/capture.json. It opens no browser and writes nothing to Grafana.
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) fail(2, `${a} needs a value`);
      return argv[++i];
    };
    if (a === "--grafana-undock-menu") o.undockMenu = true;
    else if (a === "--grafana-quote-placeholders") o.quotePlaceholders = true;
    else if (a === "--print-config") o.printConfig = true;
    else if (KEYS[a]) {
      if (o[KEYS[a]] !== undefined) fail(2, `${a} is given two times`);
      o[KEYS[a]] = next();
    } else fail(2, `unknown argument ${a}`);
  }
  for (const k of ["out", "fixture", "grafanaUrl", "huneUrl"]) {
    if (o[k] === undefined) fail(2, `the option --${k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())} is necessary`);
  }
  for (const k of ["grafanaUrl", "huneUrl"]) {
    if (!/^https?:\/\/[^\s]+$/.test(o[k])) fail(2, `--${k} needs an http or https URL, not "${o[k]}"`);
    o[k] = o[k].replace(/\/+$/, "");
  }
  o.timeoutS = o.timeoutS === undefined ? 120 : Number(o.timeoutS);
  if (!Number.isInteger(o.timeoutS) || o.timeoutS < 10 || o.timeoutS > 3600) fail(2, "--timeout-s is a whole number from 10 to 3600");
  o.fixture = resolve(o.fixture);
  if (!existsSync(o.fixture)) fail(2, `--fixture: ${o.fixture} does not exist`);
  try {
    o.dashboard = JSON.parse(readFileSync(o.fixture, "utf8"));
  } catch (e) {
    fail(2, `--fixture: ${o.fixture} is not JSON: ${e.message}`);
  }
  if (typeof o.dashboard.uid !== "string" || !/^[A-Za-z0-9_-]{1,40}$/.test(o.dashboard.uid)) fail(2, "--fixture: the member uid is a text of 1 to 40 letters, digits, _ and -");
  o.huneUid = o.huneUid ?? o.dashboard.uid.replace(/_/g, "-");
  if (o.dsMap !== undefined) {
    const i = o.dsMap.indexOf("=");
    if (i < 1 || i === o.dsMap.length - 1) fail(2, "--grafana-datasource-map needs the form FROM=TO");
    o.dsMap = [o.dsMap.slice(0, i), o.dsMap.slice(i + 1)];
  }
  const from = o.from ?? o.dashboard.time?.from;
  const to = o.to ?? o.dashboard.time?.to;
  o.fromMs = timeOf(String(from));
  o.toMs = timeOf(String(to));
  if (o.fromMs === null || o.toMs === null) fail(2, `the time range needs absolute times: from "${from}", to "${to}"`);
  if (o.fromMs >= o.toMs) fail(2, "--from must be before --to");
  o.out = resolve(o.out);
  return o;
}

// --- The Grafana dashboard ---------------------------------------------------------------------------------------------

const grafanaUid = (o) => `hpar-${o.dashboard.uid}`.slice(0, 40);

async function gapi(o, method, path, body) {
  const res = await fetch(o.grafanaUrl + path, { method, headers: body ? { "Content-Type": "application/json" } : {}, body: body ? JSON.stringify(body) : undefined });
  const text = await res.text();
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {
    json = null;
  }
  return { status: res.status, json, text };
}

/** The dashboard as Grafana gets it: another uid, and the two rewrites of the options `--grafana-datasource-map` and `--grafana-quote-placeholders`. */
export function grafanaVersion(o) {
  const d = JSON.parse(JSON.stringify(o.dashboard));
  d.uid = grafanaUid(o);
  const walk = (x) => {
    if (Array.isArray(x)) return x.forEach(walk);
    if (!x || typeof x !== "object") return;
    if (o.dsMap && x.uid === o.dsMap[0] && typeof x.type === "string") x.uid = o.dsMap[1];
    if (o.quotePlaceholders && typeof x.rawSql === "string") x.rawSql = x.rawSql.replace(/\$\{([A-Za-z0-9_]+)\}/g, "${$1:singlequote}");
    Object.values(x).forEach(walk);
  };
  walk(d);
  return d;
}

/** Put the dashboard in the folder `hune-parity`. It returns what to remove at the end. */
async function putDashboard(o) {
  const made = { dashboard: grafanaUid(o), folder: null, folderMade: false };
  const folders = await gapi(o, "GET", "/api/folders");
  if (folders.status !== 200) throw new Error(`Grafana: GET /api/folders gave ${folders.status}: ${folders.text.slice(0, 200)}`);
  let folder = folders.json.find((f) => f.title === FOLDER_TITLE);
  if (!folder) {
    const r = await gapi(o, "POST", "/api/folders", { title: FOLDER_TITLE });
    if (r.status !== 200) throw new Error(`Grafana: the folder ${FOLDER_TITLE} was not made (${r.status}): ${r.text.slice(0, 200)}`);
    folder = r.json;
    made.folderMade = true;
  }
  made.folder = folder.uid;
  const d = grafanaVersion(o);
  delete d.id;
  const r = await gapi(o, "POST", "/api/dashboards/db", { dashboard: d, folderUid: folder.uid, overwrite: true });
  if (r.status !== 200) throw new Error(`Grafana: the dashboard ${made.dashboard} was not saved (${r.status}): ${r.text.slice(0, 300)}`);
  return made;
}

async function removeDashboard(o, made) {
  if (!made) return;
  await gapi(o, "DELETE", `/api/dashboards/uid/${encodeURIComponent(made.dashboard)}`).catch(() => {});
  if (!made.folderMade) return;
  // Another run can use the folder at the same time: a folder that still holds a dashboard stays, because its removal removes the dashboard too.
  const left = await gapi(o, "GET", `/api/search?type=dash-db&folderUIDs=${encodeURIComponent(made.folder)}`).catch(() => null);
  if (left && left.status === 200 && Array.isArray(left.json) && left.json.length === 0) await gapi(o, "DELETE", `/api/folders/${encodeURIComponent(made.folder)}`).catch(() => {});
  else made.folderMade = false;
}

// --- The facts of the Grafana file ---------------------------------------------------------------------------------------

function flatten(panels, out = []) {
  for (const p of panels ?? []) {
    if (p.type === "row") flatten(p.panels, out);
    else out.push(p);
  }
  return out;
}

/** The panels of the file by id: type, title, visible queries, colour scheme, repeat. */
export function fileFacts(dashboard) {
  const out = new Map();
  for (const p of flatten(dashboard.panels)) {
    out.set(p.id, {
      id: p.id,
      type: p.type,
      title: p.title ?? null,
      queries: (p.targets ?? []).filter((t) => !t.hide).length,
      scheme: p.options?.color?.scheme ?? null,
      options: p.options ?? {},
      custom: p.fieldConfig?.defaults?.custom ?? {},
      repeat: p.repeat ?? null,
      // Every y axis is hidden: the defaults hide the axis and no override shows one.
      hiddenAxes: p.fieldConfig?.defaults?.custom?.axisPlacement === "hidden" && !(p.fieldConfig?.overrides ?? []).some((ov) => (ov.properties ?? []).some((x) => x.id === "custom.axisPlacement" && x.value !== "hidden")),
    });
  }
  const rowRepeat = (dashboard.panels ?? []).some((p) => p.type === "row" && p.repeat);
  return { panels: out, rowRepeat, variables: dashboard.templating?.list ?? [] };
}

// --- The pages ---------------------------------------------------------------------------------------------------------

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

/** The requests that the settle counts: the queries of the panels and of the variables of both products. */
const isQuery = (r) => r.method === "POST" && /\/api\/ds\/query(\?|$)|\/api\/dashboards\/[^/]+\/panels\/[^/]+\/query$|\/api\/dashboards\/[^/]+\/variables\/[^/]+\/options$/.test(r.url);

/** Wait until no query is open and no new one came for `quietMs`. */
async function settle(page, deadline, quietMs = 600) {
  let last = -1;
  let since = Date.now();
  for (;;) {
    const queries = page.requests.filter(isQuery);
    const open = queries.filter((r) => r.status === null && !r.failed).length;
    if (open !== 0 || queries.length !== last) {
      last = queries.length;
      since = Date.now();
    } else if (Date.now() - since >= quietMs) return;
    if (Date.now() > deadline) throw new Error("timeout while it waited for the page to settle");
    await sleep(50);
  }
}

const GRAFANA_PANELS = `[...document.querySelectorAll('[data-viz-panel-key]')]`;
const HUNE_PANELS = `[...document.querySelectorAll('[data-testid^="panel-"]')].filter((e) => /^panel-\\d+(-r\\d+)?$/.test(e.dataset.testid))`;

/** The script that reads each panel of the page: the key, the box, the title, the names, the state and the canvas records. */
const READ = (side) => `(() => {
  const scroller = (() => {
    const list = [document.scrollingElement, ...document.querySelectorAll('*')].filter((e) => e &&
      e.scrollHeight > e.clientHeight + 50 && (e === document.scrollingElement || ['auto', 'scroll'].includes(getComputedStyle(e).overflowY)));
    list.sort((a, b) => (b.scrollHeight - b.clientHeight) - (a.scrollHeight - a.clientHeight));
    return list[0] || null;
  })();
  const top0 = scroller ? scroller.scrollTop : 0;
  const text = (e) => (e ? e.textContent.trim() : null);
  return ${side === "grafana" ? GRAFANA_PANELS : HUNE_PANELS}.map((e) => {
    const b = e.getBoundingClientRect();
    const key = ${side === "grafana" ? `e.getAttribute('data-viz-panel-key')` : `e.dataset.testid`};
    let names = [];
    let state = null;
    if (${side === "grafana"}) {
      const legend = [...e.querySelectorAll('button[class*="LegendLabel"]')].map(text);
      const heads = [...e.querySelectorAll('[role="columnheader"]')].map((x) => x.textContent.trim());
      const select = e.querySelector('input[role="combobox"]');
      const err = e.querySelector('[data-testid="data-testid Panel data error message"]');
      const badge = e.querySelector('[data-testid="data-testid Panel status error"]');
      const lastLine = e.innerText.split('\\n').map((x) => x.trim()).filter((x) => x !== '').pop() || null;
      names = { legend, heads, select: select ? lastLine : null };
      state = { error: err ? err.textContent.trim() : null, badge: !!badge, canvas: e.querySelectorAll('canvas').length };
    } else {
      const legend = [...e.querySelectorAll('.chartCore__legendItem span')].map(text);
      const heads = [...e.querySelectorAll('thead th')].map((x) => x.textContent.trim()).filter((x) => x !== '');
      const sel = e.querySelector('[data-testid$="-frames"]');
      names = { legend, heads, select: sel ? (sel.options ? sel.options[sel.selectedIndex].textContent.trim() : sel.textContent.trim()) : null };
      state = { state: e.dataset.state || null, badge: !!e.querySelector('[data-testid$="-status"]'), canvas: e.querySelectorAll('canvas').length };
    }
    const lines = e.innerText.split('\\n').map((x) => x.trim()).filter((x) => x !== '');
    const canvases = window.__cv ? window.__cv(e) : [];
    return {
      key,
      box: [b.left, b.top + top0, b.width, b.height].map((v) => Math.round(v * 100) / 100),
      title: text(e.querySelector(${side === "grafana" ? `'h2'` : `'.panel__title'`})),
      names, state, lines, canvases,
    };
  });
})()`;

/** The first panel with the largest `gridPos.w` that has no repeat (a copy has the width of the repeat, not of the grid): `{ id, w }` or null. */
export function referencePanel(dashboard) {
  let best = null;
  for (const p of flatten(dashboard.panels)) {
    if (p.repeat) continue;
    const w = p.gridPos?.w ?? 0;
    if (best === null || w > best.w) best = { id: p.id, w };
  }
  return best;
}

async function capturePage(browser, ctx, o, side, width) {
  const page = await ctx.newPage({ width, height: 1080 });
  await page.send("Emulation.setTimezoneOverride", { timezoneId: "UTC" });
  await page.send("Page.addScriptToEvaluateOnNewDocument", { source: HOOK });
  const deadline = Date.now() + o.timeoutS * 1000;
  let url;
  if (side === "grafana") {
    await page.send("Page.navigate", { url: o.grafanaUrl + "/login" });
    await sleep(500);
    if (o.undockMenu) await page.eval(`localStorage.setItem("grafana.navigation.docked", "false")`);
    const iso = (ms) => new Date(ms).toISOString();
    url = `${o.grafanaUrl}/d/${encodeURIComponent(grafanaUid(o))}/x?orgId=1&from=${iso(o.fromMs)}&to=${iso(o.toMs)}`;
  } else {
    url = `${o.huneUrl}/d/${encodeURIComponent(o.huneUid)}?from=${o.fromMs}&to=${o.toMs}`;
  }
  await page.send("Page.navigate", { url });
  await sleep(1500);
  await settle(page, deadline, 800);
  const sweep = async () => {
    await page.eval(SCROLL(0));
    await sleep(150);
    for (let n = 0; n < 400; n++) {
      await settle(page, deadline, 300);
      const s = await page.eval(SCROLL(400));
      await sleep(250);
      if (s.end) break;
    }
    await settle(page, deadline, 600);
  };
  await sweep();
  // A page that draws late: wait until the number of canvas calls stays the same.
  let last = "";
  for (let i = 0; i < 40; i++) {
    const now = await page.eval(`JSON.stringify((${side === "grafana" ? GRAFANA_PANELS : HUNE_PANELS}).map((e) => window.__cv(e).map((c) => c.ops.length)))`);
    if (now === last) break;
    last = now;
    await sleep(400);
  }
  await page.eval(SCROLL(0));
  await sleep(200);
  const panels = await page.eval(READ(side));
  const requests = page.requests
    .filter((r) => r.method === "POST" && (side === "grafana" ? /\/api\/ds\/query(\?|$)/.test(r.url) : /\/api\/dashboards\/[^/]+\/panels\/[^/]+\/query$/.test(r.url)))
    .map((r) => ({ url: r.url, status: r.status, headers: side === "grafana" ? Object.fromEntries(Object.entries(r.headers ?? {}).filter(([k]) => /^x-(panel|dashboard|plugin)/i.test(k))) : {}, body: r.body }));
  // The reference panel for the width of the window: the first panel with the largest width in the grid.
  const ref = referencePanel(o.dashboard);
  const refRecord = ref ? panels.find((p) => p.key === `panel-${ref.id}`) : null;
  const widthOfFirst = refRecord ? refRecord.box[2] : panels.length ? panels[0].box[2] : null;
  return { panels, requests, widthOfFirst, url };
}

// --- The comparison ----------------------------------------------------------------------------------------------------

const heavy = (type) => type === "heatmap" || type === "histogram";

/** The main canvas of a panel record: the one with the largest area. */
function mainCanvas(panel) {
  let best = null;
  for (const c of panel.canvases ?? []) if (best === null || c.size[0] * c.size[1] > best.size[0] * best.size[1]) best = c;
  return best;
}

/** The repeat of the dashboard: the variable and its values in the order of the options (the capture selects all of them). */
export function repeatOf(dashboard) {
  const names = new Set();
  for (const p of dashboard.panels ?? []) {
    if (p.repeat) names.add(p.repeat);
    for (const q of p.panels ?? []) if (q.repeat) names.add(q.repeat);
  }
  const name = [...names][0] ?? null;
  const v = (dashboard.templating?.list ?? []).find((x) => x.name === name);
  const values = v ? (v.options ?? []).filter((x) => x.value !== "$__all" && x.value !== "All").map((x) => String(x.value)) : [];
  return { name, values };
}

/**
 * The panels of a repeated section and the panels below it: `{ inSection, below, copies }`. In the document of Grafana a repeated row keeps its panels
 * after the row (or in `panels` when it is collapsed). `copies` is the number of values of the repeat (the capture selects all of them).
 */
export function sectionFacts(dashboard) {
  const inSection = new Set();
  const below = new Set();
  let state = "before";
  for (const p of dashboard.panels ?? []) {
    if (p.type === "row") {
      if (state === "in") state = "after";
      if (p.repeat && state === "before") state = "in";
      for (const q of p.panels ?? []) (state === "in" ? inSection : state === "after" ? below : new Set()).add(q.id);
    } else if (state === "in") inSection.add(p.id);
    else if (state === "after") below.add(p.id);
  }
  return { inSection, below, copies: repeatOf(dashboard).values.length };
}

/** Put the number of queries of each copy into the panel records of both sides. */
export function attachQueries(o, facts, g, h) {
  const rep = repeatOf(o.dashboard);
  const repeated = new Set();
  for (const p of flatten(o.dashboard.panels)) if (p.repeat) repeated.add(p.id);
  // A panel of a repeated section repeats with the section: the panels of a row with `repeat`.
  for (const row of o.dashboard.panels ?? []) if (row.type === "row" && row.repeat) for (const p of row.panels ?? []) repeated.add(p.id);
  // In the document of Grafana a repeated row keeps its panels after the row, so a panel that follows a row with `repeat` is in it.
  let inRepeatedRow = false;
  for (const p of o.dashboard.panels ?? []) {
    if (p.type === "row") inRepeatedRow = !!p.repeat;
    else if (inRepeatedRow) repeated.add(p.id);
  }
  for (const side of [g, h]) {
    for (const panel of side.panels) {
      const parsed = side === g ? parseGrafanaKey(panel.key) : parseHuneTestid(panel.key);
      if (!parsed) continue;
      const f = facts.panels.get(parsed.id);
      const isRep = repeated.has(parsed.id) && rep.name;
      if (side === g) {
        if (!isRep) panel.queries = grafanaCopyQueries(g.requests, { id: parsed.id, plain: true }).reduce((a, b) => a + b, 0);
        else {
          const copies = grafanaCopyQueries(g.requests, { title: f?.title ?? panel.title });
          panel.queries = copies[parsed.n - 1] ?? 0;
        }
      } else {
        panel.queries = huneCopyQueries(h.requests, isRep ? { id: parsed.id, variable: rep.name, value: rep.values[parsed.n - 1] } : { id: parsed.id });
      }
    }
  }
}

export function compareCaptures(o, facts, g, h) {
  attachQueries(o, facts, g, h);
  const records = [];
  const fixture = o.dashboard.uid;
  const add = (panel, check, status, detail, ctx = {}) => {
    const r = { fixture, panel, check, status, detail };
    if (status === "different") {
      const siblings = records.filter((x) => x.panel === panel && x.status === "different");
      Object.assign(r, classifyDifference({ check, panel, detail, siblings, ...ctx }));
    }
    records.push(r);
  };
  const { pairs, onlyG, onlyH } = pairCopies(g.panels, h.panels);
  for (const x of onlyG) add(x.key, "pair", "different", `Grafana has the panel ${x.key} and Hune has no copy for it`, { side: "grafana" });
  for (const x of onlyH) add(x.key, "pair", "different", `Hune has the panel ${x.key} and Grafana has no copy for it`, { side: "hune" });
  const first = pairs[0];
  const origin = first ? { grafana: first.grafana.box, hune: first.hune.box } : null;
  const sections = facts.rowRepeat ? sectionFacts(o.dashboard) : null;
  for (const pair of pairs) {
    const label = `${pair.id}${pair.n > 1 ? `/copy ${pair.n}` : ""}`;
    const f = facts.panels.get(pair.id) ?? {};
    const ctx = { type: f.type, id: pair.id, n: pair.n, fixture, options: f.options ?? {}, custom: f.custom ?? {}, title: f.title ?? null, lines: { grafana: pair.grafana.lines, hune: pair.hune.lines } };
    if (sections) ctx.section = { inSection: sections.inSection.has(pair.id), below: sections.below.has(pair.id), copies: sections.copies };
    const copy = compareCopy(pair, { origin });
    // The signed move of the box relative to the first panel, Hune less Grafana: a positive y is a box of Hune lower on the page.
    const rel = (side, i) => pair[side].box[i] - (origin ? origin[side][i] : 0);
    const shift = [0, 1].map((i) => Math.round((rel("hune", i) - rel("grafana", i)) * 100) / 100);
    add(label, "box", copy.box_ok ? "equal" : "different", { delta: copy.box_delta, shift, grafana: pair.grafana.box, hune: pair.hune.box }, ctx);
    add(label, "title", copy.title.equal ? "equal" : "different", copy.title, ctx);
    add(label, "queries", copy.queries.equal ? "equal" : "different", copy.queries, ctx);
    // The state of the panel: a canvas, an error, or an empty result.
    const gs = pair.grafana.state ?? {};
    const hs = pair.hune.state ?? {};
    const gState = gs.error === "No data" ? "empty" : gs.error ? "error" : gs.canvas ? "canvas" : "other";
    const hState = ["error", "empty", "bucket-limit", "timeline-limit"].includes(hs.state) ? hs.state : hs.canvas ? "canvas" : "other";
    const sameState = gState === hState || (gState === "other" && hState === "empty");
    add(label, "state", sameState ? "equal" : "different", { grafana: gs.error ?? gState, hune: hs.state ?? hState }, ctx);
    if (heavy(f.type)) {
      const gc = mainCanvas(pair.grafana);
      const hc = mainCanvas(pair.hune);
      if (gc && hc) {
        const tol = colourToleranceOf(f.scheme);
        const gt = splitTexts(gc);
        const ht = splitTexts(hc);
        // A bucket offset that is not a multiple of the bucket size: the x ranges of the products differ (19.20g), so the bars pair by their x value.
        const byValue = f.type === "histogram" && offsetCuts(f.options) ? { g: xScaleOf(gt.x), h: xScaleOf(ht.x) } : null;
        const marks = compareMarks(gc, hc, { colourTolerance: tol, mode: f.type === "histogram" ? "bars" : "cells", byValue: byValue && byValue.g && byValue.h ? byValue : null });
        const ok = marks.paired === marks.grafana && marks.only_grafana_count === 0 && marks.only_hune_count === 0 && marks.box_ok;
        ctx.bars = { hune: marks.hune, grafana_no_x: marks.grafana_no_x };
        // The texts come first: a rule for the plot reads the rules of the texts of the same panel.
        const xt = compareTexts(gt.x, ht.x);
        const yt = compareTexts(gt.y, ht.y);
        add(label, "texts:x-axis", xt.equal ? "equal" : "different", xt, ctx);
        add(label, "texts:y-axis", yt.equal ? "equal" : "different", yt, ctx);
        const plotOk = Math.abs(marks.plot_shift) <= 2 && (marks.plot_width_delta ?? 0) <= 2 && (marks.plot_height_delta ?? 0) <= 4 && (marks.plot_top_delta ?? 0) <= 2;
        add(label, "plot", plotOk ? "equal" : "different", { shift: marks.plot_shift, width_delta: marks.plot_width_delta, height_delta: marks.plot_height_delta, top_delta: marks.plot_top_delta, grafana: marks.plot.grafana, hune: marks.plot.hune }, ctx);
        add(label, "marks", ok ? "equal" : "different", marks, ctx);
        if (marks.slivers) add(label, "marks:empty-buckets", "different", { hune_extra_bars: marks.slivers }, ctx);
        add(label, "colours", marks.colour_bad_count === 0 ? "equal" : "different", { worst: marks.colour_worst, bad: marks.colour_bad_count, examples: marks.colour_bad, tolerance: tol }, ctx);
        add(label, "canvas-size", compareBox([0, 0, ...gc.size], [0, 0, ...hc.size]).ok ? "equal" : "different", { grafana: gc.size, hune: hc.size }, ctx);
      }
    } else {
      // A time series whose y axes are all hidden: the top of the plot (E27, 9.24). The plot of each side comes from the clip of its lines.
      if (f.type === "timeseries" && f.hiddenAxes) {
        const gp = linePlotOf(mainCanvas(pair.grafana), { grafana: true });
        const hp = linePlotOf(mainCanvas(pair.hune));
        const top = gp && hp ? Math.round(Math.abs(gp[1] - hp[1]) * 100) / 100 : null;
        add(label, "plot:top", top !== null && top <= 2 ? "equal" : "different", { top_delta: top, grafana: gp, hune: hp }, ctx);
      }
      // The names of the series: the legend, the headers of a table and the select of the frames.
      const gn = pair.grafana.names ?? {};
      const hn = pair.hune.names ?? {};
      for (const part of f.type === "table" ? ["heads"] : ["legend"]) {
        if ((gn[part]?.length ?? 0) === 0 && (hn[part]?.length ?? 0) === 0) continue;
        const cmp = compareNames(gn[part] ?? [], hn[part] ?? []);
        add(label, `names:${part}`, cmp.equal ? "equal" : "different", cmp, ctx);
      }
      if (gn.select !== null || hn.select !== null) add(label, "names:select", gn.select === hn.select ? "equal" : "different", { grafana: gn.select, hune: hn.select }, ctx);
      if (f.type === "stat" || f.type === "bargauge" || f.type === "gauge") {
        // The body of a stat is a list of titles and values: compare the lines with no title and no menu line.
        // The lines are title, value, title, value. The tiles that fit in the box differ, so the titles compare as a list with no repeat.
        const body = (p) => [...new Set(p.lines.filter((l) => l !== p.title && !/^Panel menu/.test(l)).filter((_, i) => i % 2 === 0))];
        const cmp = compareNames(body(pair.grafana), body(pair.hune));
        add(label, "names:body", cmp.equal ? "equal" : "different", cmp, ctx);
      }
    }
  }
  return records;
}

/** The requests of a side by the panel: the number of queries each request holds. */
export function requestFacts(g, h) {
  const gq = g.requests.reduce((n, r) => n + (JSON.parse(r.body ?? "{}").queries?.length ?? 0), 0);
  return { grafana: { requests: g.requests.length, queries: gq }, hune: { requests: h.requests.length, queries: h.requests.length } };
}

// --- Main ----------------------------------------------------------------------------------------------------------------

function effective(o) {
  return {
    out: o.out,
    fixture: o.fixture,
    grafana: { url: o.grafanaUrl, uid: grafanaUid(o), folder: FOLDER_TITLE },
    hune: { url: o.huneUrl, uid: o.huneUid },
    from_ms: o.fromMs,
    to_ms: o.toMs,
    grafana_undock_menu: o.undockMenu,
    timeout_s: o.timeoutS,
  };
}

function writePrivate(path, text) {
  const tmp = `${path}.tmp`;
  writeFileSync(tmp, text, { mode: 0o600 });
  renameSync(tmp, path);
}

export async function run(o) {
  if (!existsSync(o.out)) {
    mkdirSync(o.out, { recursive: true, mode: 0o700 });
    chmodSync(o.out, 0o700);
  }
  const facts = fileFacts(o.dashboard);
  const profile = join(o.out, "chrome-profile");
  let made = null;
  let browser = null;
  let g;
  let h;
  const log = [];
  try {
    if (o.replay) {
      ({ g, h, made } = JSON.parse(readFileSync(join(resolve(o.replay), "capture.json"), "utf8")));
      o.huneWindow = h.window ?? null;
    } else {
    made = await putDashboard(o);
    browser = await Browser.launch({ profileDir: profile, width: 1920, height: 1080 });
    const ctx = await browser.newContext();
    g = await capturePage(browser, ctx, o, "grafana", 1920);
    // The window of Hune: the first panel must have the width of the first panel of Grafana.
    let width = 1920;
    // A panel of w columns has w / 24 of the width of the grid, so the window moves by 24 / w of the difference.
    const firstW = referencePanel(o.dashboard)?.w ?? 24;
    for (let i = 0; i < 5; i++) {
      h = await capturePage(browser, ctx, o, "hune", width);
      const diff = g.widthOfFirst !== null && h.widthOfFirst !== null ? Math.round(g.widthOfFirst - h.widthOfFirst) : 0;
      log.push(`hune window ${width} px: first panel ${h.widthOfFirst} px, Grafana ${g.widthOfFirst} px`);
      if (diff === 0) break;
      width += Math.round((diff * 24) / Math.max(1, firstW));
    }
    o.huneWindow = width;
    h.window = width;
    }
  } catch (e) {
    console.error(`visual: the capture failed: ${e.message}`);
    return 3;
  } finally {
    if (browser) await browser.close();
    rmSync(profile, { recursive: true, force: true });
    if (!o.replay) await removeDashboard(o, made);
  }
  if (!o.replay) writePrivate(join(o.out, "capture.json"), JSON.stringify({ g, h, made }));
  const records = compareCaptures(o, facts, g, h);
  const reqs = requestFacts(g, h);
  const meta = {
    tool: TOOL,
    fixture: o.fixture,
    grafana_dashboard: made.dashboard,
    hune_uid: o.huneUid,
    range: `${o.fromMs} to ${o.toMs}`,
    hune_window: o.huneWindow,
    requests: JSON.stringify(reqs),
  };
  const report = { ...meta, log, records, grafana_deleted: made.dashboard, folder_deleted: made.folderMade };
  writePrivate(join(o.out, "report.json"), JSON.stringify(report, null, 2) + "\n");
  writePrivate(join(o.out, "report.md"), renderReport(records, meta));
  writePrivate(join(o.out, "raw-grafana.json"), JSON.stringify({ panels: g.panels.map(({ canvases, ...p }) => ({ ...p, canvases: canvases.length })), requests: g.requests }, null, 1) + "\n");
  writePrivate(join(o.out, "raw-hune.json"), JSON.stringify({ panels: h.panels.map(({ canvases, ...p }) => ({ ...p, canvases: canvases.length })), requests: h.requests }, null, 1) + "\n");
  const faults = records.filter((r) => r.status === "different" && r.class === "fault");
  const documented = records.filter((r) => r.status === "different" && r.class === "documented");
  console.log(`${o.dashboard.uid}: ${records.length} checks, ${records.filter((r) => r.status === "equal").length} equal, ${documented.length} documented, ${faults.length} faults`);
  for (const f of faults.slice(0, 20)) console.log(`  FAULT ${f.panel} ${f.check}: ${JSON.stringify(f.detail).slice(0, 240)}`);
  return faults.length ? 1 : 0;
}

async function main() {
  const o = parseArgs(process.argv.slice(2));
  if (o.printConfig) {
    console.log(JSON.stringify(effective(o), null, 2));
    return 0;
  }
  return run(o);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().then(
    (code) => process.exit(code),
    (e) => {
      console.error(`visual: ${e.stack || e.message}`);
      process.exit(3);
    },
  );
}
