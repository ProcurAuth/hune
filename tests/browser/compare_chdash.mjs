#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
// Side-by-side harness for the chrome: ClickHouse Dash (left) and Hune (right).
//
// Use:  node tests/browser/compare_chdash.mjs --hune http://127.0.0.1:8196 --out target/ws/f-theme/compare-chdash
//       node tests/browser/compare_chdash.mjs --only header,menu --themes dark --widths 1280
//       node tests/browser/compare_chdash.mjs --list
//
// The Grafana sheets (compare.mjs) keep their role for the panels. These sheets compare the CHROME only: the header, a menu,
// a dialog, the list page and the page of an error, in the dark and the light theme.
//
// The ClickHouse Dash side:
//   - with --chdash URL, the harness opens the live pages of a running instance (only /query: its header and its states);
//   - without it, the harness serves the static files of the repository (src/static, src/shell) with a small server of its
//     own, on a free port of 127.0.0.1, and it draws a page of the same states from the real classes and the real
//     stylesheet (index.css and its layers). The server stops when the run ends.
// The repository of ClickHouse Dash is read-only: the harness reads it and writes nothing there. Set --chdash-repo or
// CHDASH_REPO to change its path (default target/clickhouse-dash). The harness builds and starts nothing else: it needs a
// Hune server (--hune) with some dashboards.
//
// Output: <out>/<state>-<theme>-<width>.png (ClickHouse Dash left, Hune right), <out>/parts/..., <out>/index.html.
import { createServer } from "node:http";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Browser } from "./lib/cdp.mjs";
import { settle } from "./shots/kit.mjs";
import { sleep } from "./lib/cdp.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "../..");

const args = process.argv.slice(2);
const KNOWN = new Set(["--only", "--themes", "--widths", "--hune", "--chdash", "--chdash-repo", "--out", "--list"]);
for (const a of args) {
  if (a.startsWith("--") && !KNOWN.has(a) && !a.startsWith("--only=")) {
    console.error(`unknown option ${a}`);
    process.exit(2);
  }
}
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
const HUNE = opt("--hune", "http://127.0.0.1:8196").replace(/\/$/, "");
const CHDASH_URL = opt("--chdash", "").replace(/\/$/, "");
const CHDASH_REPO = resolve(ROOT, opt("--chdash-repo", process.env.CHDASH_REPO || "target/clickhouse-dash"));
const OUT = resolve(ROOT, opt("--out", "target/ws/f-theme/compare-chdash"));
const PARTS = join(OUT, "parts");
const THEMES = opt("--themes", "dark,light").split(",");
const WIDTHS = opt("--widths", "1920,1280").split(",").map(Number);
const HEIGHTS = { 1920: 1080, 1280: 800 };
for (const t of THEMES) if (!["dark", "light"].includes(t)) die(`--themes: ${t} is not dark or light`);
for (const w of WIDTHS) if (!HEIGHTS[w]) die(`--widths: ${w} is not 1920 or 1280`);

function die(m) {
  console.error(m);
  process.exit(2);
}

// ---- the states -----------------------------------------------------------------------------------------------------
// Each state has: a ClickHouse Dash page (the id of a specimen), and the actions on the Hune page.

const click = (side, sel) => side.page.click(sel);
const STATES = [
  { id: "header", text: "the header and the list page at rest", chdash: "list", hune: async (v) => v.open("/") },
  {
    id: "menu",
    text: "a menu open (Sort on the list page, a menu of options on ClickHouse Dash)",
    chdash: "menu",
    hune: async (v) => {
      await v.open("/");
      await click(v, '[data-testid="list-sort"]');
      await sleep(400);
    },
  },
  {
    id: "dialog",
    text: "a modal dialog",
    chdash: "dialog",
    hune: async (v) => {
      await v.open("/");
      await click(v, '[data-testid="list-new-menu"]');
      await click(v, '[data-testid="new-folder"]');
      await sleep(500);
    },
    cleanup: async (v) => {
      await v.page.press("Escape");
    },
  },
  { id: "error-page", text: "the page of an error (not found)", chdash: "error", hune: async (v) => v.open("/d/this-dashboard-does-not-exist") },
  {
    id: "dashboard-header",
    text: "the header of a dashboard (the time controls and the actions)",
    chdash: "list",
    hune: async (v) => {
      await v.open("/d/stream?from=now-6h&to=now");
      await sleep(1500);
    },
    note: "ClickHouse Dash has no dashboard page: the left side is its list page.",
  },
];

