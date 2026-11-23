// FE-74: reduce and partition_by_values (9.21a, rules 8 and 9).
import test from "node:test";
import assert from "node:assert/strict";
import { measure, measureRun, run, vectors } from "./more_helpers.mjs";

test("FE-74 vectors of the file FE-74-reduce-partition.json", () => {
  const v = vectors("FE-74-reduce-partition.json");
  for (const c of v.cases) assert.deepEqual(run(c.frames, c.transform), c.expect, c.name);
});

test("FE-74 the cases of the measure of Grafana: same, or a difference that 19.20j lists", () => {
  const cases = measure.filter((c) => /^(re|pv)\d+$/.test(c.id));
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

