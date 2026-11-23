#!/usr/bin/env node
// SPDX-License-Identifier: MIT OR Apache-2.0
// Screenshot harness of the Hune front end.
//
// Use:  node tests/browser/shots.mjs --label before
//       node tests/browser/shots.mjs --label try --only dash-types-tour-full,list --themes light --widths 1280
//       node tests/browser/shots.mjs --list
//       node tests/browser/shots.mjs --reference
//
// The harness builds nothing: build the binary first (cargo build -p hune --locked). It copies the local demo volume
// (target/local/volume, without .hune/) to target/shots/volume-<label>, starts its own servers on free ports
// (editing on), uses the local ClickHouse container (never started or stopped here), takes the screenshots with
// Chrome (DevTools protocol), writes target/shots/<label>/<state>-<theme>-<width>.png and an index.html contact sheet,
// and stops every process on every path.
//
// Options:
//   --label <name>      the name of the run (default: run). The output is target/shots/<name>/.
//   --only <a,b,c>      the state ids to capture; the character * is a wildcard (for example dash-*).
//   --themes <list>     light, dark, auto-dark, auto-light. It replaces the themes of each state.
//   --widths <list>     for example 1280,1920,768,390. It replaces the widths of each state.
//   --full              all four themes and all four widths for every state.
//   --concurrency <n>   pages that work at the same time (default 4).
//   --list              print the states and stop.
//   --reference         capture the reference pages (static files) into target/shots/reference/ and stop.
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Browser } from "./lib/cdp.mjs";
import { SHOTS, client, copyVolume, startServer } from "./shots/infra.mjs";
import { HEIGHTS, THEMES, capture, openPage, settle } from "./shots/kit.mjs";
import { states } from "./shots/states.mjs";
import { writeIndex } from "./shots/sheet.mjs";
import { captureReferences } from "./shots/reference.mjs";

const args = process.argv.slice(2);
const opt = (name, fallback = null) => {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback;
};
const flag = (name) => args.includes(name);

// A wrong option stops the run before anything starts.
const KNOWN = new Set(["--label", "--only", "--themes", "--widths", "--full", "--concurrency", "--list", "--reference"]);
for (const a of args) {
  if (a.startsWith("--") && !KNOWN.has(a)) {
    console.error(`unknown option ${a}`);
    process.exit(2);
  }
}

const label = opt("--label", "run");
if (!/^[A-Za-z0-9._-]+$/.test(label)) {
  console.error("--label: use letters, digits, dot, underscore and dash");
  process.exit(2);
}
const concurrency = Number(opt("--concurrency", "4"));
if (!Number.isInteger(concurrency) || concurrency < 1 || concurrency > 12) {
  console.error("--concurrency: an integer from 1 to 12");
  process.exit(2);
}
const themesArg = opt("--themes") ? opt("--themes").split(",") : null;
const widthsArg = opt("--widths") ? opt("--widths").split(",").map(Number) : null;
for (const t of themesArg || []) if (!THEMES[t]) die(`--themes: unknown theme ${t} (${Object.keys(THEMES).join(", ")})`);
for (const w of widthsArg || []) if (!HEIGHTS[w]) die(`--widths: unknown width ${w} (${Object.keys(HEIGHTS).join(", ")})`);
const onlyArg = opt("--only") ? opt("--only").split(",") : null;

function die(message) {
  console.error(message);
  process.exit(2);
}

