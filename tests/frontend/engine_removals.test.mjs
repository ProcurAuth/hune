// The removals E17 to E21 and E35 to E42 of the copied chart engine (decision ID-K300, docs/engine-patches.md, 9.24).
// Hune never calls the removed code. This test fails when a removed name comes back as a call, an option or an import
// anywhere in web/js. It checks member calls and imports. A bare call counts only in the engine file that held the name,
// because Hune has its own functions with some of these names (copyCell in transform_more.js, copyText in ui.js).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const JS = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "web", "js");
const VENDOR = "vendor/chdash/";

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (name.endsWith(".js")) out.push(full);
  }
  return out;
}

// The code of a file without its comments (a comment can name a removed function).
const code = (text) => text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`\\])\/\/[^\n]*/g, "$1");
const files = walk(JS).map((f) => ({ rel: relative(JS, f).split("\\").join("/"), src: code(readFileSync(f, "utf8")) }));

// The functions and the closure names that each removal took away, by the engine file that held them.
const REMOVED = {
  "app_chart_core.js": [
    // E17, the exemplar markers.
    "placeMarkers", "sampleMarkers", "drawMarkerMarks", "showMarkerTip", "hideMarkerTip", "bindMarkers", "configureMarker", "markerNode",
    // E18, the 2-D brush.
    "startBox", "updateBox", "cancelBox", "boxRange", "clampPoint", "destroyExtras",
    // E19, the engine annotations.
    "placeAnnotations",
    // E36, E37, E40, E41, E42: the bucket readout, the colour kernel, the alignment groups, the legend that filters, the totals
    // legend, the preference of the mode and the options that Hune never sets.
    "rgbaBatch", "alignedAxis", "niceSteps", "durationSteps", "unitOfFactor", "alignGutter", "knownWidth", "settleGroup", "gutterFor",
    "xScaleKey", "readLegendMode", "storeLegendMode", "legendPref", "filterLegend", "legendPressedOf", "legendHint", "seriesTotal",
    "legendTotalItemHtml", "bridgeGaps", "capTooltipRows", "includedY", "dateText",
  ],
  // E35, the helpers of app_util.js that no file calls.
  "app_util.js": ["debounce", "scrollFocus", "latest", "isAbort", "errorText", "queryErrorParts", "setText", "setMetaLine", "setMetricText",
    "formatInt", "formatSeconds", "formatBytes", "replaceTextAreaValue", "isPhoneLayout", "modifierKeyLabel", "normalizeApiErrorPayload",
    "buildApiErrorText", "buildApiErrorFromResponse"],
  // E39, the card of app_ui_chart.js.
  "app_ui_chart.js": ["chartCardHtml"],
  // E38, the members of the shim app_ui_state.js that the engine never calls.
  "app_ui_state.js": ["block", "put", "busy"],
  // E20, the helpers of the table.
  "app_ui_table.js": ["sortHeadHtml", "bindSort", "barEligible", "barPercent", "cellBar", "cellBarStyle", "shareBar", "shareBarHtml", "rowHeight",
    "textCell", "copyCellHtml", "copyCell", "bindCopy", "keyboardMenuTarget", "scrollCues"],
  // E21, the sort of the rows and its WebAssembly kernel.
  "app_results_sort.js": ["requestSortKernel", "numericSortKeys", "orderByKeysJs", "orderByKeysWasm", "sortNumericRows", "sortNumericRowsByComparator",
    "orderTextJs", "sortTextRows", "sortTextRowsByComparator", "setSortWasm", "SORT_WASM_MIN_ROWS"],
};
// The members of the engine objects that the removals took away (chart API and `table`).
const MEMBERS = ["setMarkers", ...REMOVED["app_ui_table.js"]];
// The engine options of E17 to E19. The engine reads none of them, and no panel gives one.
const OPTIONS = ["markers", "markerSpacing", "onMarkerClick", "brush", "brushSnap", "brushTooltip", "brushClass", "onBrush", "annotations",
  // E36, E40, E41, E42.
  "bucketMs", "bucketAlign", "alignKey", "yKind", "yInteger", "legendClick", "legendPressed", "legendDisabled", "legendTitle", "onLegendClick",
  "onLegendMode", "legendTotal", "legendOrder", "legendModeButton", "tooltipTitle", "tooltipMaxRows", "tooltipNulls", "tooltipFooter",
  "onHiddenChange", "xDateOnly", "yInclude", "cursorPoints"];
// The names that a shim no longer exports (E20): the module app_ui_copy.js and `h`, `shell` of app_dom.js.
const SHIM_NAMES = { "app_dom.js": ["h", "shell"] };
const ALL_REMOVED = new Set(Object.values(REMOVED).flat());

const word = (n) => n.replace(/\$/g, "\\$");
const bareCall = (n) => new RegExp(`(?<![\\w$.])${word(n)}\\s*\\(`);
const memberCall = (n) => new RegExp(`\\.\\s*${word(n)}\\s*\\(`);
const optionKey = (n) => new RegExp(`(?<![\\w$.])${word(n)}\\s*:(?!:)`);
const optionRead = (n) => new RegExp(`\\bopts\\.${word(n)}\\b`);
const importRe = /import\s*\{([^}]*)\}\s*from\s*["']([^"']+)["']|import\s*\(\s*["']([^"']+)["']\s*\)|import\s+(?:\*\s+as\s+\w+|\w+)\s+from\s*["']([^"']+)["']/g;

function imports(src) {
  const out = [];
  for (const m of src.matchAll(importRe)) {
    const names = m[1] ? m[1].split(",").map((s) => s.trim().split(/\s+as\s+/)[0]).filter(Boolean) : [];
    out.push({ names, spec: m[2] || m[3] || m[4] });
  }
  return out;
}

test("the checks of this file find a call, an option and an import (self-test)", () => {
  assert.ok(bareCall("placeMarkers").test("    placeMarkers();"));
  assert.ok(!bareCall("placeMarkers").test("x.placeMarkers()"));
  assert.ok(memberCall("cellBar").test("chTable.cellBar(td, 50)"));
  assert.ok(memberCall("setMarkers").test("chart.setMarkers([])"));
  assert.ok(optionKey("brush").test('{ brush: "xy" }') && !optionKey("brush").test("brushSnap: f"));
  assert.ok(optionRead("annotations").test("(opts.annotations || [])"));
  assert.deepEqual(imports('import { sortTextRows, compareTextForSort as c } from "./vendor/chdash/app_results_sort.js";')[0].names, ["sortTextRows", "compareTextForSort"]);
  assert.equal(imports('import { copy } from "./app_ui_copy.js";')[0].spec, "./app_ui_copy.js");
  assert.equal(code("a(); // placeMarkers();"), "a(); ");
  assert.ok(files.length > 100, "web/js has its files");
});

test("no removed function of the engine is called in the engine file that held it", () => {
  for (const [file, names] of Object.entries(REMOVED)) {
    const f = files.find((x) => x.rel === VENDOR + file);
    assert.ok(f, `${file} exists`);
    for (const n of names) assert.ok(!bareCall(n).test(f.src), `${file} calls or defines ${n} again (removed, ID-K300)`);
  }
});

test("no file of web/js calls a removed member of the engine (chart API, table helpers)", () => {
  for (const f of files) for (const n of MEMBERS) assert.ok(!memberCall(n).test(f.src), `${f.rel} calls .${n}() (removed, ID-K300)`);
});

test("the engine reads no removed option, and no panel gives one", () => {
  const core = files.find((x) => x.rel === VENDOR + "app_chart_core.js");
  for (const n of OPTIONS) assert.ok(!optionRead(n).test(core.src), `app_chart_core.js reads opts.${n} again (removed, ID-K300)`);
  // The panels build the options of the engine (createChart in panels/common.js).
  for (const f of files.filter((x) => x.rel.startsWith("panels/"))) {
    for (const n of OPTIONS) assert.ok(!optionKey(n).test(f.src), `${f.rel} gives the engine option ${n} (removed, ID-K300)`);
  }
});

test("no file of web/js imports a removed name, the removed shim app_ui_copy.js, or h and shell of app_dom.js", () => {
  for (const f of files) {
    for (const { names, spec } of imports(f.src)) {
      if (!spec) continue;
      assert.ok(!/(^|\/)app_ui_copy\.js$/.test(spec), `${f.rel} imports app_ui_copy.js (removed, ID-K300)`);
      if (!spec.includes("app_") && !f.rel.startsWith(VENDOR)) continue;
      for (const n of names) assert.ok(!ALL_REMOVED.has(n), `${f.rel} imports ${n} from ${spec} (removed, ID-K300)`);
      for (const [shim, gone] of Object.entries(SHIM_NAMES)) {
        if (!spec.endsWith(shim)) continue;
        for (const n of names) assert.ok(!gone.includes(n), `${f.rel} imports ${n} from ${shim} (removed, ID-K300)`);
      }
    }
  }
  assert.ok(!files.some((x) => x.rel === VENDOR + "app_ui_copy.js"), "the shim app_ui_copy.js is gone");
  // E37: the copy app_palette.js is gone. E36, E37, E41: the chart engine imports no app_format.js, app_palette.js or app_state.js.
  assert.ok(!files.some((x) => x.rel === VENDOR + "app_palette.js"), "the copy app_palette.js is gone");
  const core = files.find((x) => x.rel === VENDOR + "app_chart_core.js");
  for (const gone of ["app_format.js", "app_palette.js", "app_state.js"]) assert.ok(!imports(core.src).some((i) => i.spec.endsWith(gone)), `app_chart_core.js imports ${gone}`);
});
