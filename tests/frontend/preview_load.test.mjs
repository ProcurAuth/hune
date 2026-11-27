// SPDX-License-Identifier: MIT OR Apache-2.0
// The load on demand of the preview families (9.11): web/js/preview/index.js. The registry holds no static import of a family module.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import { installFakeDom } from "./fake_dom.mjs";
import { loadFamily, loadPreviewFamilies, previewOf } from "../../web/js/preview/index.js";

const FAMILIES = ["timeseries", "barchart", "piechart", "stat", "gauge", "bargauge", "table", "heatmap", "histogram", "timeline", "logs", "geomap"];
const source = readFileSync(new URL("../../web/js/preview/index.js", import.meta.url), "utf8");

test("the registry imports each family with a quoted path and no static import", () => {
  for (const family of FAMILIES) {
    assert.ok(source.includes(`import("./${family}.js")`), family);
    assert.ok(!new RegExp(`^import .* from "\\./${family}\\.js"`, "m").test(source), `${family} has a static import`);
  }
});

test("previewOf gives null until the module of the family is loaded, then it draws", async () => {
  const pie = { id: 1, type: "piechart", options: {}, fields: {} };
  assert.equal(previewOf(pie, { w: 300, h: 200 }, {}, {}), null);
  const first = loadFamily("piechart");
  assert.equal(loadFamily("piechart"), first, "one promise for each family");
  await first;
  assert.equal(previewOf(pie, { w: 300, h: 200 }, {}, {}).family, "piechart");
  await loadPreviewFamilies();
  assert.equal(previewOf({ id: 2, type: "table", options: {}, fields: {} }, { w: 300, h: 200 }, {}, {}).family, "table");
});

// ---- observePreview with a family that is not loaded ----------------------------------------------------------------------------

installFakeDom();
const WEB = new URL("../../web/js/", import.meta.url);
const pie = { id: 1, type: "piechart", options: {}, fields: {} };
const tick = () => new Promise((resolve) => setImmediate(resolve));
let fresh = 0;
// A new copy of the registry has no loaded family, and it shares the family modules with the registry of the page.
const registry = (tag) => import(`../../web/js/preview/index.js?${tag}-${fresh++}`);
// A skeleton with a box that has a size. The fake node has no layout, so the test gives the size.
const sized = (skeleton) => {
  Object.defineProperty(skeleton, "firstElementChild", { get: () => skeleton.children[0] });
  Object.defineProperty(skeleton, "clientWidth", { value: 100 });
  Object.defineProperty(skeleton, "clientHeight", { value: 40 });
  return skeleton;
};

test("a preview that builds before its module is loaded is marked, waits for the module, draws once and clears the mark", async () => {
  const reg = await registry("wait");
  const skeleton = sized(reg.buildPreview(pie));
  assert.equal(skeleton.dataset.unpainted, "", "the empty preview is marked");
  const seen = [];
  const stop = reg.observePreview(skeleton, pie, {}, {}, (result) => seen.push(result));
  stop.measure();
  assert.equal(skeleton._preview, undefined, "nothing draws before the module is there");
  assert.equal(skeleton.dataset.unpainted, "");
  await reg.loadFamily("piechart");
  await tick();
  assert.equal(seen.length, 1, "the callback runs once");
  assert.equal(skeleton._preview.family, "piechart");
  assert.equal(skeleton.dataset.unpainted, undefined, "the mark clears with the first drawing");
  stop.measure();
  assert.equal(seen.length, 1, "the same size draws nothing again");
  stop();
  const loaded = reg.buildPreview(pie);
  assert.equal(loaded.dataset.unpainted, undefined, "a preview of a loaded family has no mark");
});

test("stop before the module arrives gives no drawing", async () => {
  const reg = await registry("stop");
  const skeleton = sized(reg.buildPreview(pie));
  let calls = 0;
  const stop = reg.observePreview(skeleton, pie, {}, {}, () => calls++);
  stop.measure();
  stop();
  await reg.loadFamily("piechart");
  await tick();
  assert.equal(skeleton._preview, undefined);
  assert.equal(calls, 0);
});

test("a rejected import gives no preview, no unhandled rejection and a new try at the next call", async () => {
  // A copy of the registry whose import of the gauge module has a wrong path. The other paths point at the real files.
  const dir = mkdtempSync(join(tmpdir(), "hune-preview-"));
  const unhandled = [];
  const onUnhandled = (error) => unhandled.push(error);
  process.on("unhandledRejection", onUnhandled);
  try {
    const abs = (path) => pathToFileURL(new URL(path, WEB).pathname).href;
    const text = source
      .replace('import("./gauge.js")', 'import("./missing_family.js")')
      .replace(/import\("\.\/(\w+)\.js"\)/g, (_, name) => `import("${abs(`preview/${name}.js`)}")`)
      .replace('from "../ui.js"', `from "${abs("ui.js")}"`)
      .replace('from "./common.js"', `from "${abs("preview/common.js")}"`);
    const file = join(dir, "index.mjs");
    writeFileSync(file, text);
    const reg = await import(pathToFileURL(file).href);
    const gauge = { id: 2, type: "gauge", options: {}, fields: {} };
    const skeleton = sized(reg.buildPreview(gauge));
    const stop = reg.observePreview(skeleton, gauge, {}, {}, () => assert.fail("a preview draws with no module"));
    stop.measure();
    await tick();
    await tick();
    assert.equal(skeleton._preview, undefined);
    assert.equal(skeleton.dataset.unpainted, "", "the empty preview stays hidden");
    const first = reg.loadFamily("gauge");
    await assert.rejects(first);
    const second = reg.loadFamily("gauge");
    assert.notEqual(first, second, "a failed import is forgotten");
    await assert.rejects(second);
    await tick();
    assert.deepEqual(unhandled, []);
    stop();
  } finally {
    process.off("unhandledRejection", onUnhandled);
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the loader of the bar chart also loads the module of the timeseries family, which the bar chart uses for its caches", async () => {
  const reg = await registry("font");
  await reg.loadFamily("barchart");
  // The font handler resets the measures of the module `timeseries` only when the registry holds it: `previewOf` of a time series shows that it does.
  assert.equal(reg.previewOf({ id: 3, type: "timeseries", options: {}, fields: {} }, { w: 300, h: 200 }, {}, {}).family, "timeseries");
});
