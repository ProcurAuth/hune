// SPDX-License-Identifier: MIT OR Apache-2.0
// The wait of a panel (9.11): first load and reload are two cases, and the test of the same data. The previews are in preview_common.test.mjs.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { WAITING, rangeKey, sameFrames, waitMode } from "../../web/js/panel_wait.js";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

test("a first load shows the preview and the dots: nothing is on the screen", () => {
  for (const state of ["idle", "queued", "loading"]) assert.equal(waitMode(state, "nothing"), "first", state);
});

test("a reload keeps the content or the text No data, and shows the dots only", () => {
  for (const state of ["idle", "queued", "loading"]) {
    assert.equal(waitMode(state, "content"), "reload", state);
    assert.equal(waitMode(state, "empty"), "reload", state);
  }
});

test("a panel that does not wait shows no sign, whatever is on the screen", () => {
  for (const state of ["ready", "empty", "error", "timeline-limit", "bucket-limit", "geo-limit", "text-limit"]) {
    for (const shown of ["content", "empty", "nothing"]) assert.equal(waitMode(state, shown), "", `${state} ${shown}`);
  }
  assert.deepEqual([...WAITING].sort(), ["idle", "loading", "queued"]);
});

test("a try that failed leaves nothing on the screen: the next wait is a first load again", () => {
  // The error overlay is not content and not the text No data: the frame reads "nothing" once it hides the overlay for the new try.
  assert.equal(waitMode("loading", "nothing"), "first");
});

test("a request that another one replaced leaves no dots: the state that follows ends the wait", () => {
  // The runtime sets the state of an aborted request itself (runtime.js). Each state that is not a waiting state clears the mode.
  assert.equal(waitMode("loading", "content"), "reload");
  assert.equal(waitMode("ready", "content"), "");
  assert.equal(waitMode("error", "nothing"), "");
});

// ---- The same data -------------------------------------------------------------------------------------------------------------

const frame = (values, extra = {}) => ({ interval_ms: 1000, rows: values[0].length, columns: [{ name: "t", type: "DateTime", kind: "time" }, { name: "v", type: "UInt64", kind: "number" }], values, ...extra });

test("an answer with the same data is the same, whatever its request members say", () => {
  const a = [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]], { query_id: "q1", stats: { elapsed_ms: 10 }, sql: "SELECT 1 -- 1", etag: "x" }) }];
  const b = [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]], { query_id: "q2", stats: { elapsed_ms: 99 }, sql: "SELECT 1 -- 2", etag: "y" }) }];
  assert.equal(sameFrames(a, b), true);
  assert.equal(sameFrames(a, a), true);
});

test("a changed value, column, interval, ref or count is a different answer", () => {
  const base = () => [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]]) }];
  assert.equal(sameFrames(base(), [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 7]]) }]), false, "a value");
  assert.equal(sameFrames(base(), [{ ref: "A", frame: frame([[1, 2], [4, 5]]) }]), false, "a row count");
  assert.equal(sameFrames(base(), [{ ref: "B", frame: frame([[1, 2, 3], [4, 5, 6]]) }]), false, "a ref");
  assert.equal(sameFrames(base(), [{ ref: "A", frame: frame([[1, 2, 3], [4, 5, 6]], { interval_ms: 2000 }) }]), false, "an interval");
  const other = frame([[1, 2, 3], [4, 5, 6]]);
  other.columns[1].name = "w";
  assert.equal(sameFrames(base(), [{ ref: "A", frame: other }]), false, "a column");
  assert.equal(sameFrames(base(), base().concat(base())), false, "a count of answers");
  assert.equal(sameFrames(null, base()), false);
  assert.equal(sameFrames(base(), null), false);
  assert.equal(sameFrames([{ ref: "A", frame: null }], base()), false);
});

