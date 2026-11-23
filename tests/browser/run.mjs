#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser suite of 16.10, against the real server and a real ClickHouse.
// Use: node tests/browser/run.mjs [--only B01,B05] [--keep-going]
//
// The runner starts a ClickHouse container, three servers (editing, no editing, base path),
// one Chrome, and runs the cases. It stops all of them on every path.
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { sleep } from "./lib/cdp.mjs";
// The browser: Chrome by default (CDP). HUNE_BROWSER=firefox selects the WebDriver BiDi client (lib/bidi.mjs).
const BROWSER = process.env.HUNE_BROWSER || "chrome";
if (BROWSER !== "chrome" && BROWSER !== "firefox") throw new Error(`HUNE_BROWSER must be "chrome" or "firefox", got "${BROWSER}"`);
const { Browser } = await import(BROWSER === "firefox" ? "./lib/bidi.mjs" : "./lib/cdp.mjs");
import { procurauthTools, startAuthzen } from "./lib/authzen.mjs";
import { WORK, ROOT, api, SEED_FROM, SEED_TO, sh, startClickHouse, startServer, DOCKER, seedDocument } from "./lib/env.mjs";

const args = process.argv.slice(2);
const only = (() => {
  const i = args.indexOf("--only");
  return i >= 0 ? new Set(args[i + 1].split(",")) : null;
})();

const CASE_TIMEOUT_MS = 120000;

// ---------- cleanup on every path ----------
const cleanups = [];
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
function cleanupSync() {
  // Last resort at exit: the sync calls only.
  for (const f of syncCleanups) {
    try {
      f();
    } catch {
      // ignore
    }
  }
}
const syncCleanups = [];
process.on("exit", cleanupSync);
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(sig, async () => {
    console.error(`signal ${sig}: cleanup`);
    await cleanup();
    process.exit(130);
  });
}
process.on("uncaughtException", async (e) => {
  console.error("uncaught:", e);
  await cleanup();
  process.exit(1);
});
process.on("unhandledRejection", async (e) => {
  console.error("unhandled rejection:", e);
  await cleanup();
  process.exit(1);
});

/** A case throws this to report "not run" with a reason. */
class Skip extends Error {}

// ---------- case context ----------
class T {
  constructor(env, browser, def) {
    this.env = env;
    this.browser = browser;
    this.def = def;
    this.server = env.servers[def.server || "main"];
    this.servers = env.servers;
    this.ch = env.ch;
    this.api = api(this.server);
    this.contexts = [];
    this.pages = [];
    this.range = `from=${SEED_FROM}&to=${SEED_TO}`;
    this.notes = [];
    this.own = [];
    this.downloads = join(WORK, "downloads", def.id);
    rmSync(this.downloads, { recursive: true, force: true });
  }
  /** A server of the case alone (own volume). It stops at the end of the case. */
  async ownServer(name, opts = {}) {
    const s = await startServer(`own-${this.def.id}-${name}`, this.env.ch, opts);
    this.own.push(s);
    syncCleanups.push(() => s.killNow());
    return s;
  }
  /** A server in mode authzen with fake-pdp, of the case alone. It stops at the end of the case. */
  async authzen(opts = {}) {
    const tools = procurauthTools();
    if (!tools) throw new Skip("fake-pdp and access-token not found: build ProcurAuth and set PROCURAUTH_DIR");
    const a = await startAuthzen(this.env.ch, `own-${this.def.id}-authzen`, tools, opts);
    this.own.push(a);
    syncCleanups.push(() => a.killNow());
    return a;
  }
  /** The URL of a path under the base path of the server of the case. */
  url(path = "") {
    return this.server.url + path;
  }
  /** The URL of a dashboard page with the seed time range. */
  dash(uid, query = "") {
    const q = [this.range, query].filter(Boolean).join("&");
    return this.url(`d/${uid}?${q}`);
  }
  async context() {
    const c = await this.browser.newContext(this.downloads);
    this.contexts.push(c);
    return c;
  }
  /** A new page in a new context (own localStorage). */
  async page(opts = {}) {
    const c = opts.context || (await this.context());
    const p = await c.newPage(opts);
    this.pages.push(p);
    return p;
  }
  /** Open a page, load the URL, wait for the idle state. */
  async open(url, opts = {}) {
    const p = await this.page(opts);
    await p.goto(url);
    await p.idle();
    return p;
  }
  assert(cond, msg) {
    if (!cond) throw new Error(msg);
  }
  eq(actual, expected, msg = "") {
    const a = JSON.stringify(actual);
    const e = JSON.stringify(expected);
    if (a !== e) throw new Error(`${msg} expected ${e}, got ${a}`);
  }
  /** Cases that provoke errors on purpose allow a pattern of the console log. */
  allow(page, re) {
    page.allow.push(re);
  }
  note(text) {
    this.notes.push(text);
  }
  async finish(failed) {
    const problems = [];
    for (const p of this.pages) problems.push(...p.unexpectedProblems());
    if (failed) {
      mkdirSync(join(WORK, "artifacts"), { recursive: true });
      let i = 0;
      for (const p of this.pages) {
        try {
          await p.screenshot(join(WORK, "artifacts", `${this.def.id}-${i}.png`));
          writeFileSync(join(WORK, "artifacts", `${this.def.id}-${i}.console.txt`), p.problems.join("\n") + "\n");
        } catch {
          // page gone
        }
        i++;
      }
    }
    for (const p of this.pages) await p.close();
    for (const c of this.contexts) await c.dispose();
    for (const s of this.own) await s.stop();
    return problems;
  }
}

