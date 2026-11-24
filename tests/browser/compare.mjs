#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
// Side-by-side harness: the dashboard `stream` in Grafana (left) and in Hune (right).
//
// Use:  node tests/browser/compare.mjs
//       node tests/browser/compare.mjs --only dashboard-top,table --themes dark --widths 1280
//       node tests/browser/compare.mjs --list
//
// The harness builds and starts nothing. It needs:
//   - the disposable Grafana of target/local/grafana (start.sh), on http://127.0.0.1:3010;
//   - a Hune server on http://127.0.0.1:8088 that holds the dashboard `stream`.
// A side that is not there gets a placeholder in the sheet; the other side is still captured.
//
// Output: target/local/compare/<state>-<theme>-<width>.png (Grafana left, Hune right),
// target/local/compare/parts/<state>-<theme>-<width>-{grafana,hune}.png (the two captures)
// and target/local/compare/index.html (relative links, opens from file://).
//
// Options:
//   --only <a,b>       state ids to capture (the character * is a wildcard)
//   --themes <list>    dark,light (default: dark,light)
//   --widths <list>    1920,1280,768,390 (default: 1920,1280; the heights are 1080, 800, 1024 and 844)
//                      The mobile widths 768 and 390 run every state, and the states of the group "mobile".
//   --grafana <url>    default http://127.0.0.1:3010
//   --hune <url>      default http://127.0.0.1:8088
//   --grafana-uid <u>  dashboard uid in Grafana (default stream-example)
//   --grafana-slug <s> dashboard slug in Grafana (default stream)
//   --hune-uid <u>    dashboard uid in Hune (default stream)
//   --from <t> --to <t>  time range of both sides for the states of the dashboard stream (default now-6h, now)
//   --demo-from <t> --demo-to <t>  time range of both sides for the states of the demo dashboards (demo, density-20,
//                      service-overview, edge-states, types-tour, tables-and-cells, variables-lab). The default is an
//                      absolute window of 6 hours, inside the data of the synthetic tables: the table analytics.demo_metrics
//                      is not refilled, so the range now-6h shows no data there.
//   --concurrency <n>  jobs at the same time, 1 to 6 (default 2)
//   --out <dir>        output directory, relative to the repository (default target/local/compare)
//   --list             print the states and stop
//
// Chrome starts with a mouse as the primary pointer (`primaryHoverType=2`), so the CSS rules
// under `@media (hover: hover)` apply in the captures as in a desktop browser.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Browser } from "./lib/cdp.mjs";
import { settle as huneSettle, sleep } from "./shots/kit.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "../..");
const SOURCE = join(ROOT, "target/local/stream-src/grafana-stream-v2.json");

// ---- options --------------------------------------------------------------------------------------------------------

const args = process.argv.slice(2);
const KNOWN = new Set(["--only", "--themes", "--widths", "--grafana", "--hune", "--grafana-uid", "--grafana-slug", "--hune-uid", "--from", "--to", "--demo-from", "--demo-to", "--concurrency", "--out", "--list"]);
for (const a of args) {
  if (a.startsWith("--") && !KNOWN.has(a)) {
    console.error(`unknown option ${a}`);
    process.exit(2);
  }
}
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
const CFG = {
  grafana: opt("--grafana", "http://127.0.0.1:3010").replace(/\/$/, ""),
  hune: opt("--hune", "http://127.0.0.1:8088").replace(/\/$/, ""),
  gUid: opt("--grafana-uid", "stream-example"),
  gSlug: opt("--grafana-slug", "stream"),
  vUid: opt("--hune-uid", "stream"),
  from: opt("--from", "now-6h"),
  to: opt("--to", "now"),
  demoFrom: opt("--demo-from", "1794305000000"),
  demoTo: opt("--demo-to", "1794327000000"),
};
const OUT = resolve(ROOT, opt("--out", "target/local/compare"));
const PARTS = join(OUT, "parts");
const THEMES = opt("--themes", "dark,light").split(",");
const WIDTHS = opt("--widths", "1920,1280").split(",").map(Number);
const HEIGHTS = { 1920: 1080, 1280: 800, 768: 1024, 390: 844 };
const CONCURRENCY = Number(opt("--concurrency", "2"));
for (const t of THEMES) if (!["dark", "light"].includes(t)) die(`--themes: ${t} is not dark or light`);
for (const w of WIDTHS) if (!HEIGHTS[w]) die(`--widths: ${w} is not 1920, 1280, 768 or 390`);
if (!Number.isInteger(CONCURRENCY) || CONCURRENCY < 1 || CONCURRENCY > 6) die("--concurrency: an integer from 1 to 6");
function die(m) {
  console.error(m);
  process.exit(2);
}

// ---- the panels of the pair -----------------------------------------------------------------------------------------

let sourceSpec = null;
try {
  sourceSpec = JSON.parse(readFileSync(SOURCE, "utf8")).spec;
} catch {
  // The source file is not needed to run: the variable test and the titles below use it.
}

// The titles of the panels and of the collapsed row come from the source file: the repository holds no title of the
// dashboard `stream`. Without the file, a state that needs a title shows the placeholder and fails on the Grafana side.
const panelTitle = (id) => sourceSpec?.elements?.[`panel-${id}`]?.spec?.title ?? `panel ${id}`;
const PANEL = {
  series: { id: 2, title: panelTitle(2) },
  table: { id: 4, title: panelTitle(4) },
  bar: { id: 6, title: panelTitle(6) },
  statBg: { id: 21, title: panelTitle(21) },
  statValue: { id: 23, title: panelTitle(23) },
};
const COLLAPSED_ROW = (sourceSpec?.layout?.spec?.rows ?? []).find((r) => r?.spec?.collapse)?.spec?.title ?? "collapsed row";
const HAS_VARIABLES = !!sourceSpec && Array.isArray(sourceSpec.variables) && sourceSpec.variables.length > 0;

// ---- URLs -----------------------------------------------------------------------------------------------------------

