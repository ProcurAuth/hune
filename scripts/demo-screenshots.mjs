#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Screenshots of the Hune demo for the documentation.
//
// Use:  node scripts/demo-screenshots.mjs
//       node scripts/demo-screenshots.mjs --only hero,editor
//       node scripts/demo-screenshots.mjs --list
//
// The script builds nothing. It needs:
//   - Node 22. It has no npm dependency.
//   - Google Chrome at /usr/bin/google-chrome (set CHROME for another path).
//   - Docker and the local image clickhouse/clickhouse-server:26.8.34.13. The script pulls no image.
//   - The binary `hune` (set --hune or HUNE_BIN; the default is the release binary under dist/ or target/release/).
//   - Python 3 with Pillow. The script uses it to write the WebP files.
//   - For the two pictures of the mode `authzen`: the ProcurAuth tools `fake-pdp` and `access-token`, and `openssl`.
//     Set PROCURAUTH_DIR to a folder that has target/release/fake-pdp and target/release/access-token.
//     Without the tools, the script skips these two pictures.
//
// What the script does:
//   1. It starts one ClickHouse container on a free port of 127.0.0.1. The name and the volume start with `hune-pub-`.
//   2. It loads examples/demo/init.sql, and it waits until the data are in.
//   3. It copies examples/demo/dashboards to a work folder, and it derives the configuration from examples/demo/hune.hcl.
//   4. It starts the Hune servers on free ports of 127.0.0.1.
//   5. It drives Chrome with the DevTools protocol, and it writes one WebP file for each picture.
//   6. It removes the container, the volume, the servers, Chrome and the work folder. This also happens after an error.
//
// Options:
//   --only <a,b,c>   Make only these pictures (the names without .webp).
//   --list           Print the names of the pictures and stop.
//   --out <dir>      The output folder (default: docs/assets/img).
//   --hune <path>    The binary of Hune.
//   --ch-url <url>   Use a ClickHouse that already has the demo data. The script starts and stops no container.
//   --keep-png       Keep the PNG files in the work folder (it prints the path).
import { spawn, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Browser } from "../tests/browser/lib/cdp.mjs";
import { hold, openPage, settle, sleep } from "../tests/browser/shots/kit.mjs";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DEMO = join(REPO, "examples/demo");
const IMAGE = "clickhouse/clickhouse-server:26.8.34.13";
const DOCKER = process.env.DOCKER || "docker";
const CH_ADMIN_USER = "demo_admin";
const CH_ADMIN_PASSWORD = "demo-admin-password"; // A public demo value. It protects nothing.
const CH_READER_PASSWORD = "hune-demo-password"; // The password of `hune_reader` in init.sql. A public demo value.
const MAX_BYTES = 118 * 1000; // The limit of one picture is 120 KB. This keeps a small margin.
const WIDE = { width: 1440, height: 900 };

// ---- options ----
const args = process.argv.slice(2);
const KNOWN = new Set(["--only", "--list", "--out", "--hune", "--ch-url", "--keep-png"]);
const opt = (name, fallback = null) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
for (const a of args) {
  if (a.startsWith("--") && !KNOWN.has(a)) die(`unknown option ${a}`);
}
function die(message) {
  console.error(message);
  process.exit(2);
}
const OUT = resolve(opt("--out", join(REPO, "docs/assets/img")));
const CH_URL_GIVEN = opt("--ch-url");
const KEEP_PNG = args.includes("--keep-png");
const HUNE = opt("--hune") || process.env.HUNE_BIN || [join(REPO, "dist/hune_0.3.0_linux_amd64/hune"), join(REPO, "target/release/hune")].find(existsSync);

// ---- cleanup on each path ----
const cleanups = []; // Async functions. They run in the reverse order.
const syncCleanups = []; // Sync functions. They run at process exit.
let cleaned = false;
async function cleanup() {
  if (cleaned) return;
  cleaned = true;
  for (const f of cleanups.reverse()) {
    try {
      await f();
    } catch (e) {
      console.error("cleanup error:", e.message);
    }
  }
}
process.on("exit", () => {
  for (const f of syncCleanups) {
    try {
      f();
    } catch {
      // Ignore. The process stops.
    }
  }
});
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(sig, async () => {
    console.error(`signal ${sig}: cleanup`);
    await cleanup();
    process.exit(130);
  });
}