function withTimeout(promise, ms, what) {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, rej) => {
      timer = setTimeout(() => rej(new Error(`timeout of ${ms} ms in ${what}`)), ms);
    }),
  ]).finally(() => clearTimeout(timer));
}

async function loadCases() {
  const i = args.indexOf("--cases-dir");
  const dir = i >= 0 ? args[i + 1] : join(ROOT, "tests/browser/cases");
  const out = [];
  for (const f of readdirSync(dir).filter((x) => x.endsWith(".mjs")).sort()) {
    const mod = await import(pathToFileURL(join(dir, f)).href);
    for (const c of mod.cases) out.push(c);
  }
  out.sort((a, b) => a.id.localeCompare(b.id));
  return out;
}

async function main() {
  mkdirSync(WORK, { recursive: true });
  const defs = (await loadCases()).filter((c) => !only || only.has(c.id));
  const env = { servers: {} };

  // (a) ClickHouse
  console.error("starting ClickHouse ...");
  syncCleanups.push(() => sh(DOCKER, ["rm", "-f", "-v", `hune-test-browser-ch-${process.pid}`]));
  env.ch = await startClickHouse();
  cleanups.push(async () => env.ch.stop());

  // (b to d) three servers
  const wanted = new Set(defs.map((d) => d.server || "main"));
  wanted.add("main");
  const specs = {
    main: {},
    noedit: { editing: false },
    base: { basePath: "/dashboards/" },
  };
  for (const [name, opts] of Object.entries(specs)) {
    if (!wanted.has(name)) continue;
    console.error(`starting server ${name} ...`);
    const s = await startServer(name, env.ch, opts);
    env.servers[name] = s;
    syncCleanups.push(() => s.killNow());
    cleanups.push(async () => {
      const r = await s.stop();
      if (r && !(r.code === 0 || r.sig === "SIGTERM")) console.error(`server ${name} exit: ${JSON.stringify(r)}`);
      env.exits = env.exits || {};
      env.exits[name] = r;
    });
  }

  // Chrome
  const browser = await Browser.launch({ profileDir: join(WORK, "profile-1") });
  syncCleanups.push(() => browser.kill());
  cleanups.push(async () => browser.close());

  // (e) cases
  const results = [];
  for (const def of defs) {
    if (def.skip) {
      console.log(`skip ${def.id} ${def.name}: ${def.skip}`);
      results.push({ id: def.id, ok: true, skipped: true, ms: 0 });
      continue;
    }
    const t = new T(env, browser, def);
    let reason = null;
    let skipReason = null;
    const t0 = Date.now();
    try {
      await withTimeout(def.run(t), def.timeout || CASE_TIMEOUT_MS, def.id);
    } catch (e) {
      if (e instanceof Skip) skipReason = e.message;
      else reason = (e && e.message) || String(e);
    }
    let problems = [];
    try {
      problems = await t.finish(!!reason);
    } catch (e) {
      problems = [`finish: ${e.message}`];
    }
    if (skipReason) {
      console.log(`skip ${def.id} ${def.name}: ${skipReason}`);
      results.push({ id: def.id, ok: true, skipped: true, ms: 0 });
      continue;
    }
    if (!reason && problems.length) reason = "page problems: " + problems.slice(0, 4).join(" | ");
    if (reason && problems.length && !reason.startsWith("page problems")) reason += " [console: " + problems.slice(0, 2).join(" | ") + "]";
    const status = reason ? "not ok" : "ok";
    console.log(reason ? `not ok ${def.id} ${def.name}: ${reason}` : `ok ${def.id} ${def.name}`);
    for (const n of t.notes) console.log(`# ${def.id}: ${n}`);
    results.push({ id: def.id, ok: !reason, ms: Date.now() - t0 });
    void status;
  }

  const bad = results.filter((r) => !r.ok);
  const skipped = results.filter((r) => r.skipped).length;
  console.log(`# ${results.length - bad.length - skipped} of ${results.length} cases ok, ${skipped} not run${bad.length ? "; failed: " + bad.map((r) => r.id).join(", ") : ""}`);
  await cleanup();
  // Server exit: SIGTERM must stop it with code 0 (14).
  for (const [name, r] of Object.entries(env.exits || {})) {
    if (r && r.code !== 0) console.log(`# server ${name} stopped with ${JSON.stringify(r)}`);
  }
  process.exit(bad.length ? 1 : 0);
}

main().catch(async (e) => {
  console.error("runner error:", e.message);
  await cleanup();
  process.exit(1);
});

export { sleep, seedDocument };