const range = `from=${encodeURIComponent(CFG.from)}&to=${encodeURIComponent(CFG.to)}`;
const gDash = (extra = "", theme) => `${CFG.grafana}/d/${CFG.gUid}/${CFG.gSlug}?${range}&theme=${theme}${extra ? "&" + extra : ""}`;
const vDash = (extra = "") => `${CFG.hune}/d/${CFG.vUid}?${range}${extra ? "&" + extra : ""}`;

// The demo dashboards: one pair (Grafana uid, Hune uid) for each. Their states do not use --grafana-uid and --hune-uid
// (those two options name the pair of the older states). The range of these pairs is --demo-from and --demo-to.
const DASHBOARDS = [
  { key: "demo", uid: "demo", title: "Demo hosts" },
  { key: "density-20", uid: "density-20", title: "Density: 20 panels" },
  { key: "service-overview", uid: "service-overview", title: "Service overview" },
  { key: "edge-states", uid: "edge-states", title: "Edge states: errors, no data, gaps" },
  { key: "types-tour", uid: "types-tour", title: "Panel types tour" },
  { key: "tables-and-cells", uid: "tables-and-cells", title: "Tables, cells and transforms" },
  { key: "variables-lab", uid: "variables-lab", title: "Variables lab" },
];
const DEMO = Object.fromEntries(DASHBOARDS.map((d) => [d.key, d]));
const demoRange = `from=${encodeURIComponent(CFG.demoFrom)}&to=${encodeURIComponent(CFG.demoTo)}`;
const gDemo = (d, extra = "", theme) => `${CFG.grafana}/d/${d.uid}/${d.key}?${demoRange}&theme=${theme}${extra ? "&" + extra : ""}`;
const vDemo = (d, extra = "") => `${CFG.hune}/d/${d.uid}?${demoRange}${extra ? "&" + extra : ""}`;

/**
 * The height of a viewport that shows a whole dashboard of the pair, for the states "full". It comes from the layout of the
 * Hune document (a row unit is 38 px with its gap; a section header is about 40 px); a document that is not on disk gets 2400.
 */
function fullHeight(d) {
  try {
    for (const dir of ["demo", "ops", "showcase"]) {
      const file = join(ROOT, "target/local/volume", dir, `${d.uid}.json`);
      if (!existsSync(file)) continue;
      const doc = JSON.parse(readFileSync(file, "utf8"));
      let rows = 0;
      let headers = 0;
      for (const sec of doc.sections) {
        if (sec.title !== null) headers++;
        if (sec.collapsed) continue;
        rows += Math.max(0, ...sec.panels.map((p) => p.grid.y + p.grid.h));
      }
      return Math.min(7000, Math.max(900, 190 + rows * 38 + headers * 44 + 60));
    }
  } catch {
    // fall through
  }
  return 2400;
}

// ---- the two sides ---------------------------------------------------------------------------------------------------

const gp = (title) => `[data-testid="data-testid Panel header ${title}"]`;
const gMenu = (title) => `[data-testid="data-testid Panel menu ${title}"]`;
const vp = (id) => `[data-testid="panel-${id}"]`;

/** Wait until the Grafana page has no request in flight for a short time, then a fixed pause. */
async function gSettle(page, { panels = true, timeout = 25000 } = {}) {
  const end = Date.now() + timeout;
  if (panels) {
    await page.waitFor(`document.querySelectorAll('[data-testid^="data-testid Panel header"]').length > 0`, { timeout: 30000, what: "panel headers" }).catch(() => {});
    // The content of a panel mounts after its header (lazy load): wait for it, then for the loading bars to go.
    await page.waitFor(`document.querySelectorAll('[data-testid="data-testid panel content"]').length > 0`, { timeout: 30000, what: "panel content" }).catch(() => {});
    await page.waitFor(`document.querySelectorAll('[aria-label="Panel loading bar"]').length === 0`, { timeout: 20000, what: "no loading bar" }).catch(() => {});
    // The refresh button reads "Cancel" while a query runs.
    await page.waitFor(`(() => { const b = document.querySelector('[data-testid="data-testid RefreshPicker run button"]'); return !b || !/cancel/i.test(b.textContent); })()`, { timeout: 45000, what: "queries finished" }).catch(() => {});
  }
  let quiet = 0;
  while (Date.now() < end && quiet < 700) {
    const busy = page.requests.some((r) => r.status === null && !r.failed && Date.now() - r.t < 30000 && !/\/api\/live|\.woff|\/avatar/.test(r.url));
    quiet = busy ? 0 : quiet + 100;
    await sleep(100);
  }
  await page.eval(`document.fonts ? document.fonts.ready.then(() => true) : true`).catch(() => {});
  await sleep(700);
}

/** The Dashboards page of Grafana (the disposable instance holds the demo folders). */
async function gBrowse(g, query) {
  const page = g.page;
  await page.send("Page.navigate", { url: `${CFG.grafana}/dashboards${query}${query ? "&" : "?"}theme=${g.theme}` });
  await gSettle(page, { panels: false });
  await page.waitFor(`document.body.innerText.includes("Stream")`, { timeout: 10000 }).catch(() => {});
  await sleep(600);
}

/** Open every folder of the Grafana list (it loads them collapsed). */
async function gExpandFolders(page) {
  for (let i = 0; i < 20; i++) {
    const n = await page.eval(`(() => { const b = document.querySelector('button[aria-label^="Expand folder"]'); if (b) b.click(); return !!b; })()`);
    if (!n) break;
    await sleep(450);
  }
  await sleep(700);
}

/** The list page of Hune. Its look is the card view of 9.7 and no longer the browse page of Grafana, so the pixel comparison with Grafana
 *  does not apply to it. The helper only clears the sort of the viewer (local storage) and loads the page. The options of the old list and
 *  folder views are not read any more. */
async function vList(v, query) {
  await v.open(`${CFG.hune}/`);
  await v.settle();
  await v.page.eval(`(() => {
    try {
      localStorage.removeItem("hune.list.sort");
    } catch (e) {}
  })()`);
  await v.open(`${CFG.hune}/${query}`);
  await v.settle();
  await sleep(300);
}