// ---- small helpers ----
function freePort() {
  return new Promise((res, rej) => {
    const s = createServer();
    s.once("error", rej);
    s.listen(0, "127.0.0.1", () => {
      const { port } = s.address();
      s.close(() => res(port));
    });
  });
}
const sh = (cmd, a, o = {}) => spawnSync(cmd, a, { encoding: "utf8", ...o });
const T = (id) => `[data-testid="${id}"]`;

// ---- the list of pictures ----
// Each picture has a name, a text and a function run(c). The function calls c.shot(name, options).
const pictures = [];
const picture = (name, text, run, needs = null) => pictures.push({ name, text, run, needs });

/** The pictures of the dashboard "Service overview", "Traffic" and "Latency" in both themes. */
picture("hero", "Service overview, light theme", async (c) => {
  const p = await c.open("main", "d/service-overview", "light");
  await c.shot("hero", p, { full: true });
});
picture("dashboard-dark", "Service overview, dark theme", async (c) => {
  const p = await c.open("main", "d/service-overview", "dark");
  await c.shot("dashboard-dark", p, { full: true });
});
picture("charts-light", "Traffic, light theme", async (c) => {
  const p = await c.open("main", "d/traffic", "light");
  await c.shot("charts-light", p, { crop: await bottomOf(p, '[data-testid^="panel-"]:not([data-testid*="-"]), [data-testid="panel-5"]') });
});
picture("charts-dark", "Traffic, dark theme", async (c) => {
  const p = await c.open("main", "d/traffic", "dark");
  await c.shot("charts-dark", p, { crop: await bottomOf(p, '[data-testid^="panel-"]:not([data-testid*="-"]), [data-testid="panel-5"]') });
});
picture("latency", "Latency: gauge, heatmap, histogram, state timeline and logs", async (c) => {
  const p = await c.open("main", "d/latency", "light");
  await c.shot("latency", p, { full: true });
});
picture("logs-timeline", "State timeline and logs panel (crop of Latency, dark theme)", async (c) => {
  const p = await c.open("main", "d/latency", "dark");
  const box = await boxOf(p, ["panel-4", "panel-5"]);
  await c.shot("logs-timeline", p, { rect: { x: 0, y: box.y - 12, width: WIDE.width, height: Math.min(box.h + 24, 640) } });
});

/** The text panels (Markdown) of the start dashboard. */
picture("welcome", "The start dashboard with two text panels", async (c) => {
  const p = await c.open("main", "d/welcome", "light");
  const box = await boxOf(p, ["panel-1", "panel-2"]);
  await c.shot("welcome", p, { rect: { x: 0, y: 0, width: WIDE.width, height: box.y + box.h + 16 } });
});

/** The table panel. */
picture("table", "Table panel with cell colours, links, a gauge cell and a footer", async (c) => {
  const p = await c.open("main", "d/servers", "light");
  const box = await boxOf(p, ["panel-1"]);
  await c.shot("table", p, { rect: { x: 0, y: 0, width: WIDE.width, height: box.y + box.h + 16 } });
});

/** The list page. */
picture("list", "The dashboard list with folders", async (c) => {
  // The page centres the cards in the window when they fit. A low window makes the cards start at the top of the page.
  const p = await c.open("main", "", "light", { panels: false, height: 360 });
  await p.waitSel(T("list-new-menu"));
  await p.waitSel('[data-testid^="row-"]');
  const bottom = await p.eval(`Math.ceil(Math.max(...[...document.querySelectorAll('[data-testid^="row-"]')].map((e) => e.getBoundingClientRect().bottom + scrollY)))`);
  await c.shot("list", p, { rect: { x: 0, y: 0, width: WIDE.width, height: bottom + 24 } });
});

