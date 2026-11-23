// The items of a stat, a bar gauge or a pie chart give the mode palette the place of the field (9.16, ID-K74), and a
// continuous colour mode the range of all number fields together.
import test from "node:test";
import assert from "node:assert/strict";
import { makeDisplay } from "../../web/js/display.js";
import { computeFieldConfig } from "../../web/js/fields.js";
import { baseDisplayNames, displayNames } from "../../web/js/names.js";
import { reduceItems } from "../../web/js/panels/items.js";
import { QCHART } from "../../web/js/colors.js";
import { toFields } from "./helpers.mjs";

function base(cols, mode) {
  const fields = computeFieldConfig(toFields(cols), { defaults: { color: { mode } }, overrides: [] }, "stat").fields;
  return { fields, names: displayNames(fields), baseNames: baseDisplayNames(fields), zone: "utc" };
}

test("items: the mode palette colours each field item by the place of the field", () => {
  const b = base([{ name: "reads", kind: "number", values: [1] }, { name: "writes", kind: "number", values: [2] }], "palette");
  const items = reduceItems(b, { calc: "last", fields: null, values: false });
  assert.equal(items.length, 2);
  for (const [k, item] of items.entries()) {
    assert.equal(item.color, makeDisplay(b.fields[k], { zone: "utc", seriesIndex: k }).valueDisplay(item.value).color);
    assert.equal(item.color, QCHART.dark[k]);
  }
  assert.notEqual(items[0].color, items[1].color);
});

test("items: the mode palette gives the row items of one field the colour of their field", () => {
  const b = base([{ name: "host", kind: "text", values: ["alpha", "beta", "gamma"] }, { name: "cpu", kind: "number", values: [1, 2, 3] }], "palette");
  const items = reduceItems(b, { calc: "last", fields: null, values: true });
  assert.deepEqual(items.map((i) => i.name), ["alpha", "beta", "gamma"]);
  assert.equal(new Set(items.map((i) => i.color)).size, 1, "one field, one palette slot");
});

test("items: a continuous mode uses the range of all number fields together", () => {
  const b = base([{ name: "low", kind: "number", values: [0] }, { name: "high", kind: "number", values: [100] }], "continuous_green_yellow_red");
  const items = reduceItems(b, { calc: "last", fields: null, values: false });
  // With the shared range 0..100, the field `low` (0) has the colour of the first stop and `high` (100) the colour of the
  // last stop. With a range of its own, each field would have min = max and the colour of the first stop.
  assert.notEqual(items[0].color, items[1].color);
  assert.equal(items[1].color, makeDisplay(b.fields[1], { zone: "utc", range: { min: 0, max: 100 } }).valueDisplay(100).color);
});