/** The element of a Grafana panel (by title) or a text, scrolled into view. */
async function gScrollTo(page, title) {
  await page.eval(`(() => {
    const h = document.querySelector(${JSON.stringify(gp(title))}) || document.querySelector(${JSON.stringify(`[data-testid="data-testid dashboard-row-title-${title}"]`)});
    if (h) h.scrollIntoView({ block: "center" });
  })()`);
  await sleep(500);
}

/** Move the mouse over the box of a selector at a fraction of it. */
async function hoverAt(page, sel, fx, fy) {
  const pt = await page.eval(`(() => {
    const e = document.querySelector(${JSON.stringify(sel)});
    if (!e) return null;
    e.scrollIntoView({ block: "center", inline: "nearest" });
    const b = e.getBoundingClientRect();
    return { x: b.left + b.width * ${fx}, y: b.top + b.height * ${fy} };
  })()`);
  if (!pt) throw new Error(`no element for ${sel}`);
  await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: pt.x - 3, y: pt.y });
  await sleep(80);
  await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: pt.x, y: pt.y });
  await sleep(300);
  return pt;
}

async function huneElementByText(page, text) {
  return page.eval(`(() => {
    let best = null;
    for (const e of document.querySelectorAll("[data-testid^='section-'], h2, h3, button, span")) {
      if (e.textContent.trim() === ${JSON.stringify(text)} && (!best || e.textContent.length <= best.textContent.length)) best = e;
    }
    if (!best) return false;
    best.setAttribute("data-cmp-target", "1");
    best.scrollIntoView({ block: "center" });
    return true;
  })()`);
}

/**
 * The states. Each state has a Grafana function and a Hune function. A function opens the page and brings it to the
 * state; the harness then captures the viewport. `needs: "variables"` marks a state that exists only when the dashboard has variables.
 */