/** The editor. */
picture("editor", "The panel editor: query, preview and options", async (c) => {
  const p = await c.open("work", "d/service-overview", "light");
  await enterEdit(p);
  await openPanelEditor(p, 5);
  await p.click(T("tab-query"));
  await settle(p);
  await c.shot("editor", p, {});
});

/** The history of versions and the diff of two versions. */
picture("history", "The history of versions", async (c) => {
  await c.makeVersions("work", "service-overview");
  const p = await c.open("work", "d/service-overview", "light");
  await moreItem(p, "history-button");
  await p.waitSel(T("history-list"));
  await p.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length >= 3`, { what: "3 versions" });
  await p.click('[data-testid="history-list"] tr[data-version="1"] input[type=checkbox]');
  await p.click('[data-testid="history-list"] tr[data-version="3"] input[type=checkbox]');
  await settle(p, { panels: false });
  await c.shot("history", p, { rect: { x: 0, y: 0, width: WIDE.width, height: 320 } });
});
picture("diff", "The diff of two versions", async (c) => {
  await c.makeVersions("work", "service-overview");
  const p = await c.open("work", "d/service-overview", "light");
  await moreItem(p, "history-button");
  await p.waitSel(T("history-list"));
  await p.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length >= 3`, { what: "3 versions" });
  await p.click('[data-testid="history-list"] tr[data-version="1"] input[type=checkbox]');
  await p.click('[data-testid="history-list"] tr[data-version="3"] input[type=checkbox]');
  await p.click(T("history-compare"));
  await p.waitSel(T("diff-view"));
  await settle(p, { panels: false });
  await c.shot("diff", p, { full: true, maxHeight: 900 });
});

/** The query inspector of a panel. */
picture("inspect", "Inspect: the SQL that Hune sends", async (c) => {
  const p = await c.open("main", "d/service-overview", "light");
  await p.click(T("panel-7-menu"));
  await p.click(T("panel-7-inspect"));
  await p.waitSel(T("inspect-7"));
  await p.waitSel(T("inspect-7-sql"));
  await p.eval("window.scrollTo(0, 0)");
  await settle(p, { panels: false });
  await c.shot("inspect", p, {});
});

/** The variables and the time picker. */
picture("variables", "Variable controls and the open time picker", async (c) => {
  const p = await c.open("main", "d/service-overview", "light");
  await p.click(T("time-picker-button"));
  await sleep(400);
  await c.shot("variables", p, { rect: { x: 0, y: 0, width: WIDE.width, height: 470 } });
});

/** The loader: one panel waits for its answer. */
picture("loader", "One panel that loads (the three pulsing dots)", async (c) => {
  const page = await c.page("main", "light", { motion: true });
  // Hold the query of the panel 5. The answer of the other panels comes at once.
  const held = await hold(page, (r) => /\/panels\/5\/query$/.test(r.url));
  await page.goto(c.url("main", "d/service-overview"));
  await page.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-state="ready"]').length >= 7`, { timeout: 20000, what: "7 panels ready" });
  await page.waitFor(`document.querySelector('[data-testid="panel-5"]')?.dataset.state === "loading"`, { timeout: 5000, what: "panel 5 loading" });
  await sleep(300);
  await c.shot("loader", page, { rect: { x: 0, y: 0, width: WIDE.width, height: 560 } });
  await held.release();
});

/** The mode `authzen`. */
picture(
  "access-refused",
  "Mode authzen: a person with no right on the dashboard",
  async (c) => {
    const p = await c.openAs("visitor", "d/latency", "light");
    await c.shot("access-refused", p, { rect: { x: 0, y: 0, width: WIDE.width, height: 360 } });
  },
  "authzen",
);
picture(
  "access-readonly",
  "Mode authzen: a person who can read and cannot write (no edit control)",
  async (c) => {
    const p = await c.openAs("viewer", "d/service-overview", "light");
    await c.shot("access-readonly", p, { rect: { x: 0, y: 0, width: WIDE.width, height: 252 } });
  },
  "authzen",
);

