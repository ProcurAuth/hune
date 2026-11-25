// SPDX-License-Identifier: MIT OR Apache-2.0
// The wait of a panel (9.11): first load and reload are two cases, the preview of each panel type, and the test of the same data.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { SKELETONS, WAITING, rangeKey, sameFrames, skeletonKind, waitMode } from "../../web/js/panel_wait.js";

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

test("each queried panel type has a preview, and a text has none", () => {
  const types = ["timeseries", "xychart", "barchart", "stat", "gauge", "bargauge", "table", "piechart", "heatmap", "histogram", "state_timeline", "status_history", "logs", "geomap"];
  for (const type of types) {
    const kind = skeletonKind(type);
    assert.ok(kind && SKELETONS[kind] && SKELETONS[kind].length > 0, type);
  }
  assert.equal(skeletonKind("text"), "");
  assert.equal(skeletonKind("not-a-type"), "");
  assert.equal(skeletonKind("constructor"), "", "an inherited name is not a type");
  // Every preview is used by a type: no dead preview.
  const used = new Set(types.map(skeletonKind));
  assert.deepEqual([...Object.keys(SKELETONS)].filter((k) => !used.has(k)), []);
});

test("the shapes stay in the body and use a known form", () => {
  const forms = new Set(["bar", "dot", "line", "ring", "cell", "blob"]);
  for (const [kind, shapes] of Object.entries(SKELETONS)) {
    for (const p of shapes) {
      assert.ok(forms.has(p.s), `${kind} ${p.s}`);
      assert.ok(p.x >= 0 && p.x <= 100 && p.y >= 0 && p.y <= 100, `${kind} point ${p.x},${p.y}`);
      if (p.s === "bar" || p.s === "cell" || p.s === "blob") assert.ok(p.x + p.w <= 100.01 && p.y + p.h <= 100.01, `${kind} box ${p.x},${p.y},${p.w},${p.h}`);
      if (p.s === "ring") assert.ok(p.y + p.h <= 100, `${kind} ring`);
      assert.ok(Number.isInteger(p.d) && p.d >= 0, `${kind} delay`);
    }
  }
});

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
  assert.match(js, /import \{ rangeKey, sameFrames, SKELETONS, skeletonKind, WAITING, waitMode \} from "\.\/panel_wait\.js";/);
  assert.match(js, /root\.dataset\.wait = mode;/);
  assert.match(js, /!sameFrames\(rendered, info\.items\)/);
  const draw = js.slice(js.indexOf("async function draw(info)"), js.indexOf("function framesSelect"));
  assert.ok(draw.indexOf("mod = await LOADERS[panel.type]()") < draw.indexOf("destroyInstance();\n    clear(content);"), "no await between the clear and the render");
  const after = draw.slice(draw.indexOf("destroyInstance();\n    clear(content);"));
  assert.equal((after.match(/\bawait\b/g) || []).length, 0, "no await after the old drawing goes");
});

test("the preview and the dots take no colour literal and do not move the layout", () => {
  const css = read("web/css/panel-loading.css");
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/.test(css));
  // The shapes change their opacity only: the keyframes name no transform that moves, no size and no position.
  const keyframes = [...css.matchAll(/@keyframes ([\w-]+) \{([^@]*?\}) \}/g)];
  assert.equal(keyframes.length, 2);
  const skel = css.slice(css.indexOf("@keyframes panel-skel"), css.indexOf("}", css.indexOf("@keyframes panel-skel") + 40) + 2);
  assert.ok(!/transform|width|height|left|top/.test(skel), skel);
});
