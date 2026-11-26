// SPDX-License-Identifier: MIT OR Apache-2.0
// The chart panels against the specification (4.26, 9.19): the table legend with no value column, the placement of a legend that the
// engine hides, the x axis of bars, and the legend cap of the narrow fallback. Each test fails on the code before the fix.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { applyLegend } from "../../web/js/panels/common.js";
import { xDomainOf } from "../../web/js/panels/timeseries.js";
import { axisPlan } from "../../web/js/panels/series.js";
import { hbarLegendModel, sortLegend } from "../../web/js/panels/barchart_h.js";

const WEB = join(dirname(fileURLToPath(import.meta.url)), "../../web");
const css = readFileSync(join(WEB, "css/panel-chart.css"), "utf8");

// A host with a dataset and attributes, and a chart whose legend element is the one given.
function fake(legendEl) {
  const attrs = new Set();
  const host = { dataset: {}, clientWidth: 800, hasAttribute: (n) => attrs.has(n), setAttribute: (n) => attrs.add(n), removeAttribute: (n) => attrs.delete(n) };
  const modes = [];
  const chart = { root: { querySelector: (sel) => (sel === ".chartCore__legend" ? legendEl : null) }, setLegendMode: (m) => modes.push(m), destroy() {} };
  return { host, chart, modes };
}

