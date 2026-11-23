// Checks of the copied chart engine (9.24): the lock file, the hashes, the imports and the shim exports.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const web = join(here, "..", "..", "web");
const dirs = ["js/vendor/chdash", "css/vendor/chdash"];
const lock = JSON.parse(readFileSync(join(web, "js/vendor/chdash/chdash.lock.json"), "utf8"));
// The upstream checkout: the test tooling variable CHDASH_REPO, else target/clickhouse-dash. The test skips when it is absent.
const UPSTREAM = process.env.CHDASH_REPO || join(here, "..", "..", "target", "clickhouse-dash");

const sha = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
}

const onDisk = dirs.flatMap((d) => walk(join(web, d)))
  .map((p) => relative(web, p))
  .filter((p) => p !== "js/vendor/chdash/chdash.lock.json");
const byPath = new Map(lock.files.map((f) => [f.path, f]));

test("lock header", () => {
  assert.equal(lock.tag, "v1.3.1");
  assert.equal(lock.commit, "e970694e5e0b17b07cf187f3128af93fa1a82b76");
  assert.equal(lock.licence, "MIT");
  const paths = lock.files.map((f) => f.path);
  assert.deepEqual(paths, [...paths].sort());
  assert.equal(new Set(paths).size, paths.length);
});

test("each file of the vendor directories is in the lock, and the lock lists no other file", () => {
  assert.deepEqual([...onDisk].sort(), [...byPath.keys()].sort());
});

test("each hash is right", () => {
  for (const f of lock.files) {
    assert.match(f.sha256, /^[0-9a-f]{64}$/);
    assert.equal(sha(join(web, f.path)), f.sha256, f.path);
  }
});

test("each copy equals its upstream hash, or holds // hune: comments", () => {
  for (const f of lock.files.filter((x) => x.origin === "copy")) {
    assert.match(f.upstream_sha256, /^[0-9a-f]{64}$/);
    if (f.sha256 !== f.upstream_sha256) {
      assert.ok(readFileSync(join(web, f.path), "utf8").includes("// hune:"), `${f.path} differs without a // hune: comment`);
    }
  }
  for (const f of lock.files.filter((x) => x.origin === "hune")) assert.equal(f.upstream_sha256, undefined);
});

test("upstream hashes match the upstream checkout", { skip: !existsSync(join(UPSTREAM, "src/static")) }, () => {
  for (const f of lock.files.filter((x) => x.origin === "copy")) {
    assert.equal(sha(join(UPSTREAM, f.upstream_path)), f.upstream_sha256, f.upstream_path);
  }
});

const importRe = /^\s*import\s+(?:\{([^}]*)\}|\*\s+as\s+\w+|\w+)\s+from\s+["'](\.[^"']+)["']/gm;

function importsOf(file) {
  const text = readFileSync(join(web, file), "utf8");
  const out = [];
  for (const m of text.matchAll(importRe)) {
    const names = m[1] ? m[1].split(",").map((s) => s.trim().split(/\s+as\s+/)[0]).filter(Boolean) : [];
    out.push({ spec: m[2], names });
  }
  return out;
}

test("each relative import of a vendor module names a file of the lock", () => {
  for (const file of onDisk.filter((p) => p.endsWith(".js"))) {
    for (const { spec } of importsOf(file)) {
      const target = normalize(join(dirname(file), spec));
      assert.ok(byPath.has(target), `${file} imports ${spec}, which is not in the lock`);
    }
  }
});

test("each shim exports the names that the copies import", async () => {
  const wanted = new Map();
  for (const f of lock.files.filter((x) => x.origin === "copy" && x.path.endsWith(".js"))) {
    for (const { spec, names } of importsOf(f.path)) {
      const target = normalize(join(dirname(f.path), spec));
      if (byPath.get(target).origin !== "hune") continue;
      if (!wanted.has(target)) wanted.set(target, new Set());
      for (const n of names) wanted.get(target).add(n);
    }
  }
  assert.ok(wanted.size >= 6);
  for (const [target, names] of wanted) {
    const mod = await import(join(web, target));
    for (const n of names) assert.ok(n in mod, `${target} does not export ${n}`);
  }
});

test("the shims touch no DOM at import and are importable in Node", async () => {
  for (const f of lock.files.filter((x) => x.origin === "hune" && x.path.endsWith(".js"))) {
    await import(join(web, f.path));
  }
  assert.equal(typeof (await import(join(web, "js/vendor/chdash/app_wasm.js"))).wasm.get(), "object");
});

test("the shim NUMERIC_RE, wasm and loader behave as 9.24 says", async () => {
  const { NUMERIC_RE } = await import(join(web, "js/vendor/chdash/app_results_values.js"));
  // E28: the upstream form with one way to match a run of digits (review v3, finding P1).
  assert.equal(NUMERIC_RE.source, String.raw`^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$`);
  const { wasm } = await import(join(web, "js/vendor/chdash/app_wasm.js"));
  assert.deepEqual(wasm.ops, {});
  assert.equal(wasm.get(), null);
  assert.equal(await wasm.load(), null);
  const { loader } = await import(join(web, "js/vendor/chdash/app_loader.js"));
  assert.deepEqual(await loader.loadGroup("x"), {});
  const { storage } = await import(join(web, "js/vendor/chdash/app_state.js"));
  for (const key of Object.values(storage.KEYS)) assert.ok(key.startsWith("hune.chdash."));
  assert.equal(storage.pref(storage.KEYS.chartLegend, "list").get(), "list");
});

// The edits of milestone F and of the parity waves of the chart engine (docs/engine-patches.md): each one has a `// hune:` mark.
test("the engine edits E7 to E12 are marked", () => {
  const core = readFileSync(join(web, "js/vendor/chdash/app_chart_core.js"), "utf8");
  const marks = [
    /\/\/ hune: the ticks and the texts of a time axis follow a zone/, // E7
    /\/\/ hune: a hook of the host draws under the series/, // E8
    /\/\/ hune: the sort of the legend table/, // E9
    /\/\/ hune: bars on a time axis draw the bars next to the range too/, // E10
    /\/\/ hune: a title under the x axis \(xTitle\) takes 20 px more/, // E11
    /\/\/ hune: 11\.5 px of room around a label of the x axis/, // E12
  ];
  for (const m of marks) assert.match(core, m);
  assert.match(core, /opts\.drawUnder\(ctx, layout/);
  assert.match(core, /opts\.barLabel\(ctx, b, L\)/);
  assert.match(core, /opts\.tooltipHideZeros === true/);
});

// The edits 13 to 15 of wave 2 (F16, F17): the right y scale, the style of one series, the titles of the axes and the soft ends.
test("the engine edits E13 to E15 are marked and the options exist", () => {
  const core = readFileSync(join(web, "js/vendor/chdash/app_chart_core.js"), "utf8");
  const marks = [
    /\/\/ hune: E13, F16\. A chart has the left scale `y`/, // E13
    /\/\/ hune: E14, F17\. A series with `negate` draws -v/, // E14
    /\/\/ hune: E15\. A title of a y axis has the font of the labels/, // E15
    /\/\/ hune: E15, F16\. A soft end moves the end of the data/, // E15
  ];
  for (const m of marks) assert.match(core, m);
  for (const needle of ["opts.scales", "opts.y2", "so.ySoft", "opts.drawOrder", "s.dashCap", "s.negate", "opts.gridScale", "chartCore__axisMark"]) assert.ok(core.includes(needle), needle);
});