function wildcard(pattern) {
  return new RegExp("^" + pattern.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*") + "$");
}

if (flag("--list")) {
  for (const s of states) console.log(`${s.id.padEnd(30)} ${s.server.padEnd(6)} ${s.view.padEnd(8)} themes=${JSON.stringify(s.themes)} widths=${JSON.stringify(s.widths)}  ${s.text}`);
  process.exit(0);
}

const ALL_THEMES = ["light", "dark", "auto-dark", "auto-light"];
const CORE_THEMES = ["light", "dark"];
const ALL_WIDTHS = [1280, 1920, 768, 390];
const WIDE_WIDTHS = [1280, 1920];
const listOf = (spec, all, core) => (Array.isArray(spec) ? spec : spec === "all" ? all : core);

// ---- cleanup on every path ----
const cleanups = [];
const syncCleanups = [];
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
      // ignore
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

// ---- references ----
if (flag("--reference")) {
  const browser = await Browser.launch({ profileDir: join(SHOTS, "profile-ref"), width: 1280, height: 900 });
  syncCleanups.push(() => browser.kill());
  cleanups.push(() => browser.close());
  try {
    await captureReferences(browser, join(SHOTS, "reference"));
  } finally {
    await cleanup();
  }
  process.exit(0);
}

// ---- selection ----
const patterns = onlyArg ? onlyArg.map(wildcard) : null;
const selected = states.filter((s) => !patterns || patterns.some((re) => re.test(s.id)));
if (!selected.length) die(`no state matches --only ${onlyArg}`);
if (onlyArg) {
  for (const p of onlyArg) if (!states.some((s) => wildcard(p).test(s.id))) die(`--only: no state matches ${p}`);
}

const units = new Map(); // "theme|width" -> { theme, width, states }
for (const s of selected) {
  const themes = flag("--full") ? ALL_THEMES : themesArg || listOf(s.themes, ALL_THEMES, CORE_THEMES);
  const widths = flag("--full") ? ALL_WIDTHS : widthsArg || listOf(s.widths, ALL_WIDTHS, WIDE_WIDTHS);
  for (const theme of themes) {
    for (const width of widths) {
      const key = `${theme}|${width}`;
      if (!units.has(key)) units.set(key, { theme, width, states: [] });
      units.get(key).states.push(s);
    }
  }
}
const total = [...units.values()].reduce((n, u) => n + u.states.length, 0);

const outDir = join(SHOTS, label);
const t0 = Date.now();
mkdirSync(SHOTS, { recursive: true });
rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

// ---- servers ----
const runDir = join(SHOTS, `run-${label}`);
rmSync(runDir, { recursive: true, force: true });
mkdirSync(runDir, { recursive: true });
const servers = {};
async function boot(name, volume, needed) {
  if (!needed) return;
  const s = await startServer({ name, dir: runDir, volume });
  servers[name] = s;
  syncCleanups.push(() => s.killNow());
  cleanups.push(async () => {
    const r = await s.stop();
    if (r && !(r.code === 0 || r.sig === "SIGTERM")) console.error(`server ${name} exit: ${JSON.stringify(r)}`);
  });
  console.error(`server ${name}: ${s.url}`);
}
const uses = (name) => selected.some((s) => s.server === name);
const mainVolume = join(SHOTS, `volume-${label}`);
const workVolume = join(SHOTS, `volume-${label}-work`);
const emptyVolume = join(SHOTS, `volume-${label}-empty`);
cleanups.push(async () => {
  // The work and empty volumes and the Chrome profile are working data. The copy of the demo volume and the server logs stay.
  for (const v of [workVolume, emptyVolume, join(runDir, "profile")]) rmSync(v, { recursive: true, force: true });
});
copyVolume(mainVolume);
if (uses("work")) copyVolume(workVolume);
mkdirSync(emptyVolume, { recursive: true });
await boot("main", mainVolume, uses("main"));
await boot("work", workVolume, uses("work"));
await boot("empty", emptyVolume, uses("empty"));

// ---- browser ----
const browser = await Browser.launch({ profileDir: join(runDir, "profile"), width: 1280, height: 900 });
syncCleanups.push(() => browser.kill());
cleanups.push(() => browser.close());

let counter = 0;
const manifest = { label, started: new Date().toISOString(), shots: [], failures: [] };
let done = 0;

function makeContext(unit, page, context, state) {
  const server = servers[state.server];
  const api = client(server);
  const tag = `${unit.theme.replace("-", "")}${unit.width}`;
  let taken = 0;
  const c = {
    page,
    theme: unit.theme,
    width: unit.width,
    server,
    api,
    url: (path = "") => server.url + path,
    settle: (o) => settle(page, o),
    async open(url) {
      await page.goto(url);
      await settle(page);
    },
    async setViewportWidth(width) {
      page.base = { ...page.base, width };
      await page.send("Emulation.setDeviceMetricsOverride", { width, height: page.base.height, deviceScaleFactor: 1, mobile: false });
    },
    async grantClipboard() {
      await browser.send("Browser.grantPermissions", { origin: server.origin, permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"], browserContextId: context.id });
    },
    /** A copy of a demo dashboard under a new uid, on the work server (the state can change it). */
    async fresh(base) {
      const src = await api.dashboard(base);
      if (src.status !== 200) throw new Error(`read ${base}: ${src.status}`);
      const uid = `${base}-${tag}-${++counter}`.slice(0, 40);
      const doc = src.json.document;
      doc.uid = uid;
      const r = await api.create(src.json.folder, doc, "Copy for a screenshot");
      if (r.status !== 201) throw new Error(`create ${uid}: ${r.status} ${r.text.slice(0, 200)}`);
      return { uid, etag: r.etag };
    },
    async shot(suffix = "", o = {}) {
      const name = `${state.id}${suffix ? "-" + suffix : ""}-${unit.theme}-${unit.width}`;
      const file = join(outDir, `${name}.png`);
      await capture(page, file, { full: state.view === "full", ...o });
      taken++;
      manifest.shots.push({ state: state.id, suffix, theme: unit.theme, width: unit.width, file: `${name}.png`, problems: page.unexpectedProblems().slice(0, 3) });
    },
    get taken() {
      return taken;
    },
  };
  return c;
}

async function runUnit(unit) {
  const context = await browser.newContext();
  try {
    for (const state of unit.states) {
      const page = await openPage(context, unit);
      const c = makeContext(unit, page, context, state);
      try {
        await withTimeout(
          (async () => {
            await state.run(c);
            if (c.taken === 0) {
              await settle(page, { panels: false });
              await c.shot("");
            }
          })(),
          90000,
          state.id,
        );
      } catch (e) {
        const message = (e && e.message) || String(e);
        manifest.failures.push({ state: state.id, theme: unit.theme, width: unit.width, error: message });
        console.error(`FAIL ${state.id} ${unit.theme} ${unit.width}: ${message.split("\n")[0]}`);
        try {
          mkdirSync(join(outDir, "failed"), { recursive: true });
          await page.send("Emulation.setDeviceMetricsOverride", { width: unit.width, height: HEIGHTS[unit.width], deviceScaleFactor: 1, mobile: false });
          await capture(page, join(outDir, "failed", `${state.id}-${unit.theme}-${unit.width}.png`), {});
        } catch {
          // no screenshot of the failure
        }
      } finally {
        done++;
        if (done % 25 === 0 || done === total) console.error(`${done}/${total} captures, ${Math.round((Date.now() - t0) / 1000)} s`);
        await page.close().catch(() => {});
      }
    }
  } finally {
    await context.dispose().catch(() => {});
  }
}

function withTimeout(promise, ms, what) {
  let timer;
  return Promise.race([promise, new Promise((_, rej) => (timer = setTimeout(() => rej(new Error(`timeout of ${ms} ms in ${what}`)), ms)))]).finally(() => clearTimeout(timer));
}

try {
  console.error(`${selected.length} states, ${units.size} theme and width pairs, ${total} captures`);
  const queue = [...units.values()];
  await Promise.all(
    Array.from({ length: Math.min(concurrency, queue.length) }, async () => {
      for (;;) {
        const unit = queue.shift();
        if (!unit) return;
        await runUnit(unit);
      }
    }),
  );
  manifest.seconds = Math.round((Date.now() - t0) / 1000);
  manifest.shots.sort((a, b) => a.file.localeCompare(b.file));
  writeFileSync(join(outDir, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  writeIndex(outDir, manifest, states);
  console.log(`label ${label}: ${manifest.shots.length} screenshots, ${manifest.failures.length} failures, ${manifest.seconds} s`);
  console.log(`contact sheet: ${join(outDir, "index.html")}`);
  for (const f of manifest.failures) console.log(`failed: ${f.state} ${f.theme} ${f.width}: ${f.error.split("\n")[0]}`);
} finally {
  await cleanup();
}
process.exit(manifest.failures.length ? 1 : 0);