if (args.includes("--list")) {
  for (const s of STATES) console.log(`${s.id.padEnd(18)} ${s.text}`);
  process.exit(0);
}
const only = opt("--only", "");
const wanted = only ? only.split(",") : STATES.map((s) => s.id);
for (const w of wanted) if (!STATES.some((s) => s.id === w)) die(`--only: unknown state ${w} (use --list)`);
const states = STATES.filter((s) => wanted.includes(s.id));

// ---- the static ClickHouse Dash server -----------------------------------------------------------------------------------

const MIME = { ".css": "text/css", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".html": "text/html; charset=utf-8", ".js": "text/javascript" };
const STATIC = join(CHDASH_REPO, "src/static");

function headerHtml() {
  let h = readFileSync(join(CHDASH_REPO, "src/shell/header.html"), "utf8");
  h = h.replace(/<!--[\s\S]*?-->/g, "").replace(/<script>[\s\S]*?<\/script>/g, "");
  h = h.replace("{{main.id}}", "main").replace("{{page.label}}", "Dashboards").replace(/\{\{[a-z.]+\}\}/g, "");
  h = h.replace(/\/static\/icons\.svg\?v=[0-9a-f]+/g, "/static/icons.svg");
  return h;
}

const ROWS = [
  ["Service overview", "ops", "ok", "2 minutes ago"],
  ["Stream", "stream", "ok", "1 hour ago"],
  ["Edge states", "showcase", "warn", "yesterday"],
  ["Broken layout", "ops", "error", "3 days ago"],
  ["Types tour", "showcase", "ok", "last week"],
];

function listBody() {
  const rows = ROWS.map(
    (r, i) =>
      `<tr${i === 1 ? ' class="is-selected"' : ""}><td>${r[0]}</td><td><span class="badge badge--neutral">${r[1]}</span></td><td><span class="badge badge--${r[2]}">${r[2]}</span></td><td class="num">${r[3]}</td></tr>`,
  ).join("");
  return `<div style="display:flex;gap:var(--space-8);align-items:center;margin-bottom:var(--space-12)">
    <input class="uiInput" style="max-width:280px" placeholder="Search dashboards" aria-label="Search">
    <button class="button" type="button">Sort</button>
    <button class="button button--primary" type="button">New</button>
    <button class="button button--danger" type="button">Delete</button>
    <button class="button" type="button" disabled>Disabled</button>
    <button class="button button--small" type="button">Small</button>
  </div>
  <table class="dataTable"><thead><tr><th>Title</th><th>Folder</th><th>Status</th><th class="num">Changed</th></tr></thead><tbody>${rows}</tbody></table>`;
}

function specimen(kind, theme) {
  const header = headerHtml();
  let body = listBody();
  let extra = "";
  if (kind === "menu") {
    extra = `<div class="uiPopover" style="top:124px;left:calc(100% - 232px);width:200px;padding:var(--space-4)" role="menu">
      <button class="pickerOption" type="button" role="menuitem" style="width:100%">New dashboard</button>
      <button class="pickerOption" type="button" role="menuitem" style="width:100%" aria-selected="true">New folder</button>
      <button class="pickerOption" type="button" role="menuitem" style="width:100%">Import</button></div>`;
  }
  if (kind === "dialog") {
    extra = `<dialog open class="uiDialog uiDialog--sm" style="position:fixed;inset:0;margin:auto;z-index:var(--z-modal);width:520px;height:fit-content" aria-labelledby="dt"><div class="uiDialog__frame">
      <div class="uiDialog__head"><div class="uiDialog__heading"><h2 id="dt" class="uiDialog__title">New folder</h2></div>
      <div class="uiDialog__actions"><button class="closeCross uiDialog__close" type="button" aria-label="Close"><svg class="icon" aria-hidden="true"><use href="/static/icons.svg#i-x"/></svg></button></div></div>
      <div class="uiDialog__body"><label>Name<br><input class="uiInput" value="Production"></label></div>
      <div class="uiDialog__foot"><button class="button" type="button">Cancel</button><button class="button button--primary" type="button">Create</button></div></div></dialog>
      <div style="position:fixed;inset:0;background:var(--backdrop);z-index:calc(var(--z-modal) - 1)"></div>`;
  }
  if (kind === "error") {
    body = `<section class="uiState uiState--error" role="alert"><div class="uiState__title">Dashboard not found</div>
      <p class="uiState__body">The dashboard does not exist, or you cannot see it.</p>
      <div class="uiState__actions"><a class="button uiState__action" href="#">Back to the list</a></div></section>`;
  }
  return `<!doctype html><html lang="en" data-theme="${theme}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ClickHouse Dash chrome specimen</title><link rel="stylesheet" href="/static/css/index.css"></head>
<body style="margin:0;background:var(--bg);color:var(--text)">${header}<main id="main" style="padding:var(--space-16)">${body}</main>${extra}</body></html>`;
}