/** A phone. */
picture("mobile", "Service overview at 390 px", async (c) => {
  const p = await c.open("main", "d/service-overview", "light", { width: 390, height: 844 });
  await c.shot("mobile", p, { rect: { x: 0, y: 0, width: 390, height: 1500 } });
});

if (args.includes("--list")) {
  for (const p of pictures) console.log(`${p.name.padEnd(18)} ${p.text}`);
  process.exit(0);
}
const onlyArg = opt("--only") ? opt("--only").split(",") : null;
for (const n of onlyArg || []) if (!pictures.some((p) => p.name === n)) die(`--only: no picture ${n}`);
const selected = pictures.filter((p) => !onlyArg || onlyArg.includes(p.name));

// ---- page helpers ----
async function enterEdit(p) {
  await p.click(T("edit-toggle"));
  await p.waitSel(T("edit-bar"));
  await p.waitSel('.edit-grid [data-testid^="panel-"][data-state]');
  await settle(p);
}
async function openPanelEditor(p, id) {
  await p.click(T(`panel-${id}-menu`));
  await p.click(T(`panel-${id}-edit`));
  await p.waitSel(T("panel-editor"));
  await settle(p);
}
async function moreItem(p, id) {
  await p.click(T("dashboard-menu"));
  await p.waitSel(`.menu ${T(id)}`);
  await p.click(`.menu ${T(id)}`);
}
/** The box of the union of the elements with these test ids, in document pixels. */
async function boxOf(p, ids) {
  return p.eval(`(() => {
    let t = Infinity, b = 0;
    for (const id of ${JSON.stringify(ids)}) {
      const e = document.querySelector('[data-testid="' + id + '"]');
      if (!e) continue;
      const r = e.getBoundingClientRect();
      t = Math.min(t, r.top + scrollY);
      b = Math.max(b, r.bottom + scrollY);
    }
    return { y: Math.floor(t), h: Math.ceil(b - t) };
  })()`);
}
/** The bottom edge of the last panel: the crop height of a page that has empty space under the panels. */
async function bottomOf(p) {
  const bottom = await p.eval(`Math.ceil(Math.max(...[...document.querySelectorAll('[data-testid^="panel-"][data-state]')].filter((e) => /^panel-\\d+$/.test(e.dataset.testid)).map((e) => e.getBoundingClientRect().bottom + scrollY)))`);
  return bottom + 16;
}

// ---- encode: PNG to WebP, with a size limit ----
const ENCODE = `
import sys
from PIL import Image
src, dst, limit = sys.argv[1], sys.argv[2], int(sys.argv[3])
img = Image.open(src).convert("RGB")
for q in (92, 88, 84, 80, 76, 72, 68, 64, 60, 55, 50, 45, 40):
    img.save(dst, "WEBP", quality=q, method=6)
    import os
    size = os.path.getsize(dst)
    if size <= limit:
        print(size, q)
        sys.exit(0)
print(size, -1)
sys.exit(3)
`;
function encode(png, webp) {
  const r = sh("python3", ["-c", ENCODE, png, webp, String(MAX_BYTES)], { cwd: tmpdir() });
  if (r.status !== 0) throw new Error(`WebP encode failed for ${webp}: ${r.stdout} ${r.stderr}`);
  const [size, quality] = r.stdout.trim().split(" ").map(Number);
  return { size, quality };
}