const STATES = [
  {
    id: "dashboard-top",
    text: "Dashboard page, top of the page",
    grafana: async (g) => {
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
    },
  },
  {
    id: "timeseries-legend",
    text: "A time series panel with a legend (view panel)",
    grafana: async (g) => {
      await g.open(gDash(`viewPanel=panel-${PANEL.series.id}`, g.theme));
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash(`viewPanel=${PANEL.series.id}`));
      await v.settle();
    },
  },
  {
    id: "table",
    text: "A table panel (view panel)",
    grafana: async (g) => {
      await g.open(gDash(`viewPanel=panel-${PANEL.table.id}`, g.theme));
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash(`viewPanel=${PANEL.table.id}`));
      await v.settle();
    },
  },
  {
    id: "stat-background",
    text: "A stat panel, colour mode background (view panel)",
    grafana: async (g) => {
      await g.open(gDash(`viewPanel=panel-${PANEL.statBg.id}`, g.theme));
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash(`viewPanel=${PANEL.statBg.id}`));
      await v.settle();
    },
  },
  {
    id: "stat-value",
    text: "A stat panel, colour mode value (view panel)",
    grafana: async (g) => {
      await g.open(gDash(`viewPanel=panel-${PANEL.statValue.id}`, g.theme));
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash(`viewPanel=${PANEL.statValue.id}`));
      await v.settle();
    },
  },
  {
    id: "barchart",
    text: "A bar chart panel (view panel)",
    grafana: async (g) => {
      await g.open(gDash(`viewPanel=panel-${PANEL.bar.id}`, g.theme));
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash(`viewPanel=${PANEL.bar.id}`));
      await v.settle();
    },
  },
  {
    id: "collapsed-row",
    text: `The collapsed row "${COLLAPSED_ROW}"`,
    grafana: async (g) => {
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
      await g.page.eval(`document.querySelector(${JSON.stringify(`[data-testid="data-testid dashboard-row-title-${COLLAPSED_ROW}"]`)})?.scrollIntoView({ block: "center" })`);
      await sleep(600);
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
      if (!(await huneElementByText(v.page, COLLAPSED_ROW))) throw new Error(`no section "${COLLAPSED_ROW}"`);
      await sleep(400);
      await v.settle();
    },
  },
  {
    id: "time-picker",
    text: "The time picker open",
    grafana: async (g) => {
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
      await g.page.click('[data-testid="data-testid TimePicker Open Button"]');
      await sleep(600);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
      await v.page.click('[data-testid="time-picker-button"]');
      await sleep(500);
    },
  },
  {
    id: "refresh-picker",
    text: "The refresh picker open",
    grafana: async (g) => {
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
      await g.page.click('[data-testid="data-testid RefreshPicker interval button"]');
      await sleep(600);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
      await v.page.click('[data-testid="refresh-interval"]');
      await sleep(500);
    },
  },
  {
    id: "variable-bar",
    text: "The variable bar (only when the dashboard has variables)",
    needs: "variables",
    grafana: async (g) => {
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
    },
  },
  {
    id: "panel-menu",
    text: "The panel menu open",
    grafana: async (g) => {
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
      await hoverAt(g.page, gp(PANEL.series.title), 0.5, 0.5);
      await g.page.click(gMenu(PANEL.series.title));
      await sleep(600);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
      await v.page.click(`[data-testid="panel-${PANEL.series.id}-menu"]`);
      await sleep(500);
    },
  },
  {
    id: "tooltip",
    text: "A tooltip on a time series (hover)",
    grafana: async (g) => {
      await g.open(gDash(`viewPanel=panel-${PANEL.series.id}`, g.theme));
      await gSettle(g.page);
      await hoverAt(g.page, '[data-testid="data-testid panel content"]', 0.6, 0.45);
      await sleep(400);
    },
    hune: async (v) => {
      await v.open(vDash(`viewPanel=${PANEL.series.id}`));
      await v.settle();
      // The Hune chart draws its tooltip on the overlay of the chart: move over a few points until it shows.
      const sel = `${vp(PANEL.series.id)} .chartCore__overlay`;
      await v.page.waitSel(sel, { timeout: 8000 }).catch(() => {});
      for (const fx of [0.6, 0.5, 0.7, 0.4, 0.8]) {
        await hoverAt(v.page, sel, fx, 0.45).catch(() => {});
        const shown = await v.page.eval(`(() => { const e = document.querySelector(${JSON.stringify(vp(PANEL.series.id) + " .chartCore__tooltip")}); return !!e && !e.hidden && e.getBoundingClientRect().width > 0; })()`).catch(() => false);
        if (shown) break;
      }
      await sleep(250);
    },
  },
  {
    id: "edit-mode",
    text: "Edit mode of the dashboard",
    grafana: async (g) => {
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
      await g.page.click('[data-testid="data-testid Edit dashboard button"]');
      await sleep(1200);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
      await v.page.click('[data-testid="edit-toggle"]');
      await v.page.waitSel('[data-testid="edit-bar"]', { timeout: 8000 });
      await v.settle();
    },
  },
  {
    id: "panel-editor",
    text: "The panel editor",
    grafana: async (g) => {
      // The URL editPanel alone gives an empty editor: go through the panel menu, as a user does.
      await g.open(gDash("", g.theme));
      await gSettle(g.page);
      await hoverAt(g.page, gp(PANEL.series.title), 0.5, 0.02);
      await g.page.click(gMenu(PANEL.series.title));
      await g.page.waitSel('[data-testid="data-testid Panel menu item Edit"]');
      await g.page.click('[data-testid="data-testid Panel menu item Edit"]');
      await g.page.waitFor(`!!document.querySelector('[data-testid="data-testid Query editor row"], [data-testid*="Query editor"]') || document.body.innerText.includes("Back to dashboard")`, { timeout: 20000 }).catch(() => {});
      await gSettle(g.page, { panels: false });
      await g.page.waitFor(`(() => { const b = document.querySelector('[data-testid="data-testid RefreshPicker run button"]'); return !b || !/cancel/i.test(b.textContent); })()`, { timeout: 45000 }).catch(() => {});
      await sleep(1500);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
      await v.page.click('[data-testid="edit-toggle"]');
      await v.page.waitSel('[data-testid="edit-bar"]', { timeout: 8000 });
      await v.settle();
      await v.page.click(`[data-testid="panel-${PANEL.series.id}-menu"]`);
      await v.page.click(`[data-testid="panel-${PANEL.series.id}-edit"]`);
      await v.page.waitSel('[data-testid="panel-editor"]', { timeout: 8000 });
      await v.settle();
    },
  },
  {
    id: "browse",
    text: "The list of dashboards (browse), every folder open",
    grafana: async (g) => {
      await gBrowse(g, "");
      await gExpandFolders(g.page);
    },
    hune: async (v) => {
      await vList(v, "");
    },
  },
  {
    id: "browse-collapsed",
    text: "The list of dashboards with every folder collapsed (Grafana: the first load)",
    grafana: async (g) => {
      await gBrowse(g, "");
    },
    hune: async (v) => {
      await vList(v, "", { collapsed: true });
    },
  },
  {
    id: "browse-search",
    text: "The list with a search text (Grafana lists the results flat; Hune: its list view)",
    grafana: async (g) => {
      await gBrowse(g, "?query=show");
    },
    hune: async (v) => {
      await vList(v, "?q=show", { layout: "list" });
    },
  },
  {
    id: "browse-list",
    text: "The list view (Grafana: the list layout of the toggle, every folder open)",
    grafana: async (g) => {
      await gBrowse(g, "");
      await g.page.eval(`(() => { const r = document.querySelectorAll('[role="radiogroup"] label'); if (r[1]) r[1].click(); })()`);
      await sleep(900);
    },
    hune: async (v) => {
      await vList(v, "", { layout: "list" });
    },
  },
  {
    id: "browse-empty-match",
    text: "The list with a search that finds nothing",
    grafana: async (g) => {
      await gBrowse(g, "?query=zzzz");
      await g.page.waitFor(`document.body.innerText.includes("No results found")`, { timeout: 8000 }).catch(() => {});
    },
    hune: async (v) => {
      await vList(v, "?q=zzzz");
    },
  },
  {
    id: "browse-tag-menu",
    text: "The list with the tag filter open",
    grafana: async (g) => {
      await gBrowse(g, "");
      await g.page.eval(`document.querySelector('input[aria-label="Tag filter"]').focus()`);
      await g.page.send("Input.dispatchKeyEvent", { type: "keyDown", key: "ArrowDown", code: "ArrowDown", windowsVirtualKeyCode: 40 });
      await sleep(600);
    },
    hune: async (v) => {
      await vList(v, "");
      await v.page.click('[data-testid="list-tag-filter"]');
      await sleep(400);
    },
  },
  {
    id: "browse-sort-menu",
    text: "The list with the sort menu open",
    grafana: async (g) => {
      await gBrowse(g, "");
      await g.page.eval(`document.querySelector('input[aria-label="Sort"]').focus()`);
      await g.page.send("Input.dispatchKeyEvent", { type: "keyDown", key: "ArrowDown", code: "ArrowDown", windowsVirtualKeyCode: 40 });
      await sleep(600);
    },
    hune: async (v) => {
      await vList(v, "");
      await v.page.click('[data-testid="list-sort"]');
      await sleep(400);
    },
  },
  {
    id: "theme-picker",
    text: "The theme / preferences picker",
    grafana: async (g) => {
      // The anonymous user has no profile page: the theme choice is in the default preferences of the organisation.
      await g.open(`${CFG.grafana}/org?theme=${g.theme}`);
      await gSettle(g.page, { panels: false });
      await g.page.waitFor(`!!document.querySelector('#theme-select, [data-testid="data-testid Preferences Theme"], input[id*="theme"]')`, { timeout: 8000 }).catch(() => {});
      await g.page.eval(`(() => { const e = document.querySelector('#theme-select, input[id*="theme"]'); if (e) e.click(); })()`).catch(() => {});
      await sleep(700);
    },
    hune: async (v) => {
      await v.open(vDash());
      await v.settle();
      // The theme selector of the header: one icon button. Its menu opens (the same selector on every page, 9.25).
      await v.page.click('[data-testid="theme-toggle"]').catch(() => {});
      await sleep(400);
    },
  },
];

