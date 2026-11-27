// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-10: reduce calculations (9.19).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { reduceField, selectReduceFields } from "../../web/js/reduce.js";
import { CALCS } from "../../web/js/editor/forms/transform_more_model.js";
import { baseDisplayNames } from "../../web/js/names.js";
import { toFields } from "./helpers.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-10-reduce.json", import.meta.url)));

test("FE-10 calculations", () => {
  const fields = toFields(v.cols);
  assert.equal(CALCS.length, 18);
  for (const c of v.cases) {
    const got = reduceField(fields[c.field], c.calc).value;
    if (typeof c.value === "number" && !Number.isInteger(c.value)) assert.ok(Math.abs(got - c.value) < 1e-9, `${c.field} ${c.calc}`);
    else assert.equal(got, c.value, `${v.cols[c.field].name} ${c.calc}`);
  }
});

test("FE-10 mean and total of exact text use the doubles", () => {
  const fields = toFields(v.cols);
  assert.equal(reduceField(fields[2], "total").num, v.nums["2:total"]);
  assert.equal(reduceField(fields[2], "mean").num, v.nums["2:mean"]);
  assert.equal(reduceField(fields[2], "max").num, 9007199254740992);
});

test("FE-10 field selection by pattern on base display names", () => {
  const fields = toFields(v.cols);
  const names = baseDisplayNames(fields);
  for (const c of v.select) assert.deepEqual(selectReduceFields(fields, names, c.fields), c.expect, String(c.fields));
});

test("FE-10 delta: the example of 9.19", async () => {
  const { deltaOf } = await import("../../web/js/reduce.js");
  assert.equal(deltaOf([1, 2, 3, 1, 2, 5]), 5);
  assert.equal(deltaOf([]), 0);
  assert.equal(deltaOf([7]), 0);
  assert.equal(deltaOf([5, 4, 3]), 0);
});

test("FE-10 diff percent divides as JavaScript does", () => {
  const fields = toFields([{ name: "z", kind: "number", values: [0, 5] }, { name: "w", kind: "number", values: [0, 0] }, { name: "m", kind: "number", values: [0, -5] }]);
  assert.equal(reduceField(fields[0], "diff_percent").value, Infinity);
  assert.ok(Number.isNaN(reduceField(fields[1], "diff_percent").value));
  assert.equal(reduceField(fields[2], "diff_percent").value, -Infinity);
});
