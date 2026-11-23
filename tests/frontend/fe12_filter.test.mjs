// FE-12: filter_by_value (9.21).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { filterByValue, applyTransforms } from "../../web/js/transform.js";
import { toFields } from "./helpers.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-12-filter.json", import.meta.url)));

test("FE-12 matchers, modes and match", () => {
  for (const c of v.cases) {
    const out = filterByValue(toFields(v.cols), c.spec);
    assert.deepEqual(out[0].values, c.expect, c.name);
    assert.equal(out[1].values.length, c.expect.length, c.name);
  }
});

test("FE-12 NaN never passes an order test; transforms apply in order", () => {
  const out = filterByValue(toFields(v.nan.cols), v.nan.spec);
  assert.deepEqual(Array.from(out[0].values), v.nan.expect.map(() => 1));
  const twice = applyTransforms(toFields(v.cols), [v.cases[0].spec, v.cases[6].spec]);
  assert.deepEqual(twice[0].values, ["a", "d"]);
});