// ---- the states of the demo dashboards, the dialogs, the error pages and the mobile widths --------------------------------
//
// A state can have these extra members:
//   dash      the pair of the demo dashboards that it opens (documentation only)
//   widths    the widths at which it runs (default: all the widths of --widths)
//   tall      true: the viewport is as high as the whole dashboard (fullHeight), for both sides
//   cleanup   { grafana, hune }: run after the capture, to close what the state opened (cancel a dialog)

const gOpenDemo = async (g, d, extra = "") => {
  await g.open(gDemo(d, extra, g.theme));
  await gSettle(g.page);
};
const vOpenDemo = async (v, d, extra = "") => {
  await v.open(vDemo(d, extra));
  await v.settle();
};

for (const d of DASHBOARDS) {
  STATES.push({
    id: `${d.key}-top`,
    dash: d.key,
    text: `Dashboard "${d.title}", top of the page (range ${CFG.demoFrom} to ${CFG.demoTo})`,
    grafana: (g) => gOpenDemo(g, d),
    hune: (v) => vOpenDemo(v, d),
  });
  if (d.key === "demo") continue; // the dashboard demo fits in the top state
  STATES.push({
    id: `${d.key}-full`,
    dash: d.key,
    tall: true,
    widths: [1920, 1280],
    text: `Dashboard "${d.title}", the whole page (the viewport is as high as the layout)`,
    grafana: (g) => gOpenDemo(g, d),
    hune: (v) => vOpenDemo(v, d),
  });
}

STATES.push({
  id: "variables-lab-dropdown",
  dash: "variables-lab",
  text: "Variables lab: the list of the multi-value variable Host open",
  grafana: async (g) => {
    await gOpenDemo(g, DEMO["variables-lab"]);
    await g.page.click('[data-testid^="data-testid Dashboard template variables Variable Value DropDown value link"][data-testid$="-input"]');
    await sleep(900);
  },
  hune: async (v) => {
    await vOpenDemo(v, DEMO["variables-lab"]);
    await v.page.click('[data-testid="variable-host-button"]');
    await sleep(600);
  },
});

STATES.push({
  id: "dialog-save",
  dash: "demo",
  text: "Edit mode, the save dashboard dialog (Grafana: the Save dashboard drawer). Cancelled, never saved.",
  grafana: async (g) => {
    await gOpenDemo(g, DEMO.demo);
    await g.page.click('[data-testid="data-testid Edit dashboard button"]');
    await sleep(1000);
    await g.page.click('[data-testid="data-testid Save dashboard button"]');
    await g.page.waitFor(`document.body.innerText.includes("Save dashboard") && !!document.querySelector('[role="dialog"], [data-testid*="drawer"]')`, { timeout: 8000 }).catch(() => {});
    await sleep(600);
  },
  hune: async (v) => {
    await vOpenDemo(v, DEMO.demo);
    await v.page.click('[data-testid="edit-toggle"]');
    await v.page.waitSel('[data-testid="edit-bar"]', { timeout: 8000 });
    await v.settle();
    // Hune enables Save only after a change (Grafana always does). The change is a new empty section at the end of the
    // page, below the first screen; the draft is never saved.
    // The action is in the menu of the button "Add" of the header (10.2).
    await v.page.click('[data-testid="add-menu"]');
    await v.page.waitSel('.menu [data-testid="add-section"]');
    await v.page.click('.menu [data-testid="add-section"]');
    await sleep(500);
    await v.page.click('[data-testid="save-button"]');
    await v.page.waitSel('[data-testid="save-dialog"]', { timeout: 8000 });
    await sleep(400);
  },
  cleanup: {
    grafana: (g) => g.page.clickText("button", "Cancel"),
    hune: (v) => v.page.clickText('[data-testid="save-dialog"] button', "Cancel"),
  },
});

STATES.push({
  id: "dialog-delete",
  dash: "demo",
  text: "The delete dashboard confirmation (Grafana: dashboard settings, Delete dashboard). Cancelled, never deleted.",
  grafana: async (g) => {
    await gOpenDemo(g, DEMO.demo, "editview=settings");
    await g.page.waitFor(`[...document.querySelectorAll("button")].some((b) => /^Delete dashboard$/i.test(b.textContent.trim()))`, { timeout: 15000 }).catch(() => {});
    await g.page.eval(`(() => { const b = [...document.querySelectorAll("button")].find((x) => /^Delete dashboard$/i.test(x.textContent.trim())); if (b) b.scrollIntoView({ block: "center" }); })()`);
    await sleep(400);
    await g.page.clickText("button", "Delete dashboard");
    await g.page.waitFor(`!!document.querySelector('[role="dialog"]')`, { timeout: 8000 }).catch(() => {});
    await sleep(600);
  },
  hune: async (v) => {
    await vOpenDemo(v, DEMO.demo);
    await v.page.click('[data-testid="dashboard-menu"]');
    await v.page.waitSel('[role="menu"]', { timeout: 5000 });
    await v.page.clickText('[role="menuitem"]', "Delete");
    await v.page.waitSel('[data-testid="delete-dashboard-dialog"]', { timeout: 8000 });
    await sleep(400);
  },
  cleanup: {
    grafana: (g) => g.page.clickText('[role="dialog"] button', "Cancel"),
    hune: (v) => v.page.clickText('[data-testid="delete-dashboard-dialog"] button', "Cancel"),
  },
});

STATES.push({
  id: "dialog-import",
  text: "The import dialog of the list (Grafana: the page Import dashboard). Cancelled, nothing imported.",
  grafana: async (g) => {
    await g.open(`${CFG.grafana}/dashboard/import?theme=${g.theme}`);
    await gSettle(g.page, { panels: false });
    await g.page.waitFor(`document.body.innerText.includes("Import dashboard")`, { timeout: 10000 }).catch(() => {});
    await sleep(600);
  },
  hune: async (v) => {
    await vList(v, "");
    // Up to 768 px the three list actions are in the menu "New": open it, then choose Import.
    const narrow = await v.page.eval(`(() => { const b = document.querySelector('[data-testid="import"]'); return !b || b.getBoundingClientRect().width === 0; })()`);
    if (narrow) {
      await v.page.click('[data-testid="list-new-menu"]');
      await v.page.clickText('.menu [role="menuitem"]', "Import");
    } else {
      await v.page.click('[data-testid="import"]');
    }
    await v.page.waitSel('[data-testid="import-dialog"]', { timeout: 8000 });
    await sleep(400);
  },
  cleanup: {
    hune: (v) => v.page.clickText('[data-testid="import-dialog"] button', "Cancel"),
  },
});

