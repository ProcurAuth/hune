// SPDX-License-Identifier: MIT OR Apache-2.0
// The registry and the shared helpers of the loading previews (9.11): web/js/preview/index.js and web/js/preview/common.js.
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";
import {
  BAR_HEIGHT, DATA, EXACT, HOOKS, LEAST, MOST, PARTS, TOLERANCE_PX, barNode, boundHolds, contentBox, exact, inset, intersect, least, lineNode, most, neutral,
  partAttrs, pathNode, px, regionNode, resultOf, setMeasurer, svgNode, textWidth, union, unknown,
} from "../../web/js/preview/common.js";
import { previewOf, skeletonKind } from "../../web/js/preview/index.js";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");

const TYPES = ["timeseries", "xychart", "barchart", "stat", "gauge", "bargauge", "table", "piechart", "heatmap", "histogram", "state_timeline", "status_history", "logs", "geomap"];
const FAMILIES = ["timeseries", "barchart", "piechart", "stat", "gauge", "bargauge", "table", "heatmap", "histogram", "timeline", "logs", "geomap"];
const panel = (type, options = {}, fields = {}) => ({ id: 1, type, options, fields });

// ---- The registry --------------------------------------------------------------------------------------------------------------

test("each queried panel type has a family, and a text has none", () => {
  for (const type of TYPES) assert.ok(skeletonKind(type), type);
  assert.equal(skeletonKind("text"), "");
  assert.equal(skeletonKind("not-a-type"), "");
  assert.equal(skeletonKind("constructor"), "", "an inherited name is not a type");
  assert.equal(skeletonKind("__proto__"), "");
  assert.deepEqual([...new Set(TYPES.map(skeletonKind))].sort(), [...FAMILIES].sort(), "every family has a type: no dead module");
});

test("a type without a family has no preview", () => {
  assert.equal(previewOf(panel("text"), { w: 100, h: 100 }), null);
  assert.equal(previewOf(null, { w: 100, h: 100 }), null);
});

test("each family module exports build, and the stub gives the neutral form", () => {
  const files = readdirSync(new URL("../../web/js/preview/", import.meta.url)).filter((n) => n.endsWith(".js") && n !== "index.js" && n !== "common.js" && n !== "grids.js");
  assert.deepEqual(files.map((n) => n.slice(0, -3)).sort(), [...FAMILIES].sort());
  for (const type of TYPES) {
    const out = previewOf(panel(type), { w: 300, h: 200 }, {}, {});
    assert.equal(out.family, skeletonKind(type), type);
    assert.ok(Array.isArray(out.elements) && Array.isArray(out.notes), type);
    assert.ok(out.bounds && typeof out.bounds === "object", type);
  }
});

test("a family that throws gives the neutral form with the note error", () => {
  // The registry never lets a preview break a panel: a box that is not an object makes a family fail in a controlled way.
  const out = previewOf(panel("timeseries"), null, null, null);
  assert.ok(out && Array.isArray(out.elements));
});