test("a value of another type or a null is a different value", () => {
  const a = [{ ref: "A", frame: frame([[1, null], ["x", "y"]]) }];
  assert.equal(sameFrames(a, [{ ref: "A", frame: frame([[1, null], ["x", "y"]]) }]), true);
  assert.equal(sameFrames(a, [{ ref: "A", frame: frame([[1, 0], ["x", "y"]]) }]), false);
  assert.equal(sameFrames(a, [{ ref: "A", frame: frame([["1", null], ["x", "y"]]) }]), false);
});

test("a very large answer counts as different, so the test stays cheap", () => {
  const big = () => [{ ref: "A", frame: frame([new Array(300000).fill(1), new Array(300000).fill(2)]) }];
  assert.equal(sameFrames(big(), big()), false);
});

test("a chart draws again when its range changes, and not when the range is the same", () => {
  assert.equal(rangeKey({ from_ms: 1, to_ms: 2 }), "1:2");
  assert.equal(rangeKey({ from_ms: 1, to_ms: 2 }), rangeKey({ from_ms: 1, to_ms: 2 }));
  assert.notEqual(rangeKey({ from_ms: 1, to_ms: 2 }), rangeKey({ from_ms: 1, to_ms: 3 }));
  assert.equal(rangeKey(null), "");
  assert.equal(rangeKey({ error: "x", from_ms: 1, to_ms: 2 }), "");
  assert.equal(rangeKey({ from_ms: NaN, to_ms: 2 }), "");
});

test("the frame uses the model: the mode is set with the state, the same data does not draw, the module loads before the old drawing goes", () => {
  const js = read("web/js/panel_frame.js");
  assert.match(js, /import \{ buildPreview, observePreview \} from "\.\/preview\/index\.js";/);
  assert.match(js, /import \{ rangeKey, sameFrames, WAITING, waitMode \} from "\.\/panel_wait\.js";/);
  assert.match(js, /const skeleton = buildPreview\(panel\);/, "the preview follows the whole panel, not its type only");
  assert.match(js, /observePreview\(skeleton, panel, rt\.doc, \{ rt, page \}\)/, "the preview builds when its box has a size");
  assert.match(js, /stopPreview\(\);/, "the observer stops with the panel");
  assert.match(js, /root\.dataset\.wait = mode;/);
  assert.match(js, /!sameFrames\(rendered, info\.items\)/);
  const draw = js.slice(js.indexOf("async function draw(info)"), js.indexOf("function framesSelect"));
  assert.ok(draw.indexOf("mod = await LOADERS[panel.type]()") < draw.indexOf("destroyInstance();\n    clear(content);"), "no await between the clear and the render");
  const after = draw.slice(draw.indexOf("destroyInstance();\n    clear(content);"));
  assert.equal((after.match(/\bawait\b/g) || []).length, 0, "no await after the old drawing goes");
});

test("the preview is drawn with the CSSOM and the SVG namespace, never with a style attribute", () => {
  const js = read("web/js/preview/index.js");
  assert.match(js, /document\.createElementNS\(SVG_NS, n\.t\)/);
  assert.match(js, /e\.style\.setProperty\(name, value\)/);
  assert.ok(!/setAttribute\("style"|innerHTML|insertAdjacentHTML/.test(js));
  assert.match(js, /"aria-hidden": "true"/);
});

test("the preview and the dots take no colour literal, and the light that sweeps moves with a transform only", () => {
  const css = read("web/css/panel-loading.css");
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/.test(css));
  const keyframes = [...css.matchAll(/@keyframes ([\w-]+)/g)].map((m) => m[1]);
  assert.deepEqual(keyframes, ["panel-pulse", "panel-skel"]);
  // The light is one band: its keyframes move it with translateX and change nothing else (no size, no position, no repaint of the shapes).
  const start = css.indexOf("@keyframes panel-skel");
  const skel = css.slice(start, css.indexOf("\n", start));
  assert.match(skel, /translateX\(-100%\)/);
  assert.ok(!/width|height|left|top|opacity/.test(skel), skel);
  // Reduced motion stops the light and keeps the drawing still.
  assert.match(css, /@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\.panel__skeleton::after \{ animation: none; display: none; \}/);
});