function startStatic() {
  return new Promise((res, rej) => {
    const srv = createServer((req, rsp) => {
      const url = new URL(req.url, "http://x");
      const send = (code, type, data) => {
        rsp.writeHead(code, { "content-type": type });
        rsp.end(data);
      };
      let p = decodeURIComponent(url.pathname);
      if (p.startsWith("/page/")) return send(200, MIME[".html"], specimen(p.slice(6), url.searchParams.get("theme") || "dark"));
      if (p.startsWith("/static/css/fonts/")) p = "/static/fonts/" + p.slice("/static/css/fonts/".length);
      if (!p.startsWith("/static/") || p.includes("..")) return send(404, "text/plain", "not found");
      const f = join(STATIC, p.slice("/static/".length));
      if (!existsSync(f)) return send(404, "text/plain", "not found");
      send(200, MIME[extname(f)] || "application/octet-stream", readFileSync(f));
    });
    srv.on("error", rej);
    srv.listen(0, "127.0.0.1", () => res({ srv, port: srv.address().port }));
  });
}

// ---- the run ------------------------------------------------------------------------------------------------------------

async function reachable(url) {
  try {
    return (await fetch(url, { signal: AbortSignal.timeout(4000) })).status < 500;
  } catch {
    return false;
  }
}

const huneOk = await reachable(`${HUNE}/api/dashboards/stream`);
if (!huneOk) console.error(`warning: Hune is not reachable at ${HUNE}: the right side has a placeholder.`);
let chdashLive = false;
if (CHDASH_URL) chdashLive = await reachable(`${CHDASH_URL}/query`);
let staticServer = null;
let staticBase = "";
if (!chdashLive) {
  if (!existsSync(join(STATIC, "css/index.css"))) die(`the repository of ClickHouse Dash is not at ${CHDASH_REPO}`);
  const s = await startStatic();
  staticServer = s.srv;
  staticBase = `http://127.0.0.1:${s.port}`;
}
const chdashMode = chdashLive ? `live ${CHDASH_URL}` : `static files of ${CHDASH_REPO} (${staticBase})`;
console.log(`ClickHouse Dash side: ${chdashMode}`);

mkdirSync(PARTS, { recursive: true });
const profile = join(OUT, ".profile");
const browser = await Browser.launch({ profileDir: profile, width: 1920, height: 1080, args: ["--blink-settings=primaryHoverType=2,availableHoverTypes=2"] });
const composer = await (await browser.newContext()).newPage({ width: 800, height: 600 });
await composer.send("Page.navigate", { url: "about:blank" });
const results = [];

async function openPage(ctx, theme, width, init) {
  const page = await ctx.newPage({ width, height: HEIGHTS[width] });
  await page.send("Emulation.setEmulatedMedia", {
    features: [
      { name: "prefers-reduced-motion", value: "reduce" },
      { name: "prefers-color-scheme", value: theme },
    ],
  });
  if (init) await page.send("Page.addScriptToEvaluateOnNewDocument", { source: init });
  return page;
}

async function shoot(page) {
  return (await page.send("Page.captureScreenshot", { format: "png" })).data;
}

async function chdashSide(ctx, state, theme, width) {
  const init = chdashLive ? `try { localStorage.setItem("chdash.themeMode.v1", ${JSON.stringify(theme)}); localStorage.setItem("chdash.theme", ${JSON.stringify(theme)}); } catch (e) {}` : null;
  const page = await openPage(ctx, theme, width, init);
  try {
    await page.send("Page.navigate", { url: chdashLive ? `${CHDASH_URL}/query` : `${staticBase}/page/${state.chdash}?theme=${theme}` });
    await page.waitFor(`document.readyState !== "loading"`, { timeout: 15000 });
    await sleep(600);
    if (chdashLive) await page.eval(`document.documentElement.setAttribute("data-theme", ${JSON.stringify(theme)})`);
    await sleep(200);
    return { png: await shoot(page) };
  } catch (e) {
    return { note: `chdash: ${String(e.message).split("\n")[0]}`, failed: true };
  } finally {
    await page.close().catch(() => {});
  }
}

