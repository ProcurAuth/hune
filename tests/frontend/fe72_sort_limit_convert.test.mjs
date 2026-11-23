// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-72: sort_by, limit and convert_field_type (9.21a, rules 4, 5 and 12).
import test from "node:test";
import assert from "node:assert/strict";
import { measure, measureRun, run, vectors } from "./more_helpers.mjs";

test("FE-72 vectors of the file FE-72-sort-limit-convert.json", () => {
  const v = vectors("FE-72-sort-limit-convert.json");
  for (const c of v.cases) assert.deepEqual(run(c.frames, c.transform), c.expect, c.name);
});

test("FE-72 the cases of the measure of Grafana: same, or a difference that 19.20j lists", () => {
  const cases = measure.filter((c) => /^(s|l|ct)\d+$/.test(c.id));
  assert.ok(cases.length > 0);
  for (const c of cases) {
    if (c.class === "not_mapped") continue;
    const got = measureRun(c);
    if (c.class === "same") assert.deepEqual(got, c.grafana, c.id);
    else {
      assert.deepEqual(got, c.got, c.id);
      assert.notDeepEqual(got, c.grafana, c.id);
    }
  }
});