// ---- main ----
async function main() {
  if (!HUNE || !existsSync(HUNE)) die(`the binary of Hune is missing: ${HUNE}. Use --hune <path>.`);
  if (!existsSync(join(DEMO, "init.sql"))) die(`missing ${join(DEMO, "init.sql")}`);
  const work = mkdtempSync(join(tmpdir(), "hune-pub-shots-"));
  syncCleanups.push(() => rmSync(work, { recursive: true, force: true }));
  mkdirSync(OUT, { recursive: true });
  const pngDir = join(work, "png");
  mkdirSync(pngDir);

  // 1. ClickHouse.
  let chUrl = CH_URL_GIVEN;
  if (!chUrl) {
    const port = await freePort();
    const name = `hune-pub-ch-${process.pid}`;
    const volume = `hune-pub-ch-data-${process.pid}`;
    const drop = () => {
      sh(DOCKER, ["rm", "-f", "-v", name]);
      sh(DOCKER, ["volume", "rm", "-f", volume]);
    };
    syncCleanups.push(drop);
    drop();
    const run = sh(DOCKER, [
      "run", "-d", "--name", name,
      "-p", `127.0.0.1:${port}:8123`,
      "-v", `${volume}:/var/lib/clickhouse`,
      "-e", `CLICKHOUSE_USER=${CH_ADMIN_USER}`,
      "-e", `CLICKHOUSE_PASSWORD=${CH_ADMIN_PASSWORD}`,
      "-e", "CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1",
      "-v", `${join(DEMO, "init.sql")}:/docker-entrypoint-initdb.d/init.sql:ro`,
      IMAGE,
    ]);
    if (run.status !== 0) throw new Error("docker run failed: " + run.stderr);
    chUrl = `http://127.0.0.1:${port}`;
    let ready = false;
    for (let i = 0; i < 120 && !ready; i++) {
      try {
        const r = await fetch(chUrl + "/", {
          method: "POST",
          body: "SELECT count() FROM demo.logs",
          headers: { Authorization: "Basic " + Buffer.from(`${CH_ADMIN_USER}:${CH_ADMIN_PASSWORD}`).toString("base64") },
        });
        ready = (await r.text()).trim() === "4000";
      } catch {
        // The server does not listen yet.
      }
      if (!ready) await sleep(1000);
    }
    if (!ready) throw new Error("ClickHouse did not load the demo data\n" + sh(DOCKER, ["logs", name]).stdout.slice(-1500));
    console.error(`clickhouse: ${chUrl}`);
  }

  // 2. The Hune servers. Each one has its own copy of the dashboards.
  const baseConfig = readFileSync(join(DEMO, "hune.hcl"), "utf8");
  const servers = {};
  async function boot(name, { accessBlock = null } = {}) {
    const dir = join(work, name);
    const volume = join(dir, "dashboards");
    mkdirSync(dir, { recursive: true });
    cpSync(join(DEMO, "dashboards"), volume, { recursive: true });
    const port = await freePort();
    const opsPort = await freePort();
    let cfg = baseConfig;
    const swap = (from, to) => {
      if (!cfg.includes(from)) throw new Error(`hune.hcl has no text ${JSON.stringify(from)}`);
      cfg = cfg.replace(from, to);
    };
    swap('listen                  = "127.0.0.1:8080"', `listen                  = "127.0.0.1:${port}"`);
    swap('ops_listen              = "127.0.0.1:9090"', `ops_listen              = "127.0.0.1:${opsPort}"`);
    swap('path            = "/tmp/hune-demo/dashboards"', `path            = "${volume}"`);
    swap('url                        = "http://127.0.0.1:8123"', `url                        = "${chUrl}"`);
    if (accessBlock) {
      swap('access {\n  mode = "open"\n}\n', accessBlock);
      // The mode authzen needs a limit for each subject on each source.
      swap("max_concurrent             = 16", "max_concurrent             = 16\n  max_concurrent_per_subject = 6");
    }
    const configPath = join(dir, "hune.hcl");
    writeFileSync(configPath, cfg);
    const env = { HUNE_DEMO_CH_PASSWORD: CH_READER_PASSWORD };
    const check = sh(HUNE, ["check-config", "-c", configPath], { env });
    if (check.status !== 0) throw new Error(`check-config failed for ${name}: ${check.stdout}${check.stderr}`);
    const logPath = join(dir, "hune.log");
    const fd = openSync(logPath, "a");
    const child = spawn(HUNE, ["serve", "-c", configPath], { stdio: ["ignore", fd, fd], env, detached: true });
    let exited = null;
    child.on("exit", (code, sig) => {
      exited = { code, sig };
    });
    const kill = () => {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {
        // The process is gone.
      }
    };
    syncCleanups.push(kill);
    cleanups.push(async () => {
      if (exited) return;
      child.kill("SIGTERM");
      for (let i = 0; i < 50 && !exited; i++) await sleep(100);
      kill();
    });
    const s = { name, volume, origin: `http://127.0.0.1:${port}`, ops: `http://127.0.0.1:${opsPort}`, log: () => readFileSync(logPath, "utf8") };
    for (let i = 0; i < 150; i++) {
      if (exited) throw new Error(`server ${name} stopped at start: ${JSON.stringify(exited)}\n${s.log().slice(-1500)}`);
      try {
        const r = await fetch(s.ops + "/readyz");
        if (r.status === 200) {
          servers[name] = s;
          return s;
        }
      } catch {
        // The server does not listen yet.
      }
      await sleep(100);
    }
    throw new Error(`server ${name}: /readyz did not give 200\n${s.log().slice(-1500)}`);
  }
  const need = (n) => selected.some((p) => p.needs === n);
  const useWork = selected.some((p) => ["editor", "history", "diff"].includes(p.name));
  await boot("main");
  if (useWork) await boot("work");
  const authzen = need("authzen") ? await bootAuthzen(work, boot) : null;
  if (need("authzen") && !authzen) console.error("authzen: the ProcurAuth tools are missing. The pictures of this mode are skipped.");

  // 3. Chrome.
  const browser = await Browser.launch({ profileDir: join(work, "profile"), width: WIDE.width, height: WIDE.height });
  syncCleanups.push(() => browser.kill());
  cleanups.push(() => browser.close());

  // 4. The context of the pictures.
  const results = [];
  const c = {
    url: (server, path) => (server === "authzen" ? authzen.server.origin : servers[server].origin) + "/" + path,
    /** A new page with a theme and a size. The option `motion` keeps the animations. */
    async page(server, theme, { width = WIDE.width, height = WIDE.height, motion = false } = {}) {
      const ctx = await browser.newContext();
      const page = await openPage(ctx, { theme, width: 1280 });
      await page.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
      page.base = { width, height };
      await page.send("Emulation.setScrollbarsHidden", { hidden: true });
      await page.send("Emulation.setEmulatedMedia", {
        features: [
          { name: "prefers-color-scheme", value: theme },
          { name: "prefers-reduced-motion", value: motion ? "no-preference" : "reduce" },
        ],
      });
      return page;
    },
    async open(server, path, theme, { panels = true, width = WIDE.width, height = WIDE.height } = {}) {
      const page = await c.page(server, theme, { width, height });
      await page.goto(c.url(server, path));
      await settle(page, { panels });
      await this.assertClean(page, path);
      return page;
    },
    /** Open a page in the mode authzen as a subject. The page sends the identity token. */
    async openAs(subject, path, theme) {
      const page = await c.page("authzen", theme);
      await page.setHeaders({ [authzen.header]: authzen.token(subject) });
      // The page may be a refusal page. It has no main area, so `goto` of the client does not fit.
      await page.send("Page.navigate", { url: c.url("authzen", path) });
      await page.waitFor(`document.readyState === "complete"`, { what: "page load" });
      await settle(page, { panels: false }).catch(() => {});
      await sleep(600);
      if (process.env.DEMO_SHOTS_DEBUG) console.error(await page.eval(`document.body.innerText.slice(0, 600)`));
      return page;
    },
    /** A panel with an error is a defect of the demo. The script stops. */
    async assertClean(page, what) {
      const bad = await page.eval(`[...document.querySelectorAll('[data-testid^="panel-"][data-state="error"]')].map((e) => e.dataset.testid + ": " + e.innerText.slice(0, 160).replace(/\\n/g, " "))`);
      if (bad.length) throw new Error(`${what}: panel error: ${bad.join("; ")}`);
      const problems = page.unexpectedProblems();
      if (problems.length) throw new Error(`${what}: console problems: ${problems.join("; ")}`);
    },
    /** Save two changes of a dashboard through the API, so that the history has three versions. */
    async makeVersions(server, uid) {
      const s = servers[server];
      if (s.versions?.has(uid)) return;
      const call = async (method, path, body, headers = {}) => {
        const r = await fetch(`${s.origin}/${path}`, {
          method,
          headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...headers },
          body: body ? JSON.stringify(body) : undefined,
        });
        return { status: r.status, json: await r.json().catch(() => null), etag: (r.headers.get("etag") || "").replaceAll('"', "") };
      };
      const edits = [
        ["Raise the threshold of the error rate", (d) => (d.sections[0].panels[1].fields.defaults.thresholds.steps[1].value = 0.5)],
        ["Rename the latency chart", (d) => (d.sections[0].panels[4].title = "p95 latency by service (15 minutes)")],
      ];
      for (const [message, edit] of edits) {
        const cur = await call("GET", `api/dashboards/${uid}`);
        const doc = cur.json.document;
        edit(doc);
        const r = await call("PUT", `api/dashboards/${uid}`, { message, document: doc }, { "Idempotency-Key": crypto.randomUUID(), "If-Match": `"${cur.etag}"` });
        if (r.status !== 200) throw new Error(`save through the API: ${r.status} ${JSON.stringify(r.json)}`);
      }
      (s.versions ||= new Set()).add(uid);
    },
    /**
     * Capture a picture. Options: `full` (the whole page), `rect` (a clip in CSS pixels) or `crop` (the page from the top, with this height).
     * The viewport grows when the picture is taller than the window, and it goes back afterwards.
     */
    async shot(name, page, { full = false, rect = null, crop = null, maxHeight = 2400 } = {}) {
      const { width, height } = page.base;
      let clip = rect;
      if (crop) clip = { x: 0, y: 0, width, height: crop };
      if (full) {
        // The content scrolls in #app-main under the header: the page is as high as the header and the content together.
        const h = await page.eval(`Math.max(document.documentElement.scrollHeight, document.getElementById("app-header").offsetHeight + document.getElementById("app-main").scrollHeight)`);
        clip = { x: 0, y: 0, width, height: Math.min(h, maxHeight) };
      }
      let grown = false;
      if (clip && clip.y + clip.height > height) {
        await page.send("Emulation.setDeviceMetricsOverride", { width, height: Math.min(maxHeight, Math.ceil(clip.y + clip.height)), deviceScaleFactor: 1, mobile: false });
        grown = true;
        await settle(page, { panels: false });
      }
      const r = await page.send("Page.captureScreenshot", clip ? { format: "png", clip: { ...clip, scale: 1 } } : { format: "png" });
      if (grown) await page.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
      const png = join(pngDir, `${name}.png`);
      writeFileSync(png, Buffer.from(r.data, "base64"));
      const webp = join(OUT, `${name}.webp`);
      const { size, quality } = encode(png, webp);
      results.push({ name, size, quality });
      console.error(`ok ${name}.webp ${(size / 1024).toFixed(1)} KB (quality ${quality})`);
    },
  };

  for (const pic of selected) {
    if (pic.needs === "authzen" && !authzen) {
      console.error(`skip ${pic.name}: no ProcurAuth tools`);
      continue;
    }
    console.error(`picture ${pic.name}: ${pic.text}`);
    await pic.run(c);
  }
  const total = results.reduce((n, r) => n + r.size, 0);
  console.error(`${results.length} pictures, ${(total / 1024).toFixed(0)} KB in total, in ${OUT}`);
  if (KEEP_PNG) {
    cpSync(pngDir, join(tmpdir(), `hune-pub-png-${process.pid}`), { recursive: true });
    console.error(`PNG files: ${join(tmpdir(), `hune-pub-png-${process.pid}`)}`);
  }
}