test("the preview is drawn with the CSSOM and the SVG namespace, never with a style attribute", () => {
  const js = read("web/js/preview/index.js");
  assert.match(js, /document\.createElementNS\(SVG_NS, n\.t\)/);
  assert.match(js, /e\.style\.setProperty\(name, value\)/);
  assert.ok(!/setAttribute\("style"|innerHTML|insertAdjacentHTML/.test(js));
  assert.match(js, /"aria-hidden": "true"/);
});

test("no family module reads a query: the preview uses the JSON document only", () => {
  for (const name of FAMILIES) {
    const js = read(`web/js/preview/${name}.js`).replace(/\/\/.*$/gm, "");
    assert.ok(!/\.queries\b|\.sql\b|\.expr\b|rawSql|query_text/.test(js), `${name} reads a query`);
    assert.ok(!/localStorage|sessionStorage|fetch\(/.test(js), `${name} reads a store or the network`);
  }
});

// ---- The helpers ---------------------------------------------------------------------------------------------------------------

test("the hooks and the parts have fixed names", () => {
  assert.deepEqual({ ...HOOKS }, { ROOT: "data-skel", PART: "data-skel-part", CLASS: "data-skel-class" });
  assert.ok(Object.isFrozen(HOOKS) && Object.isFrozen(PARTS));
  assert.equal(new Set(Object.values(PARTS)).size, Object.keys(PARTS).length);
  assert.deepEqual(partAttrs(PARTS.LEGEND, EXACT), { "data-skel-part": "legend", "data-skel-class": "exact" });
  assert.deepEqual(partAttrs(PARTS.MARK), { "data-skel-part": "mark", "data-skel-class": "data" });
});

test("a px value keeps up to three decimals and no rounding to a tenth", () => {
  assert.equal(px(8), "8px");
  assert.equal(px(8.25), "8.25px");
  assert.equal(px(8.123456), "8.123px");
  assert.equal(px(0), "0px");
});

test("the nodes carry the custom properties and the hooks, and no style or colour attribute", () => {
  const bar = barNode(4, 10, 37.5);
  assert.equal(bar.c, "skel__b");
  assert.deepEqual(bar.v, { "--x": "4px", "--y": "10px", "--w": "37.5px", "--h": `${BAR_HEIGHT}px` });
  assert.equal(bar.a["data-skel-part"], "label");
  assert.equal(bar.a["data-skel-class"], EXACT);
  const region = regionNode({ x: 1, y: 2, w: 30, h: 40 }, PARTS.PLOT);
  assert.deepEqual(region.v, { "--x": "1px", "--y": "2px", "--w": "30px", "--h": "40px" });
  assert.equal(region.a["data-skel-class"], LEAST);
  const svg = svgNode(120, 80, [pathNode("M0 0L10 10"), lineNode(0, 1, 2, 3)]);
  assert.equal(svg.t, "svg");
  assert.equal(svg.a.viewBox, "0 0 120 80");
  assert.deepEqual(svg.k.map((n) => n.t), ["path", "line"]);
  for (const n of [bar, region, svg, ...svg.k]) {
    assert.ok(!("style" in n.a) && !("fill" in n.a) && !("stroke" in n.a));
    for (const [name, value] of Object.entries(n.v)) {
      assert.match(name, /^--[a-z-]+$/);
      assert.match(String(value), /^-?[0-9.]+(px|%)?$/);
    }
  }
});

test("the rectangle maths never gives a negative size", () => {
  assert.deepEqual(contentBox(300, 200), { x: 0, y: 0, w: 284, h: 184 });
  assert.deepEqual(contentBox(10, 10), { x: 0, y: 0, w: 0, h: 0 });
  assert.deepEqual(inset({ x: 0, y: 0, w: 100, h: 50 }, 8, 8, 21, 40), { x: 40, y: 8, w: 52, h: 21 });
  assert.deepEqual(inset({ x: 0, y: 0, w: 10, h: 10 }, 20, 20, 20, 20), { x: 20, y: 20, w: 0, h: 0 });
  assert.equal(intersect({ x: 0, y: 0, w: 10, h: 10 }, { x: 10, y: 0, w: 5, h: 5 }), null, "touching edges are no overlap");
  assert.deepEqual(intersect({ x: 0, y: 0, w: 10, h: 10 }, { x: 5, y: 5, w: 10, h: 10 }), { x: 5, y: 5, w: 5, h: 5 });
  assert.deepEqual(union({ x: 0, y: 0, w: 10, h: 10 }, { x: 5, y: 5, w: 10, h: 10 }), { x: 0, y: 0, w: 15, h: 15 });
});

test("a bound holds against the loaded value by its class", () => {
  assert.equal(TOLERANCE_PX, 0.5);
  assert.equal(boundHolds(exact(40), 40.5), true);
  assert.equal(boundHolds(exact(40), 40.6), false);
  assert.equal(boundHolds(exact(40), 39.5), true);
  assert.equal(boundHolds(least(30), 30), true);
  assert.equal(boundHolds(least(30), 40), true, "the preview takes less room than the loaded panel");
  assert.equal(boundHolds(least(41), 40), false, "the preview takes more room than the loaded panel");
  assert.equal(boundHolds(least(40.4), 40), true, "within the tolerance");
  assert.equal(boundHolds(most(50), 40), true);
  assert.equal(boundHolds(most(30), 40), false);
  assert.equal(boundHolds(unknown(), 12), true, "data: nothing to check");
  assert.equal(boundHolds(null, 12), true);
  assert.equal(boundHolds(exact(1), NaN), false, "a loaded value that is not a number never agrees");
  assert.equal(boundHolds({ kind: "other", value: 1 }, 1), false, "an unknown class never agrees");
  assert.deepEqual([EXACT, LEAST, MOST, DATA], ["exact", "least", "most", "data"]);
});

test("the neutral form has no element, no bound and one note", () => {
  assert.deepEqual(neutral(), { elements: [], bounds: {}, notes: ["neutral"] });
  assert.deepEqual(neutral("empty"), { elements: [], bounds: {}, notes: ["empty"] });
  assert.deepEqual(resultOf(), { elements: [], bounds: {}, notes: [] });
});

test("the width of a text comes from the measurer with the font of the page", () => {
  const seen = [];
  setMeasurer((text, font) => {
    seen.push(font);
    return text.length * 7;
  });
  try {
    assert.equal(textWidth("hello", { family: "Test" }), 35);
    assert.equal(textWidth("ab", { size: 14, weight: 600, family: "Test" }), 14);
    assert.deepEqual(seen, ["400 12px Test", "600 14px Test"]);
  } finally {
    setMeasurer(null);
  }
});