STATES.push({
  id: "error-dashboard-not-found",
  text: "The page of a dashboard that does not exist",
  grafana: async (g) => {
    await g.open(`${CFG.grafana}/d/does-not-exist/nothing?theme=${g.theme}`);
    await g.page.waitFor(`/not found/i.test(document.body.innerText)`, { timeout: 15000 }).catch(() => {});
    await gSettle(g.page, { panels: false });
  },
  hune: async (v) => {
    await v.open(`${CFG.hune}/d/does-not-exist`);
    await v.page.waitSel('[data-testid="dashboard-not-found"]', { timeout: 8000 }).catch(() => {});
    await v.settle();
  },
});

STATES.push({
  id: "error-page-not-found",
  text: "The page of an address that does not exist",
  grafana: async (g) => {
    await g.open(`${CFG.grafana}/this/page/does/not/exist?theme=${g.theme}`);
    await g.page.waitFor(`/not found/i.test(document.body.innerText)`, { timeout: 15000 }).catch(() => {});
    await gSettle(g.page, { panels: false });
  },
  hune: async (v) => {
    await v.open(`${CFG.hune}/this/page/does/not/exist`);
    await v.page.waitSel('[data-testid="not-found"]', { timeout: 8000 }).catch(() => {});
    await v.settle();
  },
});

// The mobile group: three states at 768 and 390. They use the dashboard demo (it fits a phone).
STATES.push({
  id: "mobile-dashboard-top",
  dash: "demo",
  widths: [768, 390],
  text: "Mobile: the dashboard demo, top of the page",
  grafana: (g) => gOpenDemo(g, DEMO.demo),
  hune: (v) => vOpenDemo(v, DEMO.demo),
});
STATES.push({
  id: "mobile-list",
  widths: [768, 390],
  text: "Mobile: the list of dashboards, every folder open",
  grafana: async (g) => {
    await gBrowse(g, "");
    await gExpandFolders(g.page);
  },
  hune: (v) => vList(v, ""),
});
STATES.push({
  id: "mobile-time-picker",
  dash: "demo",
  widths: [768, 390],
  text: "Mobile: the time picker open on the dashboard demo",
  grafana: async (g) => {
    await gOpenDemo(g, DEMO.demo);
    await g.page.click('[data-testid="data-testid TimePicker Open Button"]');
    await sleep(700);
  },
  hune: async (v) => {
    await vOpenDemo(v, DEMO.demo);
    await v.page.click('[data-testid="time-picker-button"]');
    await sleep(500);
  },
});

/** The height of the viewport of a state at a width. */
const heightFor = (state, width) => (state.tall ? fullHeight(DEMO[state.dash]) : HEIGHTS[width]);

// ---- capture ---------------------------------------------------------------------------------------------------------

class Side {
  constructor(page, theme, name) {
    this.page = page;
    this.theme = theme;
    this.name = name;
  }
  async open(url) {
    if (this.name === "hune") await this.page.goto(url);
    else await this.page.send("Page.navigate", { url });
  }
  settle() {
    return huneSettle(this.page, { timeout: 25000 });
  }
}

async function openSidePage(ctx, side, theme, width, height = HEIGHTS[width]) {
  const page = await ctx.newPage({ width, height });
  page.base = { width, height };
  await page.send("Emulation.setEmulatedMedia", {
    features: [
      { name: "prefers-reduced-motion", value: "reduce" },
      { name: "prefers-color-scheme", value: theme },
    ],
  });
  if (side === "hune") {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `try { localStorage.setItem("hune.theme", ${JSON.stringify(theme)}); } catch (e) {}`,
    });
  } else {
    // Grafana docks its navigation menu (300 px) on wide windows. Hune has no left navigation, so the
    // capture closes the menu (the state that its "Undock menu" button stores): both canvases then have the
    // width of the window.
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `try { localStorage.setItem("grafana.navigation.docked", "false"); } catch (e) {}`,
    });
  }
  return new Side(page, theme, side);
}

async function shoot(side) {
  const r = await side.page.send("Page.captureScreenshot", { format: "png" });
  return r.data;
}

/** One side of one state: a PNG (base64) or a note. */
async function runSide(ctx, name, state, theme, width, available) {
  if (!available.ok) return { note: available.note };
  let side = null;
  try {
    side = await openSidePage(ctx, name, theme, width, heightFor(state, width));
    await state[name](side);
    const text = await side.page.eval(`document.body ? document.body.innerText.slice(0, 400) : ""`).catch(() => "");
    const png = await shoot(side);
    // A state that opened a dialog closes it again: the harness never saves, deletes or imports.
    if (state.cleanup && state.cleanup[name]) await state.cleanup[name](side).catch(() => {});
    const out = { png };
    if (name === "grafana" && /Failed to load|Dashboard not found|Page not found/i.test(text)) out.warn = "the page shows an error text";
    return out;
  } catch (e) {
    let png = null;
    if (side) {
      try {
        png = await shoot(side);
      } catch {
        // no capture
      }
    }
    return { png, note: `${name}: ${String(e.message).split("\n")[0]}`, failed: true };
  } finally {
    if (side) await side.page.close().catch(() => {});
  }
}

// ---- availability ----------------------------------------------------------------------------------------------------

async function get(url) {
  try {
    const r = await fetch(url, { signal: AbortSignal.timeout(5000) });
    return { status: r.status, text: await r.text() };
  } catch (e) {
    return { status: 0, text: String(e.message) };
  }
}