// ---- the mode authzen: a Hune server, a fake PDP and the tokens ----
const SERVICE = "hune-demo";
const HEADER = "X-Identity-Token";
async function bootAuthzen(work, boot) {
  const dir = process.env.PROCURAUTH_DIR || join(REPO, "target/procurauth");
  const fakePdp = join(dir, "target/release/fake-pdp");
  const accessToken = join(dir, "target/release/access-token");
  if (!existsSync(fakePdp) || !existsSync(accessToken) || sh("openssl", ["version"]).status !== 0) return null;
  const adir = join(work, "authzen");
  mkdirSync(adir, { recursive: true });
  // A key pair for this run. The edge of the demo signs the identity tokens with it.
  const priv = join(adir, "edge.pem");
  const pub = join(adir, "edge.pub.pem");
  if (sh("openssl", ["genpkey", "-algorithm", "ed25519", "-out", priv]).status !== 0) throw new Error("openssl genpkey failed");
  if (sh("openssl", ["pkey", "-in", priv, "-pubout", "-out", pub]).status !== 0) throw new Error("openssl pkey failed");
  const cred = join(adir, "pdp-credential.txt");
  writeFileSync(cred, "demo-pdp-credential\n"); // A throwaway value for this run.
  const pdpPort = await freePort();
  const accessBlock = `access {
  mode = "authzen"

  identity {
    header           = "${HEADER}"
    issuer           = "https://edge.example.com"
    audience         = "${SERVICE}"
    public_keys_file = "${pub}"
    max_age          = "5m"
    clock_skew       = "30s"
  }

  pdp {
    url             = "http://127.0.0.1:${pdpPort}"
    token_file      = "${cred}"
    plain_http      = true
    timeout         = "1s"
    cache_ttl       = "0s"
    max_connections = 4
  }
}
`;
  const server = await boot("authzen", { accessBlock });
  // The action document of Hune is the document of the service in the policy.
  const res = await fetch(server.origin + "/.well-known/procurauth/actions");
  if (res.status !== 200) throw new Error(`action document: ${res.status}`);
  const actionDocument = await res.json();
  const grants = (spec) => Object.entries(spec).flatMap(([action, paths]) => paths.map((p) => ({ action, resource: p ? `service:${SERVICE}/${p}` : `service:${SERVICE}` })));
  const policy = {
    services: [{ id: SERVICE, document: actionDocument }],
    subjects: [
      // The visitor can read the folder "start" only.
      { id: "visitor", grants: grants({ "dashboard.read": ["folder:start"] }) },
      // The viewer can read all dashboards and run their queries. The viewer cannot write.
      { id: "viewer", grants: grants({ "dashboard.read": [""], "source.query": [""] }) },
    ],
  };
  const dataFile = join(adir, "policy.json");
  writeFileSync(dataFile, JSON.stringify(policy, null, 2));
  const fd = openSync(join(adir, "fake-pdp.log"), "a");
  const child = spawn(fakePdp, ["--listen", `127.0.0.1:${pdpPort}`, "--credential-file", cred, dataFile], { stdio: ["ignore", fd, fd], env: {}, detached: true });
  const kill = () => {
    try {
      process.kill(-child.pid, "SIGKILL");
    } catch {
      // The process is gone.
    }
  };
  syncCleanups.push(kill);
  cleanups.push(async () => kill());
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(`http://127.0.0.1:${pdpPort}/.well-known/authzen-configuration`)).status === 200) break;
    } catch {
      // The PDP does not listen yet.
    }
    await sleep(50);
    if (i === 99) throw new Error("fake-pdp did not answer");
  }
  const token = (sub) => {
    const now = Math.floor(Date.now() / 1000);
    const claims = join(adir, `claims-${sub}.json`);
    writeFileSync(claims, JSON.stringify({ iss: "https://edge.example.com", sub, aud: SERVICE, iat: now, exp: now + 240 }));
    const out = sh(accessToken, ["make", "--key", priv, claims]);
    if (out.status !== 0) throw new Error("access-token make failed: " + out.stdout + out.stderr);
    return out.stdout.trim();
  };
  return { server, token, header: HEADER };
}

let failed = false;
try {
  await main();
} catch (e) {
  failed = true;
  console.error(e.stack || e.message);
} finally {
  await cleanup();
}
process.exit(failed ? 1 : 0);
