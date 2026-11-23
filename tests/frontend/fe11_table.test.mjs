// FE-11: table sort and column filter (9.20).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { rowOrder, distinctTexts, initialSort, legibleColor, footerAllowed, footerLayout, footerName, subsetField } from "../../web/js/table_model.js";
import { reduceField } from "../../web/js/reduce.js";
import { NAMED, NAMED_LIGHT, relativeLuminance } from "../../web/js/colors.js";
import { makeDisplay } from "../../web/js/display.js";
import { computeFieldConfig } from "../../web/js/fields.js";
import { toFields } from "./helpers.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-11-table.json", import.meta.url)));
const extra = JSON.parse(readFileSync(new URL("./vectors/FE-11-table-extra.json", import.meta.url)));

const { fields } = computeFieldConfig(toFields(v.cols), { defaults: { unit: "none" }, overrides: [] }, "table");
const displays = fields.map((f) => makeDisplay(f, { zone: "utc" }));
const textAt = (fi, row) => displays[fi](row).text;

test("FE-11 sort and filter", () => {
  for (const c of v.cases) {
    const filters = (c.filters || []).map((f) => ({ index: f.index, allowed: new Set(f.allowed) }));
    const got = rowOrder({ count: 6, fields, sort: c.sort, filters, textAt });
    assert.deepEqual(got, c.expect, c.name);
  }
});

test("FE-11 distinct texts of a column and the initial sort", () => {
  for (const [input, expect] of [extra.distinct.text, extra.distinct.numbers]) {
    const list = input;
    assert.deepEqual(distinctTexts(list.length, 0, (fi, row) => list[row]), expect);
  }
  assert.deepEqual(initialSort(extra.initial.sort, extra.initial.names), extra.initial.expect);
});

test("legible text colour of a color_text cell (contrast 4.5 on the panel)", async () => {
  const { legibleColor } = await import("../../web/js/table_model.js");
  const { relativeLuminance } = await import("../../web/js/colors.js");
  const contrast = (a, b) => {
    const [x, y] = [relativeLuminance(a), relativeLuminance(b)].sort((p, q) => q - p);
    return (x + 0.05) / (y + 0.05);
  };
  const light = { surface: "rgb(255, 255, 255)", ink: "rgb(17, 24, 39)" };
  const dark = { surface: "rgb(20, 24, 31)", ink: "rgb(230, 233, 238)" };
  // A colour that reads well stays as it is.
  assert.equal(legibleColor("#1F60C4", light.surface, light.ink), "#1F60C4");
  assert.equal(legibleColor("#FADE2A", dark.surface, dark.ink), "#FADE2A");
  // Yellow on white and dark blue on a dark panel move towards the ink until they read.
  for (const [c, t] of [["#FADE2A", light], ["#73BF69", light], ["#1F60C4", dark], ["#C4162A", dark]]) {
    const out = legibleColor(c, t.surface, t.ink);
    assert.notEqual(out, c, c);
    assert.ok(contrast(out, t.surface) >= 4.5, `${c} gives ${out}`);
  }
  // A colour that cannot be read, or has an alpha channel, stays as it is.
  assert.equal(legibleColor("text", light.surface, light.ink), "text");
  assert.equal(legibleColor("rgba(250, 222, 42, 0.5)", light.surface, light.ink), "rgba(250, 222, 42, 0.5)");
  assert.equal(legibleColor("#FADE2A", "rgba(0, 0, 0, 0)", light.ink), "#FADE2A");
});

test("coloured text: each named colour of the light table reaches 4.5:1 on the light panel (X4, AA)", () => {
  const contrast = (a, b) => {
    const [x, y] = [relativeLuminance(a), relativeLuminance(b)].sort((m, n) => n - m);
    return (x + 0.05) / (y + 0.05);
  };
  for (const hex of Object.values(NAMED_LIGHT)) {
    assert.ok(contrast(legibleColor(hex, "#ffffff", "#24292e"), "#ffffff") >= 4.5, hex);
  }
  for (const hex of Object.values(NAMED)) {
    assert.ok(contrast(legibleColor(hex, "#181b1f", "#ccccdc"), "#181b1f") >= 4.5, hex);
  }
  // A colour that already passes is not changed (the blue of the light table has 4.8:1).
  assert.equal(legibleColor("#3274D9", "#ffffff", "#24292e"), "#3274D9");
});

test("FE-11 footer: the names of the calculations (9.20)", () => {
  const names = { last_not_null: "Last *", first_not_null: "First *", total: "Total", std_dev: "StdDev", diff: "Difference", diff_percent: "Difference percent",
    change_count: "Change Count", distinct_count: "Distinct Count", min: "Min", max: "Max", mean: "Mean", count: "Count", median: "Median", variance: "Variance",
    range: "Range", delta: "Delta", last: "Last", first: "First" };
  for (const [calc, name] of Object.entries(names)) assert.equal(footerName(calc), name, calc);
});

test("FE-11 footer: a text column shows only some calculations", () => {
  for (const calc of ["first", "first_not_null", "last", "last_not_null", "count", "distinct_count", "change_count"]) {
    assert.ok(footerAllowed("text", calc), calc);
    assert.ok(footerAllowed("bool", calc), calc);
  }
  for (const calc of ["total", "mean", "min", "max", "median", "diff", "delta", "variance", "std_dev", "range", "diff_percent"]) {
    assert.ok(!footerAllowed("text", calc), calc);
    assert.ok(footerAllowed("number", calc), calc);
  }
});

test("FE-11 footer: names in the first column when the others share their items and it has none", () => {
  assert.deepEqual(footerLayout([{ calcs: [] }, { calcs: ["total", "count"] }, { calcs: ["total", "count"] }]), { lines: 2, names: true });
  assert.deepEqual(footerLayout([{ calcs: [] }, { calcs: ["total", "count"] }, { calcs: ["mean"] }]), { lines: 2, names: false });
  assert.deepEqual(footerLayout([{ calcs: ["total"] }, { calcs: ["total"] }]), { lines: 1, names: false });
  assert.deepEqual(footerLayout([{ calcs: [] }, { calcs: [] }]), { lines: 0, names: false });
});

test("FE-11 footer: the calculation runs over the rows that stay after the filters", () => {
  const [f, t] = toFields([{ name: "a", kind: "number", values: [1, 2, null, 4, 5] }, { name: "s", kind: "text", values: ["x", "y", "y", null, "z"] }]);
  const sub = subsetField(f, [4, 0, 1]);
  assert.equal(reduceField(sub, "total").value, 8);
  assert.equal(reduceField(sub, "count").value, 3);
  assert.equal(reduceField(sub, "first").value, 5);
  assert.equal(reduceField(subsetField(f, []), "total").value, 0);
  const st = subsetField(t, [1, 2, 3]);
  assert.equal(reduceField(st, "distinct_count").value, 1);
  assert.equal(reduceField(st, "last_not_null").value, "y");
  assert.equal(reduceField(st, "count").value, 3);
});
