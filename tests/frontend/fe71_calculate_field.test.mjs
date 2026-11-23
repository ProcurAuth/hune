// FE-71: calculate_field (9.21a, rule 3).
import test from "node:test";
import assert from "node:assert/strict";
import { measure, measureRun, run, vectors } from "./more_helpers.mjs";

test("FE-71 vectors of the file FE-71-calculate-field.json", () => {
  const v = vectors("FE-71-calculate-field.json");
  for (const c of v.cases) assert.deepEqual(run(c.frames, c.transform), c.expect, c.name);
});

test("FE-71 the cases of the measure of Grafana: same, or a difference that 19.20j lists", () => {
  const cases = measure.filter((c) => /^(c|r|u|i)\d+b?$/.test(c.id));
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

test("FE-71 the error of a text operand names the field", () => {
  const v = vectors("FE-71-calculate-field.json");
  const c = v.cases.find((x) => x.name === "a text operand is an error");
  assert.equal(run(c.frames, c.transform).error, "The transform calculate_field failed: the field host is not a number");
});