async function huneSide(ctx, state, theme, width) {
  if (!huneOk) return { note: `Hune is not reachable at ${HUNE}` };
  const page = await openPage(ctx, theme, width, `try { localStorage.setItem("hune.theme", ${JSON.stringify(theme)}); } catch (e) {}`);
  const side = {
    page,
    async open(path) {
      await page.goto(`${HUNE}${path}`);
      await settle(page, { timeout: 25000, panels: path.startsWith("/d/stream") });
      await sleep(300);
    },
  };
  try {
    await state.hune(side);
    return { png: await shoot(page) };
  } catch (e) {
    let png = null;
    try {
      png = await shoot(page);
    } catch {
      // no capture
    }
    return { png, note: `hune: ${String(e.message).split("\n")[0]}`, failed: true };
  } finally {
    if (state.cleanup) await state.cleanup(side).catch(() => {});
    await page.close().catch(() => {});
  }
}

async function compose(left, right, width, height, labels) {
  const script = `(async () => {
    const W = ${width}, H = ${height}, BAR = 28, GAP = 6;
    const load = (b64) => new Promise((res, rej) => { if (!b64) return res(null); const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.src = "data:image/png;base64," + b64; });
    const [a, b] = await Promise.all([load(${JSON.stringify(left.png || null)}), load(${JSON.stringify(right.png || null)})]);
    const c = document.createElement("canvas");
    c.width = W * 2 + GAP; c.height = H + BAR;
    const x = c.getContext("2d");
    x.fillStyle = "#808080"; x.fillRect(0, 0, c.width, c.height);
    x.font = "600 15px sans-serif"; x.textBaseline = "middle";
    const side = (img, note, ox, label) => {
      x.fillStyle = "#222"; x.fillRect(ox, 0, W, BAR); x.fillStyle = "#fff"; x.fillText(label, ox + 10, BAR / 2);
      if (img) x.drawImage(img, ox, BAR, W, H);
      else { x.fillStyle = "#e8e8e8"; x.fillRect(ox, BAR, W, H); x.fillStyle = "#444"; x.fillText(note || "no capture", ox + 20, BAR + 40); }
    };
    side(a, ${JSON.stringify(left.note || "")}, 0, ${JSON.stringify(labels[0])});
    side(b, ${JSON.stringify(right.note || "")}, W + GAP, ${JSON.stringify(labels[1])});
    return c.toDataURL("image/png").split(",")[1];
  })()`;
  return composer.eval(script);
}

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

try {
  for (const theme of THEMES) {
    for (const width of WIDTHS) {
      for (const state of states) {
        const ctx = await browser.newContext();
        try {
          const [c, v] = await Promise.all([chdashSide(ctx, state, theme, width), huneSide(ctx, state, theme, width)]);
          const name = `${state.id}-${theme}-${width}`;
          const png = await compose(c, v, width, HEIGHTS[width], ["ClickHouse Dash", "Hune"]);
          writeFileSync(join(OUT, `${name}.png`), Buffer.from(png, "base64"));
          if (c.png) writeFileSync(join(PARTS, `${name}-chdash.png`), Buffer.from(c.png, "base64"));
          if (v.png) writeFileSync(join(PARTS, `${name}-hune.png`), Buffer.from(v.png, "base64"));
          results.push({ state, theme, width, file: `${name}.png`, notes: [c.note, v.note, state.note].filter(Boolean) });
          console.log(`${c.failed || v.failed ? "warn" : "ok  "} ${name}${c.note || v.note ? "  " + [c.note, v.note].filter(Boolean).join(" | ") : ""}`);
        } finally {
          await ctx.dispose();
        }
      }
    }
  }
} finally {
  const body = results
    .map((r) => `<figure><a href="${encodeURI(r.file)}"><img loading="lazy" src="${encodeURI(r.file)}" alt="${esc(r.state.id)}"></a><figcaption>${esc(r.state.id)} · ${r.theme} · ${r.width} px · ${esc(r.state.text)}${r.notes.length ? `<br>${r.notes.map(esc).join(" / ")}` : ""}</figcaption></figure>`)
    .join("\n");
  writeFileSync(
    join(OUT, "index.html"),
    `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>ClickHouse Dash and Hune, chrome</title>
<style>body{margin:16px 24px;font:14px/1.45 system-ui,sans-serif}figure{margin:0 0 24px}img{width:100%;max-width:1900px;border:1px solid #888}figcaption{color:#555}</style></head><body>
<h1>ClickHouse Dash (left) and Hune (right): the chrome</h1><p>ClickHouse Dash side: ${esc(chdashMode)}. Hune: ${esc(HUNE)}. ${esc(new Date().toISOString())}</p>
${body}</body></html>`,
  );
  await browser.close();
  rmSync(profile, { recursive: true, force: true });
  if (staticServer) staticServer.close();
}
console.log(`done: ${results.length} sheets. ${join(OUT, "index.html")}`);
process.exit(0);