test("spec 4.26: a table legend with no value column shows the column Name only", () => {
  const legend = { show: true, placement: "bottom", mode: "table", values: [] };
  const { host, chart } = fake(null);
  applyLegend(host, chart, legend);
  assert.equal(host.dataset.legendColumns, "none");
  // A table with a column, and a list, keep the columns of the engine.
  const withColumn = fake(null);
  applyLegend(withColumn.host, withColumn.chart, { ...legend, values: ["max"] });
  assert.equal(withColumn.host.dataset.legendColumns, undefined);
  const list = fake(null);
  applyLegend(list.host, list.chart, { ...legend, mode: "list" });
  assert.equal(list.host.dataset.legendColumns, undefined);
  assert.match(css, /\[data-legend-columns="none"\][^{]*:not\(:first-child\)\s*\{\s*display: none/);
});

test("spec 9.19: a right legend that the engine hides leaves no grid gap", () => {
  globalThis.MutationObserver = class { observe() {} disconnect() {} };
  globalThis.ResizeObserver = class { observe() {} disconnect() {} };
  try {
    const legendEl = { hidden: true, offsetWidth: 0 };
    const { host, chart } = fake(legendEl);
    applyLegend(host, chart, { show: true, placement: "right", mode: "list", values: [] });
    assert.equal(host.dataset.legendPlacement, "bottom", "every entry hidden: the gap of the right placement goes");
    // The legend shows again: the placement of the document comes back.
    legendEl.hidden = false;
    legendEl.offsetWidth = 100;
    const again = fake(legendEl);
    applyLegend(again.host, again.chart, { show: true, placement: "right", mode: "list", values: [] });
    assert.equal(again.host.dataset.legendPlacement, "right");
    // A legend that is off has no placement.
    const off = fake(legendEl);
    applyLegend(off.host, off.chart, { show: false, placement: "right", mode: "list", values: [] });
    assert.equal(off.host.dataset.legendPlacement, "bottom");
  } finally {
    delete globalThis.MutationObserver;
    delete globalThis.ResizeObserver;
  }
});

test("spec 9.19: the x axis of bars keeps the span of the data, a line has the requested range", () => {
  const range = { from: 1000, to: 5000 };
  assert.equal(xDomainOf("bar", range), undefined);
  assert.deepEqual(xDomainOf("line", range), [1000, 5000]);
  assert.deepEqual(xDomainOf("area", range), [1000, 5000]);
  assert.equal(xDomainOf("line", { from: 0, to: 0 }), undefined);
  assert.equal(xDomainOf("line", null), undefined);
});

test("spec 9.19: with every axis hidden, no series is on the right scale", () => {
  const field = (unit, placement) => ({ kind: "number", config: { unit, set: { axis_placement: placement } } });
  const plan = axisPlan([field("ms", "hidden"), field("reqps", "hidden")]);
  assert.deepEqual(plan.sides, ["left", "left"]);
  assert.equal(plan.right, null);
  assert.equal(plan.left.shown, false);
  // One series that shows an axis keeps the rule of the unit: a hidden series of another unit goes right.
  const mixed = axisPlan([field("ms", "auto"), field("reqps", "hidden")]);
  assert.deepEqual(mixed.sides, ["left", "right"]);
});

test("spec 9.19: a legend under the plot in the fallback scrolls beyond 35 % of the height", () => {
  const body = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rule = (selector) => {
    const at = body.indexOf(selector);
    assert.ok(at >= 0, `the rule ${selector}`);
    return body.slice(at, body.indexOf("}", at));
  };
  const list = rule(".panel__plot[data-legend-fallback] .chartCore__legendList {");
  assert.match(list, /max-height: var\(--legend-max/);
  assert.match(list, /overflow-y: auto/);
  const wrap = rule(".panel__plot[data-legend-fallback] .chartCore__legendTableWrap {");
  assert.match(wrap, /max-height: var\(--legend-max/);
  assert.doesNotMatch(wrap, /max-height: 8rem/);
});

// A series of the horizontal bars: the name, the values and the flags of `hide_from`.
const bar = (label, values, flags = {}) => {
  const v = Float64Array.from(values);
  return { id: label, label, values: v, field: { kind: "number", values: v, nulls: new Uint8Array(v.length), config: { unit: "none", decimals: null } }, legendHidden: false, ...flags };
};

test("spec 9.19: the legend of horizontal bars follows the shared object: show, mode, values and hide_from.legend", () => {
  const series = [bar("a", [1, 5]), bar("b", [2, 3], { legendHidden: true }), bar("c", [9])];
  const list = hbarLegendModel(series, { show: true, placement: "bottom", mode: "list", values: ["max"] });
  assert.deepEqual([list.show, list.mode, list.columns, list.entries.map((s) => s.label)], [true, "list", [], ["a", "c"]], "a list has no column, and a hidden entry is out");
  const table = hbarLegendModel(series, { show: true, placement: "right", mode: "table", values: ["last", "max", "last", "bogus"] });
  assert.deepEqual([table.mode, table.columns], ["table", ["last", "max"]], "the columns of the document, with no duplicate");
  assert.deepEqual(hbarLegendModel(series, { show: true, placement: "bottom", mode: "table", values: [] }).columns, [], "values = [] gives the column Name only");
  assert.equal(hbarLegendModel(series, { show: false, placement: "bottom", mode: "list", values: [] }).show, false);
  const hidden = [bar("a", [1], { legendHidden: true })];
  assert.equal(hbarLegendModel(hidden, { show: true, placement: "right", mode: "list", values: [] }).show, false, "every entry hidden: no legend");
  assert.equal(hbarLegendModel([], { show: true, placement: "bottom", mode: "list", values: [] }).show, false, "no series: no legend");
});

test("spec 9.19: the legend table sorts by name or by a calculation, with the empty keys last", () => {
  const series = [bar("b", [5]), bar("A", [9]), bar("c", [NaN]), bar("d", [1])];
  const names = (sort) => sortLegend(series, sort).map((s) => s.label);
  assert.deepEqual(names(null), ["b", "A", "c", "d"]);
  assert.deepEqual(names({ by: "name", desc: false }), ["A", "b", "c", "d"]);
  assert.deepEqual(names({ by: "name", desc: true }), ["d", "c", "b", "A"]);
  assert.deepEqual(names({ by: "max", desc: false }), ["d", "b", "A", "c"]);
  assert.deepEqual(names({ by: "max", desc: true }), ["A", "b", "d", "c"]);
  assert.deepEqual(names({ by: "last_not_null", desc: false }), ["d", "b", "A", "c"]);
});