async function probe() {
  const info = { grafana: { ok: false, note: "", version: "" }, hune: { ok: false, note: "" } };
  const h = await get(`${CFG.grafana}/api/health`);
  if (h.status !== 200) info.grafana.note = `Grafana is not reachable at ${CFG.grafana}`;
  else {
    try {
      info.grafana.version = JSON.parse(h.text).version;
    } catch {
      // no version
    }
    const d = await get(`${CFG.grafana}/api/dashboards/uid/${CFG.gUid}`);
    if (d.status === 200) info.grafana.ok = true;
    else {
      // A v2 dashboard may answer 404 here and exist under the search.
      const s = await get(`${CFG.grafana}/api/search?query=${encodeURIComponent(CFG.gSlug)}`);
      info.grafana.ok = s.status === 200 && s.text.includes(`"${CFG.gUid}"`);
      if (!info.grafana.ok) info.grafana.note = `Grafana has no dashboard with the uid ${CFG.gUid}`;
    }
  }
  const v = await get(`${CFG.hune}/api/dashboards/${CFG.vUid}`);
  if (v.status === 200) info.hune.ok = true;
  else if (v.status === 0) info.hune.note = `Hune is not reachable at ${CFG.hune}`;
  else info.hune.note = `Hune has no dashboard "${CFG.vUid}" yet (HTTP ${v.status})`;
  return info;
}

// ---- composition -----------------------------------------------------------------------------------------------------

/** Compose the two captures in a page with a canvas. Returns a PNG as base64. */
async function compose(page, left, right, width, height, labels) {
  const script = `(async () => {
    const W = ${width}, H = ${height}, BAR = 28, GAP = 6;
    const load = (b64) => new Promise((res, rej) => { if (!b64) return res(null); const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.src = "data:image/png;base64," + b64; });
    const L = ${JSON.stringify(left.png || null)}, R = ${JSON.stringify(right.png || null)};
    const [a, b] = await Promise.all([load(L), load(R)]);
    const c = document.createElement("canvas");
    c.width = W * 2 + GAP; c.height = H + BAR;
    const x = c.getContext("2d");
    x.fillStyle = "#808080"; x.fillRect(0, 0, c.width, c.height);
    x.font = "600 15px sans-serif"; x.textBaseline = "middle";
    const side = (img, note, ox, label) => {
      x.fillStyle = "#222"; x.fillRect(ox, 0, W, BAR);
      x.fillStyle = "#fff"; x.fillText(label, ox + 10, BAR / 2);
      if (img) { x.drawImage(img, ox, BAR, W, H); }
      if (note) {
        if (!img) { x.fillStyle = "#e8e8e8"; x.fillRect(ox, BAR, W, H); }
        x.fillStyle = img ? "rgba(160,20,20,.92)" : "#555";
        if (img) x.fillRect(ox, BAR, W, 26);
        x.fillStyle = img ? "#fff" : "#444";
        x.font = img ? "600 14px sans-serif" : "600 22px sans-serif";
        x.textAlign = img ? "left" : "center";
        const tx = img ? ox + 10 : ox + W / 2, ty = img ? BAR + 13 : BAR + H / 2;
        x.fillText(note, tx, ty);
        x.textAlign = "left"; x.font = "600 15px sans-serif";
      }
    };
    side(a, ${JSON.stringify(left.note || "")}, 0, ${JSON.stringify(labels[0])});
    side(b, ${JSON.stringify(right.note || "")}, W + GAP, ${JSON.stringify(labels[1])});
    return c.toDataURL("image/png").split(",")[1];
  })()`;
  return page.eval(script);
}

// ---- index -----------------------------------------------------------------------------------------------------------

function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
}

function writeIndex(fresh, info, skipped) {
  // Keep the sheets of earlier runs: the manifest lists each sheet by its file name.
  const manifest = join(OUT, "results.json");
  let old = [];
  try {
    old = JSON.parse(readFileSync(manifest, "utf8"));
  } catch {
    // no earlier run
  }
  const slim = (r) => ({ id: r.state.id, text: r.state.text, theme: r.theme, width: r.width, file: r.file, partG: r.partG, partV: r.partV, grafana: { note: r.grafana.note || "", warn: r.grafana.warn || "" }, hune: { note: r.hune.note || "" } });
  const merged = new Map(old.map((r) => [r.file, r]));
  for (const r of fresh) merged.set(r.file, slim(r));
  const order = new Map(STATES.map((s, i) => [s.id, i]));
  const all = [...merged.values()].filter((r) => existsSync(join(OUT, r.file)));
  all.sort((a, b) => (order.get(a.id) ?? 99) - (order.get(b.id) ?? 99) || ["dark", "light"].indexOf(a.theme) - ["dark", "light"].indexOf(b.theme) || b.width - a.width);
  writeFileSync(manifest, JSON.stringify(all, null, 1));
  const results = all.map((r) => ({ ...r, state: { id: r.id, text: r.text } }));
  const rel = (n) => encodeURI(n);
  const byState = new Map();
  for (const r of results) {
    if (!byState.has(r.state.id)) byState.set(r.state.id, { state: r.state, items: [] });
    byState.get(r.state.id).items.push(r);
  }
  let body = "";
  for (const { state, items } of byState.values()) {
    body += `<section id="${esc(state.id)}"><h2>${esc(state.id)} <small>${esc(state.text)}</small></h2><div class="grid">`;
    for (const r of items) {
      const notes = [r.grafana.note, r.hune.note, r.grafana.warn].filter(Boolean).map(esc).join(" / ");
      body += `<figure><a href="${rel(r.file)}"><img loading="lazy" src="${rel(r.file)}" alt="${esc(state.id)} ${r.theme} ${r.width}"></a><figcaption>${r.theme} ${r.width} px` +
        ` · <a href="${rel("parts/" + r.partG)}">grafana</a> · <a href="${rel("parts/" + r.partV)}">hune</a>${notes ? `<br><span class="note">${notes}</span>` : ""}</figcaption></figure>`;
    }
    body += `</div></section>`;
  }
  for (const s of skipped) body += `<section><h2>${esc(s.id)} <small>${esc(s.text)}</small></h2><p class="note">Not captured: ${esc(s.why)}</p></section>`;
  const nav = [...byState.keys()].map((id) => `<a href="#${esc(id)}">${esc(id)}</a>`).join(" ");
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Grafana and Hune, side by side</title>
<style>
:root { --bg:#fff; --fg:#1b1d22; --mut:#5b6270; --line:#d5d8de; --warn:#a01414; color-scheme: light dark; }
@media (prefers-color-scheme: dark) { :root { --bg:#14161a; --fg:#e6e8ec; --mut:#9aa1ae; --line:#2f333b; --warn:#ff8a8a; } }
body { margin:0; padding:16px 24px 48px; background:var(--bg); color:var(--fg); font:14px/1.45 system-ui, sans-serif; }
h1 { font-size:20px; margin:0 0 4px; } h2 { font-size:16px; margin:28px 0 8px; } h2 small { font-weight:400; color:var(--mut); margin-left:8px; }
a { color:inherit; } nav { margin:12px 0; line-height:1.9; } nav a { margin-right:10px; }
.meta { color:var(--mut); } .grid { display:grid; grid-template-columns:repeat(auto-fill, minmax(560px, 1fr)); gap:16px; }
figure { margin:0; border:1px solid var(--line); padding:8px; } img { width:100%; height:auto; display:block; } figcaption { margin-top:6px; color:var(--mut); }
.note { color:var(--warn); }
</style></head><body>
<h1>Grafana (left) and Hune (right)</h1>
<p class="meta">Grafana ${esc(info.grafana.version || "not reachable")} · dashboard ${esc(CFG.gUid)} · Hune dashboard ${esc(CFG.vUid)} · range ${esc(CFG.from)} to ${esc(CFG.to)} · ${esc(new Date().toISOString())}</p>
<p class="meta">${info.grafana.ok ? "" : esc(info.grafana.note) + ". "}${info.hune.ok ? "" : esc(info.hune.note) + ". "}</p>
<nav>${nav}</nav>
${body}
</body></html>
`;
  writeFileSync(join(OUT, "index.html"), html);
}

// ---- main ------------------------------------------------------------------------------------------------------------

function selectStates() {
  const only = opt("--only", null);
  if (!only) return STATES;
  const res = only.split(",").map((p) => new RegExp("^" + p.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*") + "$"));
  const sel = STATES.filter((s) => res.some((re) => re.test(s.id)));
  if (sel.length === 0) die(`--only: no state matches ${only}`);
  return sel;
}

if (args.includes("--list")) {
  for (const s of STATES) console.log(`${s.id.padEnd(26)} ${s.widths ? `[${s.widths.join(",")}] ` : ""}${s.text}`);
  process.exit(0);
}

const chosen = selectStates();
const skipped = [];
const states = chosen.filter((s) => {
  if (s.needs === "variables" && !HAS_VARIABLES) {
    skipped.push({ ...s, why: "the dashboard has no variables" });
    return false;
  }
  return true;
});

mkdirSync(PARTS, { recursive: true });
const info = await probe();
console.log(`grafana: ${info.grafana.ok ? "ok " + info.grafana.version : info.grafana.note}`);
console.log(`hune:   ${info.hune.ok ? "ok" : info.hune.note}`);
if (!info.grafana.ok && !info.hune.ok) {
  console.error("neither side is available: nothing to capture");
  process.exit(1);
}

async function setGrafanaTheme(theme) {
  if (!info.grafana.ok) return;
  try {
    await fetch(`${CFG.grafana}/api/org/preferences`, { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify({ theme }) });
  } catch {
    // the theme parameter of the URL still applies
  }
}

const profile = join(OUT, ".profile");
const browser = await Browser.launch({ profileDir: profile, width: 1920, height: 1080, args: ["--blink-settings=primaryHoverType=2,availableHoverTypes=2"] });
const composer = await (await browser.newContext()).newPage({ width: 800, height: 600 });
await composer.send("Page.navigate", { url: "about:blank" });
const results = [];
let failures = 0;

async function job(theme, width, state) {
  const ctx = await browser.newContext();
  try {
    const [g, v] = await Promise.all([runSide(ctx, "grafana", state, theme, width, info.grafana), runSide(ctx, "hune", state, theme, width, info.hune)]);
    const name = `${state.id}-${theme}-${width}`;
    const png = await compose(composer, g, v, width, heightFor(state, width), [`Grafana ${info.grafana.version}`, "Hune"]);
    writeFileSync(join(OUT, `${name}.png`), Buffer.from(png, "base64"));
    if (g.png) writeFileSync(join(PARTS, `${name}-grafana.png`), Buffer.from(g.png, "base64"));
    if (v.png) writeFileSync(join(PARTS, `${name}-hune.png`), Buffer.from(v.png, "base64"));
    if (g.failed || v.failed) failures++;
    results.push({ state, theme, width, file: `${name}.png`, partG: `${name}-grafana.png`, partV: `${name}-hune.png`, grafana: g, hune: v });
    const flag = [g.note && `G: ${g.note}`, v.note && `V: ${v.note}`].filter(Boolean).join(" | ");
    console.log(`${g.failed || v.failed ? "warn" : "ok  "} ${name}${flag ? "  " + flag : ""}`);
  } finally {
    await ctx.dispose();
  }
}

try {
  for (const theme of THEMES) {
    await setGrafanaTheme(theme);
    const queue = [];
    for (const width of WIDTHS) for (const state of states) if (!state.widths || state.widths.includes(width)) queue.push([width, state]);
    const workers = Array.from({ length: CONCURRENCY }, async () => {
      for (;;) {
        const next = queue.shift();
        if (!next) return;
        await job(theme, next[0], next[1]);
      }
    });
    await Promise.all(workers);
  }
} finally {
  const order = new Map(STATES.map((s, i) => [s.id, i]));
  results.sort((a, b) => order.get(a.state.id) - order.get(b.state.id) || THEMES.indexOf(a.theme) - THEMES.indexOf(b.theme) || b.width - a.width);
  writeIndex(results, info, skipped);
  await browser.close();
  rmSync(profile, { recursive: true, force: true });
}
console.log(`done: ${results.length} sheets, ${failures} with a problem, ${skipped.length} state(s) not applicable. ${join(OUT, "index.html")}`);
process.exit(0);
